use anyhow::Context;
use sea_orm::{ColumnTrait, EntityTrait, ExprTrait, QueryFilter};
use serde::{Deserialize, Serialize};
use summer::plugin::Service;
use summer_sea_orm::DbConn;

use summer_ai_core::types::common::Usage;
use summer_ai_model::entity::model_config;
use summer_ai_model::entity::token;
use summer_common::error::{ApiErrors, ApiResult};

use crate::service::runtime_cache::RuntimeCacheService;
use crate::service::runtime_ops::RuntimeOpsService;
use crate::service::token::TokenInfo;

const MODEL_CONFIG_CACHE_TTL_SECONDS: u64 = 300;
const GROUP_RATIO_CACHE_TTL_SECONDS: u64 = 300;
const BILLING_RECORD_TTL_SECONDS: u64 = 24 * 60 * 60;

/// Pricing ratios for one model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfigInfo {
    pub model_name: String,
    pub input_ratio: f64,
    pub output_ratio: f64,
    pub cached_input_ratio: f64,
    pub reasoning_ratio: f64,
    pub supported_endpoints: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
enum BillingReservationStatus {
    /// Intent recorded in Redis; DB deduction has NOT yet been confirmed.
    Pending,
    /// DB deduction succeeded and Redis record is authoritative.
    Reserved,
    Settled,
    Refunded,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BillingReservationRecord {
    request_id: String,
    token_id: i64,
    pre_consumed: i64,
    status: BillingReservationStatus,
    actual_quota: Option<i64>,
}

#[derive(Clone, Service)]
pub struct BillingEngine {
    #[inject(component)]
    db: DbConn,
    #[inject(component)]
    cache: RuntimeCacheService,
    #[inject(component)]
    runtime_ops: RuntimeOpsService,
}

impl BillingEngine {
    /// Reserve quota synchronously before sending the upstream request.
    ///
    /// Uses a three-phase approach to prevent quota loss on crash:
    ///   1. Write a **Pending** record to Redis (intent to deduct)
    ///   2. Execute atomic DB deduction:
    ///      `UPDATE ai.token SET remain_quota = remain_quota - ? WHERE id = ? AND remain_quota >= ?`
    ///   3. Upgrade the Redis record to **Reserved**
    ///
    /// If the process crashes between steps 2 and 3, the Pending record
    /// remains in Redis and can be reconciled on startup.
    pub async fn pre_consume(
        &self,
        request_id: &str,
        token_info: &TokenInfo,
        estimated_tokens: i32,
        model_input_ratio: f64,
        group_ratio: f64,
    ) -> ApiResult<i64> {
        let record_key = billing_record_key(request_id);

        // Idempotency: if a record already exists, return its pre_consumed value.
        if let Some(record) = self
            .cache
            .get_json::<BillingReservationRecord>(&record_key)
            .await?
        {
            // If we find a Pending record from an earlier attempt that crashed
            // before completing the DB deduction, clean it up and proceed.
            if record.status != BillingReservationStatus::Pending {
                return Ok(record.pre_consumed);
            }
            // Pending record exists from a prior crash — delete it and retry below.
            if let Err(e) = self.cache.delete(&record_key).await {
                tracing::warn!(error = %e, "failed to delete stale pending billing record");
            }
        }

        if token_info.unlimited_quota {
            let record = BillingReservationRecord {
                request_id: request_id.to_string(),
                token_id: token_info.token_id,
                pre_consumed: 0,
                status: BillingReservationStatus::Reserved,
                actual_quota: None,
            };
            self.cache
                .set_json(&record_key, &record, BILLING_RECORD_TTL_SECONDS)
                .await?;
            return Ok(0);
        }

        let quota = (estimated_tokens as f64 * model_input_ratio * group_ratio).ceil() as i64;
        let quota = Ord::max(quota, 1);

        // Phase 1: Write Pending record to Redis (marks intent to deduct).
        let pending_record = BillingReservationRecord {
            request_id: request_id.to_string(),
            token_id: token_info.token_id,
            pre_consumed: quota,
            status: BillingReservationStatus::Pending,
            actual_quota: None,
        };
        let inserted = self
            .cache
            .set_json_if_absent(&record_key, &pending_record, BILLING_RECORD_TTL_SECONDS)
            .await
            .map_err(|e| {
                tracing::error!("failed to write billing pending record: {e}");
                e
            })?;
        if !inserted {
            // Another request with the same ID is in flight — read its state.
            if let Some(existing) = self
                .cache
                .get_json::<BillingReservationRecord>(&record_key)
                .await?
            {
                return Ok(existing.pre_consumed);
            }
        }

        // Phase 2: Atomic DB deduction.
        use sea_orm::sea_query::Expr;
        let result = token::Entity::update_many()
            .col_expr(
                token::Column::RemainQuota,
                Expr::col(token::Column::RemainQuota).sub(quota),
            )
            .filter(token::Column::Id.eq(token_info.token_id))
            .filter(token::Column::RemainQuota.gte(quota))
            .exec(&self.db)
            .await
            .context("failed to reserve quota")
            .map_err(|e| {
                // DB failed — clean up the Pending record so quota is not orphaned.
                let cache = self.cache.clone();
                let key = record_key.clone();
                tokio::spawn(async move {
                    if let Err(e) = cache.delete(&key).await {
                        tracing::warn!(error = %e, "failed to delete billing record after DB error");
                    }
                });
                ApiErrors::Internal(e)
            })?;

        if result.rows_affected == 0 {
            // Insufficient quota — remove the Pending record.
            if let Err(e) = self.cache.delete(&record_key).await {
                tracing::warn!(error = %e, "failed to delete pending billing record after insufficient quota");
            }
            return Err(ApiErrors::Forbidden("quota exceeded".into()));
        }

        // Phase 3: Upgrade to Reserved (DB already deducted).
        let reserved_record = BillingReservationRecord {
            request_id: request_id.to_string(),
            token_id: token_info.token_id,
            pre_consumed: quota,
            status: BillingReservationStatus::Reserved,
            actual_quota: None,
        };
        if let Err(error) = self
            .cache
            .set_json(&record_key, &reserved_record, BILLING_RECORD_TTL_SECONDS)
            .await
        {
            // Redis failed to upgrade Pending→Reserved. The DB deduction is
            // already committed. Log a critical warning — the Pending record
            // will remain and can be reconciled on startup or by post_consume.
            tracing::error!(
                "CRITICAL: DB quota deducted but failed to upgrade billing record to Reserved: {error}"
            );
        }

        Ok(quota)
    }

