use futures::StreamExt;
use summer_web::axum::response::sse::{Event, KeepAlive, Sse};
use summer_web::axum::response::{IntoResponse, Response};

use summer_ai_core::provider::{ProviderErrorKind, ProviderStreamError};
use summer_ai_core::types::chat::ChatCompletionChunk;
use summer_ai_core::types::common::Usage;

use crate::relay::billing::{BillingEngine, ModelConfigInfo};
use crate::relay::channel_router::SelectedChannel;
use crate::relay::rate_limit::RateLimitEngine;
use crate::router::openai::settle_usage_accounting;
use crate::service::channel::ChannelService;
use crate::service::log::LogService;
use crate::service::request::{ExecutionStatusUpdate, RequestService, RequestStatusUpdate};
use crate::service::token::TokenInfo;

use crate::service::metrics::relay_metrics;

/// Convert the upstream chunk stream into an axum SSE response.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_sse_response(
    chunk_stream: futures::stream::BoxStream<'static, anyhow::Result<ChatCompletionChunk>>,
    token_info: TokenInfo,
    pre_consumed: i64,
    model_config: ModelConfigInfo,
    group_ratio: f64,
    channel: SelectedChannel,
    requested_model: String,
    start_elapsed: i64,
    client_ip: String,
    log_svc: LogService,
    channel_svc: ChannelService,
    rate_limiter: RateLimitEngine,
    billing: BillingEngine,
    request_id: String,
    upstream_request_id: String,
    user_agent: String,
    request_svc: RequestService,
    tracking: crate::router::openai::RequestTrackingIds,
) -> Response {
    let response_request_id = request_id.clone();
    let stream = async_stream::stream! {
        let metrics = relay_metrics();
        metrics.record_stream_start();

        let mut last_usage: Option<Usage> = None;
        let mut saw_terminal_finish_reason = false;
        let mut first_token_time: Option<i64> = None;
        let start = std::time::Instant::now();
        let mut upstream_model = String::new();
        let mut stream_error: Option<anyhow::Error> = None;

        tokio::pin!(chunk_stream);
        while let Some(result) = chunk_stream.next().await {
            match result {
                Ok(chunk) => {
                    if first_token_time.is_none() && !chunk.choices.is_empty() {
                        first_token_time = Some(start.elapsed().as_millis() as i64);
                    }
                    if chunk
                        .choices
                        .iter()
                        .any(|choice| choice.finish_reason.is_some())
                    {
                        saw_terminal_finish_reason = true;
                    }
                    if chunk.usage.is_some() {
                        last_usage = chunk.usage.clone();
                    }
                    if upstream_model.is_empty() {
                        upstream_model = chunk.model.clone();
                    }
                    match serde_json::to_string(&chunk) {
                        Ok(json) => yield Ok::<_, std::convert::Infallible>(Event::default().data(json)),
                        Err(e) => {
                            tracing::error!("failed to serialize chat chunk: {e}");
                            break;
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Stream error: {e}");
                    stream_error = Some(e);
                    break;
                }
            }
        }

        yield Ok(Event::default().data("[DONE]"));

        metrics.record_stream_end();
        if let Some(ftt_ms) = first_token_time {
            metrics.record_first_token_latency(ftt_ms as u64);
        }
        if let Some(ref usage) = last_usage {
            metrics.record_tokens(usage.prompt_tokens, usage.completion_tokens, usage.cached_tokens);
        }

        let total_elapsed = start_elapsed + start.elapsed().as_millis() as i64;
        let ftt = first_token_time.unwrap_or(0) as i32;

        match resolve_stream_settlement(
            last_usage.clone(),
            saw_terminal_finish_reason,
            stream_error.as_ref(),
        ) {
            StreamSettlement::Success { usage } => {
                request_svc
                    .try_update_execution_status(
                        tracking.execution_id,
                        ExecutionStatusUpdate {
                            status: summer_ai_model::entity::request_execution::ExecutionStatus::Success,
                            error_message: None,
                            duration_ms: Some(total_elapsed as i32),
                            first_token_ms: Some(ftt),
                            response_status_code: Some(200),
                            response_body: None,
                            upstream_request_id: Some(upstream_request_id.clone()),
                        },
                    )
                    .await;
                request_svc
                    .try_update_request_status(
                        tracking.request_id,
                        RequestStatusUpdate {
                            status: summer_ai_model::entity::request::RequestStatus::Success,
                            error_message: None,
                            duration_ms: Some(total_elapsed as i32),
                            first_token_ms: Some(ftt),
                            response_status_code: Some(200),
                            response_body: None,
                            upstream_model: (!upstream_model.is_empty()).then(|| upstream_model.clone()),
                        },
                    )
                    .await;
                settle_usage_accounting(
                    billing,
                    rate_limiter,
                    log_svc,
                    channel_svc,
                    token_info,
                    channel,
                    model_config,
                    group_ratio,
                    pre_consumed,
                    usage,
                    request_id,
                    upstream_request_id,
                    requested_model,
                    upstream_model,
                    client_ip,
                    user_agent,
                    "chat/completions",
                    "openai/chat_completions",
                    total_elapsed,
                    ftt,
                    true,
                )
                .await;
            }
            StreamSettlement::Failure { status_code, message } => {
                request_svc
                    .try_update_execution_status(
                        tracking.execution_id,
                        ExecutionStatusUpdate {
                            status: summer_ai_model::entity::request_execution::ExecutionStatus::Failed,
                            error_message: Some(message.clone()),
                            duration_ms: Some(total_elapsed as i32),
                            first_token_ms: Some(ftt),
                            response_status_code: Some(status_code),
                            response_body: None,
                            upstream_request_id: Some(upstream_request_id.clone()),
                        },
                    )
                    .await;
                request_svc
                    .try_update_request_status(
                        tracking.request_id,
                        RequestStatusUpdate {
                            status: summer_ai_model::entity::request::RequestStatus::Failed,
                            error_message: Some(message.clone()),
                            duration_ms: Some(total_elapsed as i32),
                            first_token_ms: Some(ftt),
                            response_status_code: Some(status_code),
                            response_body: None,
                            upstream_model: (!upstream_model.is_empty()).then(|| upstream_model.clone()),
                        },
                    )
                    .await;
                if let Err(error) = billing
                    .refund_with_retry(&request_id, token_info.token_id, pre_consumed)
                    .await
                {
                    tracing::warn!("failed to refund stream reservation: {error}");
                }
                if let Err(error) = rate_limiter.finalize_failure_with_retry(&request_id).await {
                    tracing::warn!("failed to finalize stream rate limit failure: {error}");
                }
                if let Err(error) = channel_svc
                    .record_relay_failure(
                        channel.channel_id,
                        channel.account_id,
                        total_elapsed,
                        status_code,
                        &message,
                    )
                    .await
                {
                    tracing::warn!("failed to update stream relay failure health state: {error}");
                }
            }
        }
    };

    let mut response = Sse::new(stream)
        .keep_alive(
            KeepAlive::new()
                .interval(std::time::Duration::from_secs(15))
                .text("keepalive"),
        )
        .into_response();
    if let Ok(value) = summer_web::axum::http::HeaderValue::from_str(&response_request_id) {
        response.headers_mut().insert("x-request-id", value);
    }
    response
}

fn stream_error_health_status_code(error: &anyhow::Error) -> i32 {
    error
        .downcast_ref::<ProviderStreamError>()
        .map(|error| match error.info.kind {
            ProviderErrorKind::InvalidRequest => 400,
            ProviderErrorKind::Authentication => 401,
            ProviderErrorKind::RateLimit => 429,
            ProviderErrorKind::Server | ProviderErrorKind::Api => 502,
        })
        .unwrap_or(0)
}

fn stream_error_health_message(error: &anyhow::Error) -> String {
    error
        .downcast_ref::<ProviderStreamError>()
        .map(|error| error.info.message.clone())
        .unwrap_or_else(|| error.to_string())
}

#[derive(Debug, Clone)]
enum StreamSettlement {
    Success { usage: Usage },
    Failure { status_code: i32, message: String },
}

fn resolve_stream_settlement(
    last_usage: Option<Usage>,
    saw_terminal_finish_reason: bool,
    stream_error: Option<&anyhow::Error>,
) -> StreamSettlement {
    if let Some(error) = stream_error {
        return StreamSettlement::Failure {
            status_code: stream_error_health_status_code(error),
            message: stream_error_health_message(error),
        };
    }

    match (last_usage, saw_terminal_finish_reason) {
        (Some(usage), true) => StreamSettlement::Success { usage },
        (Some(usage), false) if usage.completion_tokens > 0 => {
            // Some providers (Ollama, certain OpenAI-compatible proxies) omit
            // finish_reason but still deliver complete content.  If we received
            // usage with completion tokens, treat the stream as successful.
            tracing::debug!(
                "stream settled without finish_reason but completion_tokens={}, treating as success",
                usage.completion_tokens
            );
            StreamSettlement::Success { usage }
        }
        (Some(usage), false) => {
            // Usage was reported but no finish_reason arrived — likely a provider quirk
            // or a client-side disconnect after the final content chunk.  Bill for the
            // actual usage instead of triggering a full refund.
            tracing::info!(
                "stream settled with usage (prompt={}, completion={}) but no finish_reason; billing actual usage",
                usage.prompt_tokens,
                usage.completion_tokens,
            );
            StreamSettlement::Success { usage }
        }
        (None, _) => StreamSettlement::Failure {
            status_code: 0,
            message: "stream ended without usage".into(),
        },
    }
}

#[cfg(test)]
mod tests {
    use summer_ai_core::provider::{ProviderErrorInfo, ProviderErrorKind, ProviderStreamError};

    use super::{
        StreamSettlement, resolve_stream_settlement, stream_error_health_message,
        stream_error_health_status_code,
    };
    use summer_ai_core::types::common::Usage;

    #[test]
    fn stream_error_health_status_code_uses_provider_error_kind() {
        let error = anyhow::Error::new(ProviderStreamError::new(ProviderErrorInfo::new(
            ProviderErrorKind::InvalidRequest,
            "bad tool schema",
            "invalid_request_error",
        )));
        assert_eq!(stream_error_health_status_code(&error), 400);

        let error = anyhow::Error::new(ProviderStreamError::new(ProviderErrorInfo::new(
            ProviderErrorKind::RateLimit,
            "slow down",
            "rate_limit_error",
        )));
        assert_eq!(stream_error_health_status_code(&error), 429);
    }

    #[test]
    fn stream_error_health_message_prefers_provider_message() {
        let error = anyhow::Error::new(ProviderStreamError::new(ProviderErrorInfo::new(
            ProviderErrorKind::InvalidRequest,
            "bad tool schema",
            "invalid_request_error",
        )));
        assert_eq!(stream_error_health_message(&error), "bad tool schema");

        let error = anyhow::anyhow!("plain stream failure");
        assert_eq!(stream_error_health_message(&error), "plain stream failure");
    }

    #[test]
    fn resolve_stream_settlement_succeeds_without_finish_reason_when_completion_tokens_present() {
        let settlement = resolve_stream_settlement(
            Some(Usage {
                prompt_tokens: 10,
                completion_tokens: 5,
                total_tokens: 15,
                cached_tokens: 0,
                reasoning_tokens: 0,
            }),
            false,
            None,
        );

        assert!(matches!(settlement, StreamSettlement::Success { .. }));
    }

    #[test]
    fn resolve_stream_settlement_succeeds_with_usage_but_no_finish_reason_and_zero_completion() {
        let settlement = resolve_stream_settlement(
            Some(Usage {
                prompt_tokens: 10,
                completion_tokens: 0,
                total_tokens: 10,
                cached_tokens: 0,
                reasoning_tokens: 0,
            }),
            false,
            None,
        );

        // Even with zero completion tokens, if usage is reported we bill for actual usage
        // instead of triggering a full refund + overload penalty.
        assert!(matches!(settlement, StreamSettlement::Success { .. }));
    }

    #[test]
    fn resolve_stream_settlement_prefers_provider_error_over_usage() {
        let error = anyhow::Error::new(ProviderStreamError::new(ProviderErrorInfo::new(
            ProviderErrorKind::InvalidRequest,
            "bad tool schema",
            "invalid_request_error",
        )));

        let settlement = resolve_stream_settlement(
            Some(Usage {
                prompt_tokens: 10,
                completion_tokens: 5,
                total_tokens: 15,
                cached_tokens: 0,
                reasoning_tokens: 0,
            }),
            false,
            Some(&error),
        );

        assert!(matches!(
            settlement,
            StreamSettlement::Failure {
                status_code: 400,
                ..
            }
        ));
    }

    #[test]
    fn resolve_stream_settlement_succeeds_only_on_clean_terminal_stream() {
        let settlement = resolve_stream_settlement(
            Some(Usage {
                prompt_tokens: 10,
                completion_tokens: 5,
                total_tokens: 15,
                cached_tokens: 0,
                reasoning_tokens: 0,
            }),
            true,
            None,
        );

        assert!(matches!(settlement, StreamSettlement::Success { .. }));
    }
}
