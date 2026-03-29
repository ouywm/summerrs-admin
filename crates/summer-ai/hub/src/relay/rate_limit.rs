use serde::{Deserialize, Serialize};
use summer::plugin::Service;
use summer_common::error::{ApiErrors, ApiResult};

use crate::service::runtime_cache::RuntimeCacheService;
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
            let current = self.cache.incr(&key).await?;
            if current == 1 {
                self.cache
                    .expire(&key, RATE_LIMIT_BUCKET_TTL_SECONDS)
                    .await?;
            }
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
            let current = self.cache.incr_by(&key, reserved_tokens).await?;
            if current == reserved_tokens {
                self.cache
                    .expire(&key, RATE_LIMIT_BUCKET_TTL_SECONDS)
                    .await?;
            }
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
            let current = self.cache.incr(&key).await?;
            self.cache.expire(&key, CONCURRENCY_KEY_TTL_SECONDS).await?;
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
                let current = self.cache.incr_by(tpm_key, delta).await?;
                if current == delta {
                    self.cache
                        .expire(tpm_key, RATE_LIMIT_BUCKET_TTL_SECONDS)
                        .await?;
                }
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
        retry_async(|| self.finalize_success(request_id, actual_total_tokens)).await
    }

    pub async fn finalize_failure_with_retry(&self, request_id: &str) -> ApiResult<()> {
        retry_async(|| self.finalize_failure(request_id)).await
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

async fn retry_async<T, F, Fut>(mut f: F) -> ApiResult<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = ApiResult<T>>,
{
    let mut backoff_ms = 200_u64;
    let mut last_error = None;

    for _ in 0..3 {
        match f().await {
            Ok(value) => return Ok(value),
            Err(error) => {
                last_error = Some(error);
                tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
                backoff_ms *= 2;
            }
        }
    }

    Err(last_error.unwrap_or_else(|| {
        ApiErrors::Internal(anyhow::anyhow!(
            "retry operation failed without a captured error"
        ))
    }))
}
