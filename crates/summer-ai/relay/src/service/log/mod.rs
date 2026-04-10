use sea_orm::Set;
use sea_orm::prelude::BigDecimal;
use summer::plugin::Service;
use summer_common::error::{ApiErrors, ApiResult};
use summer_plugins::log_batch_collector::{AiLogCollector, LogBatchPushError};

use summer_ai_core::types::common::Usage;
use summer_ai_model::entity::log::{self, LogStatus, LogType};

use crate::service::token::TokenInfo;

pub struct UsageLogRecord {
    pub channel_id: i64,
    pub channel_name: String,
    pub account_id: i64,
    pub account_name: String,
    pub execution_id: i64,
    pub endpoint: String,
    pub request_format: String,
    pub requested_model: String,
    pub upstream_model: String,
    pub model_name: String,
    pub usage: Usage,
    pub quota: i64,
    pub cost_total: BigDecimal,
    pub price_reference: String,
    pub elapsed_time: i32,
    pub first_token_time: i32,
    pub is_stream: bool,
    pub request_id: String,
    pub upstream_request_id: String,
    pub status_code: i32,
    pub client_ip: String,
    pub user_agent: String,
    pub content: String,
}

pub struct FailureLogRecord {
    pub channel_id: i64,
    pub channel_name: String,
    pub account_id: i64,
    pub account_name: String,
    pub execution_id: i64,
    pub endpoint: String,
    pub request_format: String,
    pub requested_model: String,
    pub upstream_model: String,
    pub model_name: String,
    pub price_reference: String,
    pub elapsed_time: i32,
    pub is_stream: bool,
    pub request_id: String,
    pub upstream_request_id: String,
    pub status_code: i32,
    pub client_ip: String,
    pub user_agent: String,
    pub content: String,
}

#[derive(Clone, Service)]
pub struct LogService {
    #[inject(component)]
    collector: AiLogCollector,
}

impl LogService {
    pub async fn record_usage(
        &self,
        token_info: &TokenInfo,
        record: UsageLogRecord,
    ) -> ApiResult<()> {
        self.collector
            .push(Self::build_usage_log_active_model(token_info, record))
            .map_err(map_push_error)
    }

    pub async fn record_failure(
        &self,
        token_info: &TokenInfo,
        record: FailureLogRecord,
    ) -> ApiResult<()> {
        self.collector
            .push(Self::build_failure_log_active_model(token_info, record))
            .map_err(map_push_error)
    }

    fn build_usage_log_active_model(
        token_info: &TokenInfo,
        record: UsageLogRecord,
    ) -> log::ActiveModel {
        let now = chrono::Utc::now().fixed_offset();
        let dedupe_key = request_final_dedupe_key(&record.request_id);
        log::ActiveModel {
            user_id: Set(token_info.user_id),
            token_id: Set(token_info.token_id),
            token_name: Set(token_info.name.clone()),
            project_id: Set(token_info.project_id),
            conversation_id: Set(0),
            message_id: Set(0),
            session_id: Set(0),
            thread_id: Set(0),
            trace_id: Set(0),
            channel_id: Set(record.channel_id),
            channel_name: Set(record.channel_name),
            account_id: Set(record.account_id),
            account_name: Set(record.account_name),
            execution_id: Set(record.execution_id),
            endpoint: Set(record.endpoint),
            request_format: Set(record.request_format),
            requested_model: Set(record.requested_model),
            upstream_model: Set(record.upstream_model),
            model_name: Set(record.model_name),
            prompt_tokens: Set(record.usage.prompt_tokens),
            completion_tokens: Set(record.usage.completion_tokens),
            total_tokens: Set(record.usage.total_tokens),
            cached_tokens: Set(record.usage.cached_tokens),
            reasoning_tokens: Set(record.usage.reasoning_tokens),
            quota: Set(record.quota),
            cost_total: Set(record.cost_total),
            price_reference: Set(record.price_reference),
            elapsed_time: Set(record.elapsed_time),
            first_token_time: Set(record.first_token_time),
            is_stream: Set(record.is_stream),
            request_id: Set(record.request_id),
            dedupe_key: Set(dedupe_key),
            upstream_request_id: Set(record.upstream_request_id),
            status_code: Set(record.status_code),
            client_ip: Set(record.client_ip),
            user_agent: Set(record.user_agent),
            content: Set(record.content),
            log_type: Set(LogType::Consumption),
            status: Set(LogStatus::Succeeded),
            create_time: Set(now),
            ..Default::default()
        }
    }

    fn build_failure_log_active_model(
        token_info: &TokenInfo,
        record: FailureLogRecord,
    ) -> log::ActiveModel {
        Self::build_usage_log_active_model(
            token_info,
            UsageLogRecord {
                channel_id: record.channel_id,
                channel_name: record.channel_name,
                account_id: record.account_id,
                account_name: record.account_name,
                execution_id: record.execution_id,
                endpoint: record.endpoint,
                request_format: record.request_format,
                requested_model: record.requested_model,
                upstream_model: record.upstream_model,
                model_name: record.model_name,
                usage: Usage::default(),
                quota: 0,
                cost_total: BigDecimal::from(0),
                price_reference: record.price_reference,
                elapsed_time: record.elapsed_time,
                first_token_time: 0,
                is_stream: record.is_stream,
                request_id: record.request_id,
                upstream_request_id: record.upstream_request_id,
                status_code: record.status_code,
                client_ip: record.client_ip,
                user_agent: record.user_agent,
                content: record.content,
            },
        )
        .tap_mut(|active| {
            active.status = Set(LogStatus::Failed);
        })
    }
}