    /// Calculate the final quota from actual usage.
    pub fn calculate_actual_quota(
        usage: &Usage,
        model_config: &ModelConfigInfo,
        group_ratio: f64,
    ) -> i64 {
        let base = usage.prompt_tokens as f64 * model_config.input_ratio
            + usage.completion_tokens as f64 * model_config.output_ratio
            + usage.cached_tokens as f64 * model_config.cached_input_ratio
            + usage.reasoning_tokens as f64 * model_config.reasoning_ratio;

        (base * group_ratio).ceil() as i64
    }

    /// Settle quota asynchronously after the upstream request finishes.
    ///
    /// actual_quota = prompt * input_ratio + completion * output_ratio
    ///              + cached * cached_ratio + reasoning * reasoning_ratio
    /// delta = actual - pre_consumed
    pub async fn post_consume(
        &self,
        request_id: &str,
        token_info: &TokenInfo,
        pre_consumed: i64,
        usage: &Usage,
        model_config: &ModelConfigInfo,
        group_ratio: f64,
    ) -> ApiResult<i64> {
        let actual_quota = Self::calculate_actual_quota(usage, model_config, group_ratio);
        let record_key = billing_record_key(request_id);
        let mut record = self
            .cache
            .get_json::<BillingReservationRecord>(&record_key)
            .await?
            .unwrap_or(BillingReservationRecord {
                request_id: request_id.to_string(),
                token_id: token_info.token_id,
                pre_consumed,
                status: BillingReservationStatus::Reserved,
                actual_quota: None,
            });

        if record.status == BillingReservationStatus::Settled {
            return Ok(record.actual_quota.unwrap_or(actual_quota));
        }

        use sea_orm::sea_query::Expr;
        let mut update = token::Entity::update_many().col_expr(
            token::Column::UsedQuota,
            Expr::col(token::Column::UsedQuota).add(actual_quota),
        );

        if !token_info.unlimited_quota {
            let quota_delta = match record.status {
                BillingReservationStatus::Pending | BillingReservationStatus::Reserved => {
                    actual_quota - record.pre_consumed
                }
                BillingReservationStatus::Refunded => actual_quota,
                BillingReservationStatus::Settled => 0,
            };
            if quota_delta != 0 {
                update = update.col_expr(
                    token::Column::RemainQuota,
                    Expr::col(token::Column::RemainQuota).sub(quota_delta),
                );
            }
        }

        update
            .filter(token::Column::Id.eq(record.token_id))
            .exec(&self.db)
            .await
            .context("post settlement failed")
            .map_err(ApiErrors::Internal)?;

        record.status = BillingReservationStatus::Settled;
        record.actual_quota = Some(actual_quota);
        self.cache
            .set_json(&record_key, &record, BILLING_RECORD_TTL_SECONDS)
            .await?;

        Ok(actual_quota)
    }

