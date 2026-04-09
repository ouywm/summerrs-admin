use std::time::Instant;

use summer_ai_billing::service::channel_model_price::ResolvedModelPrice;
use summer_ai_billing::service::engine::BillingEngine;
use summer_ai_core::types::common::Usage;

use crate::service::log::{FailureLogRecord, LogService, UsageLogRecord};
use crate::service::shared::stream::driver::BoxFinalizeFuture;
use crate::service::token::TokenInfo;
use crate::service::tracking::{
    TrackingService, build_request_trace_failure_metadata, build_request_trace_success_metadata,
};

#[derive(Clone)]
pub(crate) struct UsageStreamBillingSnapshot {
    pub(crate) token_id: i64,
    pub(crate) unlimited_quota: bool,
    pub(crate) group_ratio: f64,
    pub(crate) pre_consumed: i64,
    pub(crate) estimated_prompt_tokens: i32,
    pub(crate) price: ResolvedModelPrice,
}

#[derive(Clone)]
pub(crate) struct UsageStreamLogSnapshot {
    pub(crate) token_info: TokenInfo,
    pub(crate) channel_id: i64,
    pub(crate) channel_name: String,
    pub(crate) account_id: i64,
    pub(crate) account_name: String,
    pub(crate) execution_id: i64,
    pub(crate) requested_model: String,
    pub(crate) client_ip: String,
    pub(crate) user_agent: String,
}

#[derive(Clone)]
pub(crate) struct UsageStreamFinalizeMeta {
    pub(crate) endpoint: &'static str,
    pub(crate) request_format: &'static str,
    pub(crate) request_id: String,
    pub(crate) requested_model: String,
    pub(crate) upstream_model: String,
    pub(crate) upstream_request_id: Option<String>,
    pub(crate) response_status_code: i32,
}

impl UsageStreamFinalizeMeta {
    pub(crate) fn new(
        endpoint: &'static str,
        request_format: &'static str,
        request_id: String,
        requested_model: String,
        upstream_model: String,
        upstream_request_id: Option<String>,
        response_status_code: i32,
    ) -> Self {
        Self {
            endpoint,
            request_format,
            request_id,
            requested_model,
            upstream_model,
            upstream_request_id,
            response_status_code,
        }
    }
}

#[derive(Clone)]
pub(crate) struct UsageStreamFinalizeContext {
    tracking: Option<TrackingService>,
    billing: Option<BillingEngine>,
    billing_context: Option<UsageStreamBillingSnapshot>,
    log: Option<LogService>,
    log_context: Option<UsageStreamLogSnapshot>,
    trace_id: Option<i64>,
    tracked_request_id: Option<i64>,
    tracked_execution_id: Option<i64>,
    started_at: Instant,
    meta: UsageStreamFinalizeMeta,
}

impl UsageStreamFinalizeContext {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        meta: UsageStreamFinalizeMeta,
        started_at: Instant,
        tracking: Option<TrackingService>,
        billing: Option<BillingEngine>,
        billing_context: Option<UsageStreamBillingSnapshot>,
        log: Option<LogService>,
        log_context: Option<UsageStreamLogSnapshot>,
        trace_id: Option<i64>,
        tracked_request_id: Option<i64>,
        tracked_execution_id: Option<i64>,
    ) -> Self {
        Self {
            tracking,
            billing,
            billing_context,
            log,
            log_context,
            trace_id,
            tracked_request_id,
            tracked_execution_id,
            started_at,
            meta,
        }
    }

    #[cfg(test)]
    pub(crate) fn without_services(meta: UsageStreamFinalizeMeta) -> Self {
        Self {
            tracking: None,
            billing: None,
            billing_context: None,
            log: None,
            log_context: None,
            trace_id: None,
            tracked_request_id: None,
            tracked_execution_id: None,
            started_at: Instant::now(),
            meta,
        }
    }

    pub(crate) fn meta(&self) -> &UsageStreamFinalizeMeta {
        &self.meta
    }

    pub(crate) fn billing_context(&self) -> Option<&UsageStreamBillingSnapshot> {
        self.billing_context.as_ref()
    }

    pub(crate) fn started_at(&self) -> Instant {
        self.started_at
    }
}