trait TapMut: Sized {
    fn tap_mut(mut self, f: impl FnOnce(&mut Self)) -> Self {
        f(&mut self);
        self
    }
}

impl<T> TapMut for T {}

fn request_final_dedupe_key(request_id: &str) -> String {
    if request_id.trim().is_empty() {
        String::new()
    } else {
        format!("req:{request_id}:final")
    }
}

fn map_push_error(error: LogBatchPushError) -> ApiErrors {
    match error {
        LogBatchPushError::Full => ApiErrors::ServiceUnavailable("AI 日志批量队列已满".to_string()),
        LogBatchPushError::Closed => {
            ApiErrors::ServiceUnavailable("AI 日志批量队列已关闭".to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use sea_orm::Set;
    use summer_ai_billing::service::channel_model_price::ResolvedModelPrice;
    use summer_ai_billing::service::engine::BillingEngine;
    use summer_ai_core::types::common::Usage;
    use summer_ai_model::entity::channel_model_price::ChannelModelPriceBillingMode;
    use summer_ai_model::entity::log::{LogStatus, LogType};

    use crate::service::token::TokenInfo;

    use super::{FailureLogRecord, LogService, UsageLogRecord};

    #[test]
    fn usage_log_active_model_carries_price_reference() {
        let active = LogService::build_usage_log_active_model(
            &sample_token(),
            UsageLogRecord {
                channel_id: 101,
                channel_name: "primary".into(),
                account_id: 202,
                account_name: "acct-a".into(),
                execution_id: 303,
                endpoint: "/v1/chat/completions".into(),
                request_format: "openai/chat_completions".into(),
                requested_model: "gpt-5.4".into(),
                upstream_model: "gpt-5.4".into(),
                model_name: "gpt-5.4".into(),
                usage: Usage {
                    prompt_tokens: 10,
                    completion_tokens: 5,
                    total_tokens: 15,
                    cached_tokens: 2,
                    reasoning_tokens: 3,
                },
                quota: 123,
                cost_total: BillingEngine::calculate_cost_total(
                    &Usage {
                        prompt_tokens: 10,
                        completion_tokens: 5,
                        total_tokens: 15,
                        cached_tokens: 2,
                        reasoning_tokens: 3,
                    },
                    &sample_price(),
                ),
                price_reference: "cmp_live_001".into(),
                elapsed_time: 456,
                first_token_time: 78,
                is_stream: true,
                request_id: "req_123".into(),
                upstream_request_id: "up_123".into(),
                status_code: 200,
                client_ip: "127.0.0.1".into(),
                user_agent: "codex-test".into(),
                content: String::new(),
            },
        );

        assert_eq!(active.price_reference, Set("cmp_live_001".to_string()));
        assert_eq!(active.log_type, Set(LogType::Consumption));
        assert_eq!(active.status, Set(LogStatus::Succeeded));
        assert_eq!(active.dedupe_key, Set("req:req_123:final".to_string()));
        assert_eq!(
            active.cost_total,
            Set("138.0000000000".parse().expect("cost total"))
        );
        assert!(matches!(active.create_time, Set(_)));
    }

    #[test]
    fn failure_log_active_model_keeps_price_reference_when_available() {
        let active = LogService::build_failure_log_active_model(
            &sample_token(),
            FailureLogRecord {
                channel_id: 101,
                channel_name: "primary".into(),
                account_id: 202,
                account_name: "acct-a".into(),
                execution_id: 303,
                endpoint: "/v1/chat/completions".into(),
                request_format: "openai/chat_completions".into(),
                requested_model: "gpt-5.4".into(),
                upstream_model: "gpt-5.4".into(),
                model_name: "gpt-5.4".into(),
                price_reference: "cmp_live_001".into(),
                elapsed_time: 456,
                is_stream: false,
                request_id: "req_123".into(),
                upstream_request_id: "up_123".into(),
                status_code: 502,
                client_ip: "127.0.0.1".into(),
                user_agent: "codex-test".into(),
                content: "upstream failed".into(),
            },
        );

        assert_eq!(active.price_reference, Set("cmp_live_001".to_string()));
        assert_eq!(active.log_type, Set(LogType::Consumption));
        assert_eq!(active.status, Set(LogStatus::Failed));
        assert_eq!(active.quota, Set(0));
        assert_eq!(active.dedupe_key, Set("req:req_123:final".to_string()));
    }

    fn sample_token() -> TokenInfo {
        TokenInfo {
            token_id: 1,
            user_id: 2,
            project_id: 3,
            service_account_id: 4,
            name: "demo-token".into(),
            group: "default".into(),
            remain_quota: 100,
            unlimited_quota: false,
            rpm_limit: 0,
            tpm_limit: 0,
            concurrency_limit: 0,
            allowed_models: vec![],
            endpoint_scopes: vec![],
        }
    }

    fn sample_price() -> ResolvedModelPrice {
        ResolvedModelPrice {
            model_name: "gpt-5.4".into(),
            billing_mode: ChannelModelPriceBillingMode::ByToken,
            currency: "USD".into(),
            input_ratio: 2.5,
            output_ratio: 10.0,
            cached_input_ratio: 1.5,
            reasoning_ratio: 20.0,
            supported_endpoints: vec!["chat".into()],
            price_reference: "cmp_live_001".into(),
        }
    }
}