    pub async fn post_consume_with_retry(
        &self,
        request_id: &str,
        token_info: &TokenInfo,
        pre_consumed: i64,
        usage: &Usage,
        model_config: &ModelConfigInfo,
        group_ratio: f64,
    ) -> ApiResult<i64> {
        let result = self
            .retry_async(|| {
                self.post_consume(
                    request_id,
                    token_info,
                    pre_consumed,
                    usage,
                    model_config,
                    group_ratio,
                )
            })
            .await;
        if result.is_err() {
            self.runtime_ops.record_settlement_failure_async();
        }
        result
    }

    /// Schedule a refund after a terminal failure.
    pub fn refund_later(&self, request_id: String, token_id: i64, pre_consumed: i64) {
        if pre_consumed <= 0 {
            return;
        }

        let this = self.clone();
        tokio::spawn(async move {
            if let Err(error) = this
                .refund_with_retry(&request_id, token_id, pre_consumed)
                .await
            {
                tracing::warn!("failed to refund reserved quota asynchronously: {error}");
            }
        });
    }

    /// Refund previously reserved quota after a failed request.
    pub async fn refund(
        &self,
        request_id: &str,
        token_id: i64,
        pre_consumed: i64,
    ) -> ApiResult<()> {
        let record_key = billing_record_key(request_id);
        let mut record = self
            .cache
            .get_json::<BillingReservationRecord>(&record_key)
            .await?
            .unwrap_or(BillingReservationRecord {
                request_id: request_id.to_string(),
                token_id,
                pre_consumed,
                status: BillingReservationStatus::Reserved,
                actual_quota: None,
            });

        if record.status == BillingReservationStatus::Settled
            || record.status == BillingReservationStatus::Refunded
        {
            return Ok(());
        }

        self.apply_refund_amount(record.token_id, record.pre_consumed)
            .await?;
        record.status = BillingReservationStatus::Refunded;
        self.cache
            .set_json(&record_key, &record, BILLING_RECORD_TTL_SECONDS)
            .await?;
        self.runtime_ops.record_refund_async();
        Ok(())
    }

    pub async fn refund_with_retry(
        &self,
        request_id: &str,
        token_id: i64,
        pre_consumed: i64,
    ) -> ApiResult<()> {
        let result = self
            .retry_async(|| self.refund(request_id, token_id, pre_consumed))
            .await;
        if result.is_err() {
            self.runtime_ops.record_settlement_failure_async();
        }
        result
    }

