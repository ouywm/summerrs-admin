use serde::{Deserialize, Serialize};
use summer::plugin::Service;
use summer_common::error::{ApiErrors, ApiResult};

use crate::service::runtime_cache::RuntimeCacheService;
use crate::service::runtime_ops::RuntimeOpsService;
use crate::service::token::TokenInfo;

const REQUEST_RECORD_TTL_SECONDS: u64 = 24 * 60 * 60;
const RATE_LIMIT_BUCKET_TTL_SECONDS: i64 = 120;
const CONCURRENCY_KEY_TTL_SECONDS: i64 = 600;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RateLimitRequestRecord {
    request_id: String,
    token_id: i64,
    rpm_key: Option<String>,
    rpm_reserved: i64,
    tpm_key: Option<String>,
    tpm_reserved: i64,
    concurrency_key: Option<String>,
    concurrency_reserved: i64,
    finalized: bool,
    final_total_tokens: Option<i64>,
}

#[derive(Clone, Service)]
pub struct RateLimitEngine {
    #[inject(component)]
    cache: RuntimeCacheService,
    #[inject(component)]
    runtime_ops: RuntimeOpsService,
}

impl RateLimitEngine {
    pub async fn reserve(
        &self,
        token_info: &TokenInfo,
        request_id: &str,
        estimated_total_tokens: i64,
    ) -> ApiResult<()> {
        let record_key = request_record_key(request_id);
        if self
            .cache
            .get_json::<RateLimitRequestRecord>(&record_key)
            .await?
            .is_some()
        {
            return Ok(());
        }

        let window = current_minute_window();
        let mut record = RateLimitRequestRecord {
            request_id: request_id.to_string(),
            token_id: token_info.token_id,
            rpm_key: None,
            rpm_reserved: 0,
            tpm_key: None,
            tpm_reserved: 0,
            concurrency_key: None,
            concurrency_reserved: 0,
            finalized: false,
            final_total_tokens: None,
        };

        if token_info.rpm_limit > 0 {
            let key = rpm_key(token_info.token_id, window);
            let current = self
                .cache
                .incr_with_expire(&key, RATE_LIMIT_BUCKET_TTL_SECONDS)
                .await?;
            if current > i64::from(token_info.rpm_limit) {
                let _ = self.cache.decr_by(&key, 1).await;
                return Err(ApiErrors::TooManyRequests("RPM limit exceeded".into()));
            }
            record.rpm_key = Some(key);
            record.rpm_reserved = 1;
        }

        if token_info.tpm_limit > 0 {
            let reserved_tokens = estimated_total_tokens.max(1);
            let key = tpm_key(token_info.token_id, window);
            let current = self
                .cache
                .incr_by_with_expire(&key, reserved_tokens, RATE_LIMIT_BUCKET_TTL_SECONDS)
                .await?;
            if current > token_info.tpm_limit {
                let _ = self.cache.decr_by(&key, reserved_tokens).await;
                if let Some(rpm_key) = record.rpm_key.as_ref() {
                    let _ = self.cache.decr_by(rpm_key, record.rpm_reserved).await;
                }
                return Err(ApiErrors::TooManyRequests("TPM limit exceeded".into()));
            }
            record.tpm_key = Some(key);
            record.tpm_reserved = reserved_tokens;
        }

        if token_info.concurrency_limit > 0 {
            let key = concurrency_key(token_info.token_id);
            let current = self
                .cache
                .incr_with_expire(&key, CONCURRENCY_KEY_TTL_SECONDS)
                .await?;
            if current > i64::from(token_info.concurrency_limit) {
                let _ = self.cache.decr_by(&key, 1).await;
                if let Some(tpm_key) = record.tpm_key.as_ref() {
                    let _ = self.cache.decr_by(tpm_key, record.tpm_reserved).await;
                }
                if let Some(rpm_key) = record.rpm_key.as_ref() {
                    let _ = self.cache.decr_by(rpm_key, record.rpm_reserved).await;
                }
                return Err(ApiErrors::TooManyRequests(
                    "concurrency limit exceeded".into(),
                ));
            }
            record.concurrency_key = Some(key);
            record.concurrency_reserved = 1;
        }

        let inserted = match self
            .cache
            .set_json_if_absent(&record_key, &record, REQUEST_RECORD_TTL_SECONDS)
            .await
        {
            Ok(inserted) => inserted,
            Err(error) => {
                self.rollback_reservation(&record).await;
                return Err(error);
            }
        };

        if !inserted {
            self.rollback_reservation(&record).await;
        }

        Ok(())
    }

