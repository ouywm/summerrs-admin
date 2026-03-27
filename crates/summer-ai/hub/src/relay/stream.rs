use futures::StreamExt;
use summer_web::axum::response::sse::{Event, KeepAlive, Sse};
use summer_web::axum::response::{IntoResponse, Response};

use summer_ai_core::provider::{ProviderErrorKind, ProviderStreamError};
use summer_ai_core::types::chat::ChatCompletionChunk;
use summer_ai_core::types::common::Usage;

use crate::relay::billing::{BillingEngine, ModelConfigInfo};
use crate::relay::channel_router::SelectedChannel;
use crate::relay::rate_limit::RateLimitEngine;
use crate::service::channel::ChannelService;
use crate::service::log::{ChatCompletionLogRecord, LogService};
use crate::service::token::TokenInfo;

/// Convert the upstream chunk stream into an axum SSE response.
#[allow(clippy::too_many_arguments)]
pub fn build_sse_response(
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
) -> Response {
    let response_request_id = request_id.clone();
    let stream = async_stream::stream! {
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

        let total_elapsed = start_elapsed + start.elapsed().as_millis() as i64;
        let ftt = first_token_time.unwrap_or(0) as i32;

        match resolve_stream_settlement(
            last_usage.clone(),
            saw_terminal_finish_reason,
            stream_error.as_ref(),
        ) {
            StreamSettlement::Success { usage } => {
            let usage_clone = usage.clone();
            tokio::spawn(async move {
                let logged_quota =
                    BillingEngine::calculate_actual_quota(&usage_clone, &model_config, group_ratio);
                let actual_quota = match billing
                    .post_consume_with_retry(
                        &request_id,
                        &token_info,
                        pre_consumed,
                        &usage_clone,
                        &model_config,
                        group_ratio,
                    )
                    .await
                {
                    Ok(quota) => quota,
                    Err(error) => {
                        tracing::error!("failed to settle stream usage asynchronously: {error}");
                        logged_quota
                    }
                };

                log_svc.record_chat_completion_async(
                    &token_info,
                    &channel,
                    &usage,
                    ChatCompletionLogRecord {
                        request_id: request_id.clone(),
                        upstream_request_id,
                        requested_model,
                        upstream_model,
                        model_name: model_config.model_name,
                        quota: actual_quota,
                        elapsed_time: total_elapsed as i32,
                        first_token_time: ftt,
                        is_stream: true,
                        client_ip,
                        user_agent,
                    },
                );

                if let Err(error) = rate_limiter
                    .finalize_success_with_retry(&request_id, i64::from(usage.total_tokens))
                    .await
                {
                    tracing::warn!("failed to finalize stream rate limit success: {error}");
                }

                if let Err(error) = channel_svc
                    .record_relay_success(channel.channel_id, channel.account_id, total_elapsed)
                    .await
                {
                    tracing::warn!("failed to update stream relay success health state: {error}");
                }
            });
            }
            StreamSettlement::Failure { status_code, message } => {
            billing.refund_later(request_id.clone(), token_info.token_id, pre_consumed);
            let rl = rate_limiter.clone();
            let request_id_for_task = request_id.clone();
            tokio::spawn(async move {
                if let Err(error) = rl.finalize_failure_with_retry(&request_id_for_task).await {
                    tracing::warn!("failed to finalize stream rate limit failure: {error}");
                }
            });
            channel_svc.record_relay_failure_async(
                channel.channel_id,
                channel.account_id,
                total_elapsed,
                status_code,
                message,
            );
        }
        }
    };

    let mut response = Sse::new(stream)
        .keep_alive(KeepAlive::default())
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
        (Some(_), false) => StreamSettlement::Failure {
            status_code: 0,
            message: "stream ended before terminal finish_reason".into(),
        },
        (None, _) => StreamSettlement::Failure {
            status_code: 0,
            message: "stream ended without usage".into(),
        },
    }
}

#[cfg(test)]
mod tests {
    use summer_ai_core::provider::{
        ProviderErrorInfo, ProviderErrorKind, ProviderStreamError,
    };

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
    fn resolve_stream_settlement_requires_terminal_finish_reason() {
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

        assert!(matches!(
            settlement,
            StreamSettlement::Failure { status_code: 0, .. }
        ));
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
            StreamSettlement::Failure { status_code: 400, .. }
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