    /// Load pricing ratios for one model.
    pub async fn get_model_config(&self, model_name: &str) -> ApiResult<ModelConfigInfo> {
        let cache_key = model_config_cache_key(model_name);
        if let Some(config) = self.cache.get_json::<ModelConfigInfo>(&cache_key).await? {
            return Ok(config);
        }

        let cfg = model_config::Entity::find()
            .filter(model_config::Column::ModelName.eq(model_name))
            .one(&self.db)
            .await
            .context("failed to query model config")
            .map_err(ApiErrors::Internal)?;

        let config = match cfg {
            Some(c) => {
                let to_f64 = |bd: sea_orm::entity::prelude::BigDecimal| -> f64 {
                    bd.to_string().parse().unwrap_or(1.0)
                };
                ModelConfigInfo {
                    model_name: c.model_name,
                    input_ratio: to_f64(c.input_ratio),
                    output_ratio: to_f64(c.output_ratio),
                    cached_input_ratio: to_f64(c.cached_input_ratio),
                    reasoning_ratio: to_f64(c.reasoning_ratio),
                    supported_endpoints: json_string_list(&c.supported_endpoints),
                }
            }
            None => {
                return Err(ApiErrors::BadRequest(format!(
                    "model is not available: {model_name}"
                )));
            }
        };

        if let Err(e) = self
            .cache
            .set_json(&cache_key, &config, MODEL_CONFIG_CACHE_TTL_SECONDS)
            .await
        {
            tracing::warn!(error = %e, "failed to cache model config");
        }

        Ok(config)
    }

    pub async fn get_model_config_for_endpoint(
        &self,
        model_name: &str,
        endpoint_scope: &str,
    ) -> ApiResult<ModelConfigInfo> {
        let config = self.get_model_config(model_name).await?;
        config.ensure_endpoint_supported(endpoint_scope)?;
        Ok(config)
    }

    /// Load the pricing ratio for one token group.
    pub async fn get_group_ratio(&self, group_code: &str) -> ApiResult<f64> {
        use summer_ai_model::entity::group_ratio;

        let cache_key = group_ratio_cache_key(group_code);
        if let Some(ratio) = self.cache.get_json::<f64>(&cache_key).await? {
            return Ok(ratio);
        }

        let gr = group_ratio::Entity::find()
            .filter(group_ratio::Column::GroupCode.eq(group_code))
            .filter(group_ratio::Column::Enabled.eq(true))
            .one(&self.db)
            .await
            .context("failed to query group ratio")
            .map_err(ApiErrors::Internal)?;

        let ratio = match gr {
            Some(g) => g.ratio.to_string().parse().unwrap_or(1.0),
            None => 1.0,
        };

        if let Err(e) = self
            .cache
            .set_json(&cache_key, &ratio, GROUP_RATIO_CACHE_TTL_SECONDS)
            .await
        {
            tracing::warn!(error = %e, "failed to cache group ratio");
        }

        Ok(ratio)
    }

