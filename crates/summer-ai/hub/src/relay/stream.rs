use futures::StreamExt;
use summer_web::axum::response::sse::{Event, KeepAlive, Sse};
use summer_web::axum::response::{IntoResponse, Response};

use summer_ai_core::types::chat::ChatCompletionChunk;
use summer_ai_core::types::common::Usage;

use crate::relay::billing::{BillingEngine, ModelConfigInfo};
use crate::relay::channel_router::SelectedChannel;
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
    billing: BillingEngine,
) -> Response {
    let stream = async_stream::stream! {
        let mut last_usage: Option<Usage> = None;
        let mut first_token_time: Option<i64> = None;
        let start = std::time::Instant::now();
        let mut upstream_model = String::new();

        tokio::pin!(chunk_stream);
        while let Some(result) = chunk_stream.next().await {
            match result {
                Ok(chunk) => {
                    if first_token_time.is_none() && !chunk.choices.is_empty() {
                        first_token_time = Some(start.elapsed().as_millis() as i64);
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
                    break;
                }
            }
        }

        yield Ok(Event::default().data("[DONE]"));

        let total_elapsed = start_elapsed + start.elapsed().as_millis() as i64;
        let ftt = first_token_time.unwrap_or(0) as i32;

        if let Some(usage) = last_usage {
            let usage_clone = usage.clone();
            tokio::spawn(async move {
                let logged_quota =
                    BillingEngine::calculate_actual_quota(&usage_clone, &model_config, group_ratio);
                let actual_quota = match billing
                    .post_consume(
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
                        requested_model,
                        upstream_model,
                        model_name: model_config.model_name,
                        quota: actual_quota,
                        elapsed_time: total_elapsed as i32,
                        first_token_time: ftt,
                        is_stream: true,
                        client_ip,
                    },
                );
            });
        } else {
            billing.refund_later(token_info.token_id, pre_consumed);
        }
    };

    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}