    pub async fn finalize_success(
        &self,
        request_id: &str,
        actual_total_tokens: i64,
    ) -> ApiResult<()> {
        let record_key = request_record_key(request_id);
        let Some(mut record) = self
            .cache
            .get_json::<RateLimitRequestRecord>(&record_key)
            .await?
        else {
            return Ok(());
        };

        if record.finalized {
            return Ok(());
        }

        if let Some(tpm_key) = record.tpm_key.as_ref() {
            let delta = actual_total_tokens - record.tpm_reserved;
            if delta > 0 {
                let _ = self
                    .cache
                    .incr_by_with_expire(tpm_key, delta, RATE_LIMIT_BUCKET_TTL_SECONDS)
                    .await?;
            } else if delta < 0 {
                let _ = self.cache.decr_by(tpm_key, delta.abs()).await;
            }
        }

        if let Some(concurrency_key) = record.concurrency_key.as_ref()
            && record.concurrency_reserved > 0
        {
            let _ = self
                .cache
                .decr_by(concurrency_key, record.concurrency_reserved)
                .await;
        }

        record.finalized = true;
        record.final_total_tokens = Some(actual_total_tokens);
        self.cache
            .set_json(&record_key, &record, REQUEST_RECORD_TTL_SECONDS)
            .await
    }

    pub async fn finalize_failure(&self, request_id: &str) -> ApiResult<()> {
        let record_key = request_record_key(request_id);
        let Some(mut record) = self
            .cache
            .get_json::<RateLimitRequestRecord>(&record_key)
            .await?
        else {
            return Ok(());
        };

        if record.finalized {
            return Ok(());
        }

        if let Some(tpm_key) = record.tpm_key.as_ref()
            && record.tpm_reserved > 0
        {
            let _ = self.cache.decr_by(tpm_key, record.tpm_reserved).await;
        }

        if let Some(concurrency_key) = record.concurrency_key.as_ref()
            && record.concurrency_reserved > 0
        {
            let _ = self
                .cache
                .decr_by(concurrency_key, record.concurrency_reserved)
                .await;
        }

        record.finalized = true;
        record.final_total_tokens = Some(0);
        self.cache
            .set_json(&record_key, &record, REQUEST_RECORD_TTL_SECONDS)
            .await
    }

    pub async fn finalize_success_with_retry(
        &self,
        request_id: &str,
        actual_total_tokens: i64,
    ) -> ApiResult<()> {
        let result = self
            .retry_async(|| self.finalize_success(request_id, actual_total_tokens))
            .await;
        if result.is_err() {
            self.runtime_ops.record_settlement_failure_async();
        }
        result
    }

    pub async fn finalize_failure_with_retry(&self, request_id: &str) -> ApiResult<()> {
        let result = self.retry_async(|| self.finalize_failure(request_id)).await;
        if result.is_err() {
            self.runtime_ops.record_settlement_failure_async();
        }
        result
    }

    async fn rollback_reservation(&self, record: &RateLimitRequestRecord) {
        if let Some(concurrency_key) = record.concurrency_key.as_ref()
            && record.concurrency_reserved > 0
        {
            let _ = self
                .cache
                .decr_by(concurrency_key, record.concurrency_reserved)
                .await;
        }
        if let Some(tpm_key) = record.tpm_key.as_ref()
            && record.tpm_reserved > 0
        {
            let _ = self.cache.decr_by(tpm_key, record.tpm_reserved).await;
        }
        if let Some(rpm_key) = record.rpm_key.as_ref()
            && record.rpm_reserved > 0
        {
            let _ = self.cache.decr_by(rpm_key, record.rpm_reserved).await;
        }
    }

    async fn retry_async<T, F, Fut>(&self, mut f: F) -> ApiResult<T>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = ApiResult<T>>,
    {
        let mut backoff_ms = 200_u64;

        for attempt in 0..3 {
            match f().await {
                Ok(value) => return Ok(value),
                Err(error) => {
                    if attempt == 2 {
                        return Err(error);
                    }
                    self.runtime_ops.record_retry_async();
                    tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
                    backoff_ms *= 2;
                }
            }
        }

        Err(ApiErrors::Internal(anyhow::anyhow!(
            "retry operation failed without a captured error"
        )))
    }
}

fn request_record_key(request_id: &str) -> String {
    format!("ai:rate-limit:req:{request_id}")
}

fn rpm_key(token_id: i64, window: i64) -> String {
    format!("ai:rate-limit:rpm:{token_id}:{window}")
}

fn tpm_key(token_id: i64, window: i64) -> String {
    format!("ai:rate-limit:tpm:{token_id}:{window}")
}

fn concurrency_key(token_id: i64) -> String {
    format!("ai:rate-limit:concurrency:{token_id}")
}

fn current_minute_window() -> i64 {
    chrono::Utc::now().timestamp() / 60
}

#[cfg(test)]
mod tests {
    use super::*;
    use summer_redis::redis::AsyncCommands;

    const TEST_REDIS_URL: &str = "redis://127.0.0.1/";

    fn test_token_info(token_id: i64) -> TokenInfo {
        TokenInfo {
            token_id,
            user_id: 7,
            name: "rate-limit-test".into(),
            group: "default".into(),
            remain_quota: 1_000_000,
            unlimited_quota: false,
            rpm_limit: 10,
            tpm_limit: 100,
            concurrency_limit: 5,
            allowed_models: Vec::new(),
            endpoint_scopes: vec!["chat".into()],
        }
    }