    async fn apply_refund_amount(&self, token_id: i64, amount: i64) -> ApiResult<()> {
        if amount <= 0 {
            return Ok(());
        }

        use sea_orm::sea_query::Expr;
        token::Entity::update_many()
            .col_expr(
                token::Column::RemainQuota,
                Expr::col(token::Column::RemainQuota).add(amount),
            )
            .filter(token::Column::Id.eq(token_id))
            .exec(&self.db)
            .await
            .context("refund reserved quota failed")
            .map_err(ApiErrors::Internal)?;
        Ok(())
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

impl ModelConfigInfo {
    pub fn ensure_endpoint_supported(&self, endpoint_scope: &str) -> ApiResult<()> {
        if endpoint_scope.is_empty()
            || self
                .supported_endpoints
                .iter()
                .any(|supported| supported == endpoint_scope)
        {
            return Ok(());
        }

        Err(ApiErrors::BadRequest(format!(
            "model {} does not support endpoint: {endpoint_scope}",
            self.model_name
        )))
    }
}

/// Roughly estimate prompt tokens for pre-reservation.
pub fn estimate_prompt_tokens(messages: &[summer_ai_core::types::common::Message]) -> i32 {
    // Heuristic: ~3 characters per token on average (balances ASCII ~4:1 and CJK ~1.5:1).
    // Uses char count instead of byte count for consistent cross-language estimation.
    let total_chars: usize = messages
        .iter()
        .map(|m| match &m.content {
            serde_json::Value::String(s) => s.chars().count(),
            other => other.to_string().chars().count(),
        })
        .sum();
    (total_chars as f64 / 3.0).ceil() as i32
}

pub fn estimate_total_tokens_for_rate_limit(
    messages: &[summer_ai_core::types::common::Message],
    max_tokens: Option<i64>,
) -> i64 {
    let prompt_tokens = i64::from(estimate_prompt_tokens(messages));
    prompt_tokens + std::cmp::Ord::max(max_tokens.unwrap_or(2048), 1)
}

pub fn model_config_cache_key(model_name: &str) -> String {
    format!("ai:cache:model-config:{model_name}")
}

pub fn group_ratio_cache_key(group_code: &str) -> String {
    format!("ai:cache:group-ratio:{group_code}")
}

fn json_string_list(value: &serde_json::Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(ToOwned::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn billing_record_key(request_id: &str) -> String {
    format!("ai:billing:req:{request_id}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use summer_ai_core::types::common::{Message, Usage};

    fn sample_model_config(supported_endpoints: &[&str]) -> ModelConfigInfo {
        ModelConfigInfo {
            model_name: "gpt-5.4".into(),
            input_ratio: 1.0,
            output_ratio: 1.0,
            cached_input_ratio: 0.0,
            reasoning_ratio: 0.0,
            supported_endpoints: supported_endpoints
                .iter()
                .map(|endpoint| (*endpoint).to_string())
                .collect(),
        }
    }

    fn make_message(content: &str) -> Message {
        Message {
            role: "user".into(),
            content: serde_json::Value::String(content.into()),
            name: None,
            tool_calls: None,
            tool_call_id: None,
        }
    }

    // ── endpoint support ───────────────────────────────────────────

    #[test]
    fn endpoint_support_check_accepts_supported_scope() {
        let config = sample_model_config(&["chat", "responses"]);
        assert!(config.ensure_endpoint_supported("responses").is_ok());
    }

    #[test]
    fn endpoint_support_check_rejects_missing_scope() {
        let config = sample_model_config(&["chat"]);
        let error = config.ensure_endpoint_supported("responses").unwrap_err();
        assert!(matches!(error, ApiErrors::BadRequest(_)));
        assert!(error.to_string().contains("responses"));
    }

    // ── calculate_actual_quota ─────────────────────────────────────

    #[test]
    fn actual_quota_basic_input_output() {
        let usage = Usage {
            prompt_tokens: 100,
            completion_tokens: 50,
            total_tokens: 150,
            cached_tokens: 0,
            reasoning_tokens: 0,
        };
        let config = ModelConfigInfo {
            model_name: "gpt-4o".into(),
            input_ratio: 2.5,
            output_ratio: 10.0,
            cached_input_ratio: 0.0,
            reasoning_ratio: 0.0,
            supported_endpoints: vec![],
        };

        // (100 * 2.5 + 50 * 10.0) * 1.0 = 750
        assert_eq!(
            BillingEngine::calculate_actual_quota(&usage, &config, 1.0),
            750
        );
    }

    #[test]
    fn actual_quota_with_cached_and_reasoning_tokens() {
        let usage = Usage {
            prompt_tokens: 100,
            completion_tokens: 50,
            total_tokens: 150,
            cached_tokens: 30,
            reasoning_tokens: 20,
        };
        let config = ModelConfigInfo {
            model_name: "claude".into(),
            input_ratio: 3.0,
            output_ratio: 15.0,
            cached_input_ratio: 1.5,
            reasoning_ratio: 15.0,
            supported_endpoints: vec![],
        };

        // (100*3 + 50*15 + 30*1.5 + 20*15) * 1.0 = (300+750+45+300) = 1395
        assert_eq!(
            BillingEngine::calculate_actual_quota(&usage, &config, 1.0),
            1395
        );
    }

    #[test]
    fn actual_quota_applies_group_ratio() {
        let usage = Usage {
            prompt_tokens: 100,
            completion_tokens: 100,
            total_tokens: 200,
            cached_tokens: 0,
            reasoning_tokens: 0,
        };
        let config = ModelConfigInfo {
            model_name: "test".into(),
            input_ratio: 1.0,
            output_ratio: 1.0,
            cached_input_ratio: 0.0,
            reasoning_ratio: 0.0,
            supported_endpoints: vec![],
        };

        // (100 + 100) * 0.5 = 100
        assert_eq!(
            BillingEngine::calculate_actual_quota(&usage, &config, 0.5),
            100
        );
        // (100 + 100) * 2.0 = 400
        assert_eq!(
            BillingEngine::calculate_actual_quota(&usage, &config, 2.0),
            400
        );
    }

    #[test]
    fn actual_quota_ceils_fractional_result() {
        let usage = Usage {
            prompt_tokens: 1,
            completion_tokens: 0,
            total_tokens: 1,
            cached_tokens: 0,
            reasoning_tokens: 0,
        };
        let config = ModelConfigInfo {
            model_name: "test".into(),
            input_ratio: 0.3,
            output_ratio: 0.0,
            cached_input_ratio: 0.0,
            reasoning_ratio: 0.0,
            supported_endpoints: vec![],
        };

        // 1 * 0.3 * 1.0 = 0.3 → ceil = 1
        assert_eq!(
            BillingEngine::calculate_actual_quota(&usage, &config, 1.0),
            1
        );
    }

    // ── estimate_prompt_tokens ─────────────────────────────────────

    #[test]
    fn estimate_tokens_english_text() {
        let messages = vec![make_message("Hello, how are you today?")];
        let estimate = estimate_prompt_tokens(&messages);
        // 25 chars / 3.0 = 8.33 → ceil = 9
        assert_eq!(estimate, 9);
    }

    #[test]
    fn estimate_tokens_chinese_text() {
        let messages = vec![make_message("你好世界")];
        let estimate = estimate_prompt_tokens(&messages);
        // 4 chars / 3.0 = 1.33 → ceil = 2
        assert_eq!(estimate, 2);
    }

    #[test]
    fn estimate_tokens_multiple_messages() {
        let messages = vec![
            make_message("Hi"),          // 2 chars
            make_message("Hello world"), // 11 chars
        ];
        let estimate = estimate_prompt_tokens(&messages);
        // 13 / 3.0 = 4.33 → ceil = 5
        assert_eq!(estimate, 5);
    }

    #[test]
    fn estimate_tokens_empty_messages() {
        let messages: Vec<Message> = vec![];
        assert_eq!(estimate_prompt_tokens(&messages), 0);
    }

    // ── estimate_total_tokens_for_rate_limit ───────────────────────

    #[test]
    fn total_token_estimate_uses_max_tokens_when_provided() {
        let messages = vec![make_message("test")]; // ~2 tokens
        let total = estimate_total_tokens_for_rate_limit(&messages, Some(500));
        assert_eq!(total, 2 + 500);
    }

    #[test]
    fn total_token_estimate_defaults_to_2048() {
        let messages = vec![make_message("test")]; // ~2 tokens
        let total = estimate_total_tokens_for_rate_limit(&messages, None);
        assert_eq!(total, 2 + 2048);
    }

    // ── BillingReservationStatus transitions ───────────────────────

    #[test]
    fn billing_reservation_status_serde_roundtrip() {
        for status in [
            BillingReservationStatus::Pending,
            BillingReservationStatus::Reserved,
            BillingReservationStatus::Settled,
            BillingReservationStatus::Refunded,
        ] {
            let json = serde_json::to_string(&status).unwrap();
            let back: BillingReservationStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(status, back);
        }
    }
}