#[derive(Clone, Debug)]
pub(crate) enum UsageStreamFinalizeSettlement {
    Success {
        upstream_model: String,
        first_token_ms: i32,
        final_usage: Option<Usage>,
    },
    Failure {
        upstream_model: String,
        first_token_ms: i32,
        status_code: i32,
        message: String,
    },
}

impl UsageStreamFinalizeSettlement {
    pub(crate) fn success(
        upstream_model: String,
        first_token_ms: i32,
        final_usage: Option<Usage>,
    ) -> Self {
        Self::Success {
            upstream_model,
            first_token_ms,
            final_usage,
        }
    }

    pub(crate) fn failure(
        upstream_model: String,
        first_token_ms: i32,
        status_code: i32,
        message: String,
    ) -> Self {
        Self::Failure {
            upstream_model,
            first_token_ms,
            status_code,
            message,
        }
    }

    #[cfg(test)]
    pub(crate) fn upstream_model(&self) -> &str {
        match self {
            Self::Success { upstream_model, .. } | Self::Failure { upstream_model, .. } => {
                upstream_model.as_str()
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn first_token_ms(&self) -> i32 {
        match self {
            Self::Success { first_token_ms, .. } | Self::Failure { first_token_ms, .. } => {
                *first_token_ms
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn final_usage(&self) -> Option<&Usage> {
        match self {
            Self::Success { final_usage, .. } => final_usage.as_ref(),
            Self::Failure { .. } => None,
        }
    }
}

impl UsageStreamFinalizeContext {
    pub(crate) fn build_usage_finalize_future(
        &self,
        settlement: UsageStreamFinalizeSettlement,
    ) -> Option<BoxFinalizeFuture> {
        let tracking = self.tracking.clone()?;
        let billing = self.billing.clone();
        let billing_context = self.billing_context.clone();
        let log = self.log.clone();
        let log_context = self.log_context.clone();
        let tracked_request_id = self.tracked_request_id;
        let tracked_execution_id = self.tracked_execution_id;
        let trace_id = self.trace_id;
        let meta = self.meta.clone();
        let duration_ms = self.started_at.elapsed().as_millis() as i32;

        Some(Box::pin(async move {
            match settlement {
                UsageStreamFinalizeSettlement::Success {
                    upstream_model,
                    first_token_ms,
                    final_usage,
                } => {
                    if let (Some(billing), Some(billing_context)) =
                        (billing.clone(), billing_context.clone())
                    {
                        let result = if let Some(usage) = final_usage.as_ref() {
                            billing
                                .post_consume(
                                    billing_context.token_id,
                                    billing_context.unlimited_quota,
                                    billing_context.pre_consumed,
                                    usage,
                                    &billing_context.price,
                                    billing_context.group_ratio,
                                )
                                .await
                        } else {
                            billing
                                .settle_pre_consumed(
                                    billing_context.token_id,
                                    billing_context.unlimited_quota,
                                    billing_context.pre_consumed,
                                )
                                .await
                        };

                        if let Err(error) = result {
                            tracing::warn!(
                                request_id = meta.request_id,
                                error = %error,
                                "failed to finalize usage stream billing"
                            );
                        }
                    }

                    if let (Some(log), Some(log_context), Some(billing_context)) =
                        (log.clone(), log_context.clone(), billing_context.clone())
                    {
                        let usage = final_usage.clone().unwrap_or_default();
                        let quota = if let Some(actual_usage) = final_usage.as_ref() {
                            BillingEngine::calculate_actual_quota(
                                actual_usage,
                                &billing_context.price,
                                billing_context.group_ratio,
                            )
                        } else {
                            billing_context.pre_consumed
                        };

                        let record = UsageLogRecord {
                            channel_id: log_context.channel_id,
                            channel_name: log_context.channel_name.clone(),
                            account_id: log_context.account_id,
                            account_name: log_context.account_name.clone(),
                            execution_id: log_context.execution_id,
                            endpoint: meta.endpoint.into(),
                            request_format: meta.request_format.into(),
                            requested_model: log_context.requested_model.clone(),
                            upstream_model: upstream_model.clone(),
                            model_name: billing_context.price.model_name.clone(),
                            usage: usage.clone(),
                            quota,
                            cost_total: BillingEngine::calculate_cost_total(
                                &usage,
                                &billing_context.price,
                            ),
                            price_reference: billing_context.price.price_reference.clone(),
                            elapsed_time: duration_ms,
                            first_token_time: first_token_ms,
                            is_stream: true,
                            request_id: meta.request_id.clone(),
                            upstream_request_id: meta
                                .upstream_request_id
                                .clone()
                                .unwrap_or_default(),
                            status_code: meta.response_status_code,
                            client_ip: log_context.client_ip.clone(),
                            user_agent: log_context.user_agent.clone(),
                            content: String::new(),
                        };

                        if let Err(error) = log.record_usage(&log_context.token_info, record).await
                        {
                            tracing::warn!(
                                request_id = meta.request_id,
                                error = %error,
                                "failed to write usage stream log"
                            );
                        }
                    }

                    if let Some(request_pk) = tracked_request_id
                        && let Err(error) = tracking
                            .finish_request_stream_success(
                                request_pk,
                                &upstream_model,
                                meta.response_status_code,
                                duration_ms,
                                first_token_ms,
                            )
                            .await
                    {
                        tracing::warn!(
                            request_id = meta.request_id,
                            error = %error,
                            "failed to finalize request streaming success"
                        );
                    }

                    if let Some(request_pk) = tracked_request_id
                        && let Err(error) = tracking
                            .finish_request_trace_from_request_success(
                                request_pk,
                                &upstream_model,
                                meta.response_status_code,
                                duration_ms,
                                first_token_ms,
                            )
                            .await
                    {
                        tracing::warn!(
                            request_id = meta.request_id,
                            error = %error,
                            "failed to finalize trace streaming success"
                        );
                    }

                    if tracked_request_id.is_none()
                        && let Some(trace_id) = trace_id
                        && let Err(error) = tracking
                            .finish_trace_success(
                                trace_id,
                                build_request_trace_success_metadata(
                                    &meta.request_id,
                                    meta.endpoint,
                                    meta.request_format,
                                    &meta.requested_model,
                                    &upstream_model,
                                    true,
                                    meta.response_status_code,
                                    duration_ms,
                                    first_token_ms,
                                ),
                            )
                            .await
                    {
                        tracing::warn!(
                            request_id = meta.request_id,
                            error = %error,
                            "failed to finalize trace streaming success without request tracking"
                        );
                    }

                    if let Some(execution_id) = tracked_execution_id
                        && let Err(error) = tracking
                            .finish_execution_stream_success(
                                execution_id,
                                meta.upstream_request_id.as_deref(),
                                meta.response_status_code,
                                duration_ms,
                                first_token_ms,
                            )
                            .await
                    {
                        tracing::warn!(
                            request_id = meta.request_id,
                            error = %error,
                            "failed to finalize request_execution streaming success"
                        );
                    }

                    if let Some(execution_id) = tracked_execution_id
                        && let Err(error) = tracking
                            .finish_execution_trace_span_from_execution_success(
                                execution_id,
                                meta.upstream_request_id.as_deref(),
                                meta.response_status_code,
                                serde_json::json!({
                                    "usage": final_usage,
                                }),
                                duration_ms,
                                first_token_ms,
                            )
                            .await
                    {
                        tracing::warn!(
                            request_id = meta.request_id,
                            error = %error,
                            "failed to finalize trace span streaming success"
                        );
                    }
                }
                UsageStreamFinalizeSettlement::Failure {
                    upstream_model,
                    first_token_ms,
                    status_code,
                    message,
                } => {
                    if let (Some(billing), Some(billing_context)) =
                        (billing.clone(), billing_context.clone())
                        && let Err(error) = billing
                            .refund(billing_context.token_id, billing_context.pre_consumed)
                            .await
                    {
                        tracing::warn!(
                            request_id = meta.request_id,
                            error = %error,
                            "failed to refund usage stream billing reservation"
                        );
                    }

                    if let (Some(log), Some(log_context), Some(billing_context)) =
                        (log.clone(), log_context.clone(), billing_context.clone())
                    {
                        let record = FailureLogRecord {
                            channel_id: log_context.channel_id,
                            channel_name: log_context.channel_name.clone(),
                            account_id: log_context.account_id,
                            account_name: log_context.account_name.clone(),
                            execution_id: log_context.execution_id,
                            endpoint: meta.endpoint.into(),
                            request_format: meta.request_format.into(),
                            requested_model: log_context.requested_model.clone(),
                            upstream_model: upstream_model.clone(),
                            model_name: billing_context.price.model_name.clone(),
                            price_reference: billing_context.price.price_reference.clone(),
                            elapsed_time: duration_ms,
                            is_stream: true,
                            request_id: meta.request_id.clone(),
                            upstream_request_id: meta
                                .upstream_request_id
                                .clone()
                                .unwrap_or_default(),
                            status_code,
                            client_ip: log_context.client_ip.clone(),
                            user_agent: log_context.user_agent.clone(),
                            content: message.clone(),
                        };

                        if let Err(error) =
                            log.record_failure(&log_context.token_info, record).await
                        {
                            tracing::warn!(
                                request_id = meta.request_id,
                                error = %error,
                                "failed to write usage stream failure log"
                            );
                        }
                    }

                    if let Some(request_pk) = tracked_request_id
                        && let Err(error) = tracking
                            .finish_request_stream_failure(
                                request_pk,
                                Some(&upstream_model),
                                status_code,
                                &message,
                                None,
                                duration_ms,
                                first_token_ms,
                            )
                            .await
                    {
                        tracing::warn!(
                            request_id = meta.request_id,
                            error = %error,
                            "failed to finalize request streaming failure"
                        );
                    }

                    if let Some(request_pk) = tracked_request_id
                        && let Err(error) = tracking
                            .finish_request_trace_from_request_failure(
                                request_pk,
                                Some(&upstream_model),
                                status_code,
                                &message,
                                duration_ms,
                                first_token_ms,
                            )
                            .await
                    {
                        tracing::warn!(
                            request_id = meta.request_id,
                            error = %error,
                            "failed to finalize trace streaming failure"
                        );
                    }

                    if tracked_request_id.is_none()
                        && let Some(trace_id) = trace_id
                        && let Err(error) = tracking
                            .finish_trace_failure(
                                trace_id,
                                build_request_trace_failure_metadata(
                                    &meta.request_id,
                                    meta.endpoint,
                                    meta.request_format,
                                    &meta.requested_model,
                                    Some(&upstream_model),
                                    true,
                                    status_code,
                                    &message,
                                    duration_ms,
                                    first_token_ms,
                                ),
                            )
                            .await
                    {
                        tracing::warn!(
                            request_id = meta.request_id,
                            error = %error,
                            "failed to finalize trace streaming failure without request tracking"
                        );
                    }

                    if let Some(execution_id) = tracked_execution_id
                        && let Err(error) = tracking
                            .finish_execution_stream_failure(
                                execution_id,
                                meta.upstream_request_id.as_deref(),
                                status_code,
                                &message,
                                None,
                                duration_ms,
                                first_token_ms,
                            )
                            .await
                    {
                        tracing::warn!(
                            request_id = meta.request_id,
                            error = %error,
                            "failed to finalize request_execution streaming failure"
                        );
                    }

                    if let Some(execution_id) = tracked_execution_id
                        && let Err(error) = tracking
                            .finish_execution_trace_span_from_execution_failure(
                                execution_id,
                                meta.upstream_request_id.as_deref(),
                                status_code,
                                &message,
                                serde_json::json!({}),
                                duration_ms,
                                first_token_ms,
                            )
                            .await
                    {
                        tracing::warn!(
                            request_id = meta.request_id,
                            error = %error,
                            "failed to finalize trace span streaming failure"
                        );
                    }
                }
            }
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        UsageStreamFinalizeContext, UsageStreamFinalizeMeta, UsageStreamFinalizeSettlement,
    };

    #[test]
    fn success_settlement_uses_provided_upstream_model() {
        let settlement = UsageStreamFinalizeSettlement::success("gpt-5.4".to_string(), 12, None);

        assert_eq!(settlement.upstream_model(), "gpt-5.4");
        assert_eq!(settlement.first_token_ms(), 12);
        assert!(settlement.final_usage().is_none());
    }

    #[test]
    fn finalize_meta_preserves_endpoint_shape() {
        let meta = UsageStreamFinalizeMeta::new(
            "/v1/chat/completions",
            "openai/chat_completions",
            "req_test".to_string(),
            "gpt-5.4".to_string(),
            "gpt-5.4".to_string(),
            Some("up_123".to_string()),
            200,
        );

        let context = UsageStreamFinalizeContext::without_services(meta);

        assert_eq!(context.meta().endpoint, "/v1/chat/completions");
        assert_eq!(context.meta().request_format, "openai/chat_completions");
        assert_eq!(context.meta().request_id, "req_test");
    }
}