    async fn test_engine() -> (RateLimitEngine, RuntimeCacheService, summer_redis::Redis) {
        let redis = summer_redis::redis::Client::open(
            std::env::var("REDIS_URL").unwrap_or_else(|_| TEST_REDIS_URL.to_string()),
        )
        .expect("create redis client")
        .get_connection_manager()
        .await
        .expect("connect redis");
        let cache = RuntimeCacheService::new(redis.clone());
        let runtime_ops = RuntimeOpsService::new(cache.clone());
        (
            RateLimitEngine {
                cache: cache.clone(),
                runtime_ops,
            },
            cache,
            redis,
        )
    }

    async fn ttl(redis: &summer_redis::Redis, key: &str) -> i64 {
        let mut conn = redis.clone();
        conn.ttl(key).await.expect("query ttl")
    }

    #[tokio::test]
    #[ignore = "requires local redis"]
    async fn reserve_reapplies_ttl_to_existing_rpm_and_tpm_keys_without_expiry() {
        let (engine, cache, redis) = test_engine().await;
        let token_info = test_token_info(42);
        let window = current_minute_window();
        let rpm = rpm_key(token_info.token_id, window);
        let rpm_next = rpm_key(token_info.token_id, window + 1);
        let tpm = tpm_key(token_info.token_id, window);
        let tpm_next = tpm_key(token_info.token_id, window + 1);
        let concurrency = concurrency_key(token_info.token_id);
        let request_id = "rate-limit-reserve-existing-ttl";

        let mut conn = redis.clone();
        conn.set::<_, _, ()>(&rpm, 1_i64)
            .await
            .expect("seed rpm key");
        conn.set::<_, _, ()>(&rpm_next, 1_i64)
            .await
            .expect("seed next rpm key");
        conn.set::<_, _, ()>(&tpm, 5_i64)
            .await
            .expect("seed tpm key");
        conn.set::<_, _, ()>(&tpm_next, 5_i64)
            .await
            .expect("seed next tpm key");
        conn.set::<_, _, ()>(&concurrency, 1_i64)
            .await
            .expect("seed concurrency key");

        cache
            .delete(&request_record_key(request_id))
            .await
            .expect("cleanup request record");

        engine
            .reserve(&token_info, request_id, 3)
            .await
            .expect("reserve rate limit");

        let record = cache
            .get_json::<RateLimitRequestRecord>(&request_record_key(request_id))
            .await
            .expect("load request record")
            .expect("request record exists");

        let actual_rpm = record.rpm_key.expect("rpm key in record");
        let actual_tpm = record.tpm_key.expect("tpm key in record");
        assert!(
            ttl(&redis, &actual_rpm).await > 0,
            "rpm key should have ttl"
        );
        assert!(
            ttl(&redis, &actual_tpm).await > 0,
            "tpm key should have ttl"
        );
        assert!(
            ttl(&redis, &concurrency).await > 0,
            "concurrency key should have ttl"
        );

        cache.delete(&rpm).await.expect("delete rpm key");
        cache.delete(&rpm_next).await.expect("delete next rpm key");
        cache.delete(&tpm).await.expect("delete tpm key");
        cache.delete(&tpm_next).await.expect("delete next tpm key");
        cache
            .delete(&concurrency)
            .await
            .expect("delete concurrency key");
        cache
            .delete(&request_record_key(request_id))
            .await
            .expect("delete request record");
    }

    #[tokio::test]
    #[ignore = "requires local redis"]
    async fn finalize_success_reapplies_ttl_to_existing_tpm_key_without_expiry() {
        let (engine, cache, redis) = test_engine().await;
        let token_info = test_token_info(43);
        let window = current_minute_window();
        let tpm = tpm_key(token_info.token_id, window);
        let request_id = "rate-limit-finalize-success-existing-ttl";

        let mut conn = redis.clone();
        conn.set::<_, _, ()>(&tpm, 5_i64)
            .await
            .expect("seed tpm key");

        cache
            .set_json(
                &request_record_key(request_id),
                &RateLimitRequestRecord {
                    request_id: request_id.into(),
                    token_id: token_info.token_id,
                    rpm_key: None,
                    rpm_reserved: 0,
                    tpm_key: Some(tpm.clone()),
                    tpm_reserved: 2,
                    concurrency_key: None,
                    concurrency_reserved: 0,
                    finalized: false,
                    final_total_tokens: None,
                },
                0,
            )
            .await
            .expect("seed request record");

        engine
            .finalize_success(request_id, 6)
            .await
            .expect("finalize success");

        assert!(ttl(&redis, &tpm).await > 0, "tpm key should have ttl");

        cache.delete(&tpm).await.expect("delete tpm key");
        cache
            .delete(&request_record_key(request_id))
            .await
            .expect("delete request record");
    }
}
