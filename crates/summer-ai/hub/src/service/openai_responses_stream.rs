use bytes::Bytes;
use futures::StreamExt;
use futures::stream::BoxStream;
use summer_web::axum::body::Body;
use summer_web::axum::http::{
    HeaderValue, StatusCode,
    header::{CACHE_CONTROL, CONTENT_TYPE},
};
use summer_web::axum::response::Response;

use summer_ai_core::types::chat::ChatCompletionChunk;
use summer_ai_core::types::common::Usage;
use summer_ai_core::types::responses::{
    ResponsesResponse, extract_response_model, extract_response_usage, is_output_text_delta_event,
};
use summer_ai_model::entity::request::RequestStatus;
use summer_ai_model::entity::request_execution::ExecutionStatus;

use crate::relay::billing::{BillingEngine, ModelConfigInfo};
use crate::relay::rate_limit::RateLimitEngine;
use crate::service::channel::ChannelService;
use crate::service::log::LogService;
use crate::service::openai_http::{
    insert_request_id_header, insert_upstream_request_id_header, response_usage_from_usage,
};
use crate::service::openai_tracking::RequestTrackingIds;
use crate::service::request::{ExecutionStatusUpdate, RequestService, RequestStatusUpdate};
use crate::service::resource_affinity::ResourceAffinityService;
use crate::service::response_bridge::ResponseBridgeService;

use crate::service::openai_responses_relay::spawn_usage_accounting_task;

#[derive(Default)]
pub(crate) struct ResponsesStreamTracker {
    pub(crate) buffer: Vec<u8>,
    pub(crate) usage: Option<Usage>,
    pub(crate) upstream_model: String,
    pub(crate) response_id: String,
}

impl ResponsesStreamTracker {
    pub(crate) fn ingest(
        &mut self,
        chunk: &Bytes,
        start: &std::time::Instant,
        first_token_time: &mut Option<i64>,
    ) {
        self.buffer.extend_from_slice(chunk);

        while let Some(pos) = find_double_newline(&self.buffer) {
            let event_bytes = self.buffer[..pos].to_vec();
            self.buffer = self.buffer[pos + 2..].to_vec();

            let event_block = match std::str::from_utf8(&event_bytes) {
                Ok(s) => s.to_string(),
                Err(_) => String::from_utf8_lossy(&event_bytes).into_owned(),
            };

            let mut data = String::new();
            for line in event_block.lines() {
                if let Some(value) = line.strip_prefix("data:") {
                    if !data.is_empty() {
                        data.push('\n');
                    }
                    data.push_str(value.trim_start());
                }
            }

            if data.is_empty() || data == "[DONE]" {
                continue;
            }

            let Ok(payload) = serde_json::from_str::<serde_json::Value>(&data) else {
                continue;
            };

            if first_token_time.is_none() && is_output_text_delta_event(&payload) {
                *first_token_time = Some(start.elapsed().as_millis() as i64);
            }

            if self.upstream_model.is_empty()
                && let Some(model) = extract_response_model(&payload)
            {
                self.upstream_model = model;
            }

            if self.response_id.is_empty()
                && let Some(response_id) = extract_response_id(&payload)
            {
                self.response_id = response_id;
            }

            if let Some(usage) = extract_response_usage(&payload) {
                self.usage = Some(usage);
            }
        }
    }
}

pub(crate) fn find_double_newline(buf: &[u8]) -> Option<usize> {
    buf.windows(2).position(|w| w == b"\n\n")
}

pub(crate) fn extract_response_id(payload: &serde_json::Value) -> Option<String> {
    payload
        .get("response")
        .and_then(|response| response.get("id"))
        .or_else(|| payload.get("id"))
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned)
}

fn responses_sse_bytes(payload: &serde_json::Value) -> Bytes {
    Bytes::from(format!("data: {payload}\n\n"))
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_responses_stream_response(
    upstream: reqwest::Response,
    token_info: crate::service::token::TokenInfo,
    pre_consumed: i64,
    model_config: ModelConfigInfo,
    group_ratio: f64,
    channel: crate::relay::channel_router::SelectedChannel,
    requested_model: String,
    estimated_prompt_tokens: i32,
    start_elapsed: i64,
    client_ip: String,
    log_svc: LogService,
    channel_svc: ChannelService,
    rate_limiter: RateLimitEngine,
    billing: BillingEngine,
    request_id: String,
    upstream_request_id: String,
    user_agent: String,
    resource_affinity: ResourceAffinityService,
    request_svc: RequestService,
    tracking: RequestTrackingIds,
) -> Response {
    let status = upstream.status();
    let content_type = upstream.headers().get(CONTENT_TYPE).cloned();
    let response_request_id = request_id.clone();
    let response_upstream_request_id = upstream_request_id.clone();

    let stream = async_stream::stream! {
        let start = std::time::Instant::now();
        let mut tracker = ResponsesStreamTracker::default();
        let mut first_token_time = None;
        let mut stream_error = None;
        let mut byte_stream = upstream.bytes_stream();

        while let Some(result) = byte_stream.next().await {
            match result {
                Ok(chunk) => {
                    tracker.ingest(&chunk, &start, &mut first_token_time);
                    yield Ok::<Bytes, std::convert::Infallible>(chunk);
                }
                Err(error) => {
                    tracing::error!("responses stream read error: {error}");
                    stream_error = Some(error.to_string());
                    break;
                }
            }
        }

        let total_elapsed = start_elapsed + start.elapsed().as_millis() as i64;
        if !tracker.response_id.is_empty()
            && let Err(error) = resource_affinity
                .bind(&token_info, "response", &tracker.response_id, &channel)
                .await
        {
            tracing::warn!("failed to bind streamed response affinity: {error}");
        }

        if let Some(usage) = tracker.usage {
            let upstream_model = if tracker.upstream_model.is_empty() {
                requested_model.clone()
            } else {
                tracker.upstream_model
            };
            request_svc
                .try_update_execution_status(
                    tracking.execution_id,
                    ExecutionStatusUpdate {
                        status: ExecutionStatus::Success,
                        error_message: None,
                        duration_ms: Some(total_elapsed as i32),
                        first_token_ms: Some(first_token_time.unwrap_or(0) as i32),
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
                        status: RequestStatus::Success,
                        error_message: None,
                        duration_ms: Some(total_elapsed as i32),
                        first_token_ms: Some(first_token_time.unwrap_or(0) as i32),
                        response_status_code: Some(200),
                        response_body: None,
                        upstream_model: Some(upstream_model.clone()),
                    },
                )
                .await;

            spawn_usage_accounting_task(
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
                "responses",
                "openai/responses",
                total_elapsed,
                first_token_time.unwrap_or(0) as i32,
                true,
            );
        } else {
            let fallback_reason = stream_error.unwrap_or_else(|| "response stream ended without usage".into());
            request_svc
                .try_update_execution_status(
                    tracking.execution_id,
                    ExecutionStatusUpdate {
                        status: ExecutionStatus::Failed,
                        error_message: Some(fallback_reason.clone()),
                        duration_ms: Some(total_elapsed as i32),
                        first_token_ms: Some(first_token_time.unwrap_or(0) as i32),
                        response_status_code: Some(0),
                        response_body: None,
                        upstream_request_id: Some(upstream_request_id.clone()),
                    },
                )
                .await;
            request_svc
                .try_update_request_status(
                    tracking.request_id,
                    RequestStatusUpdate {
                        status: RequestStatus::Failed,
                        error_message: Some(fallback_reason.clone()),
                        duration_ms: Some(total_elapsed as i32),
                        first_token_ms: Some(first_token_time.unwrap_or(0) as i32),
                        response_status_code: Some(0),
                        response_body: None,
                        upstream_model: (!tracker.upstream_model.is_empty())
                            .then(|| tracker.upstream_model.clone()),
                    },
                )
                .await;
            billing.refund_later(request_id.clone(), token_info.token_id, pre_consumed);
            let rl = rate_limiter.clone();
            let request_id_for_task = request_id.clone();
            tokio::spawn(async move {
                if let Err(error) = rl.finalize_failure_with_retry(&request_id_for_task).await {
                    tracing::warn!("failed to finalize responses rate limit failure: {error}");
                }
            });
            channel_svc.record_relay_failure_async(
                channel.channel_id,
                channel.account_id,
                total_elapsed,
                0,
                if estimated_prompt_tokens > 0 {
                    format!("{fallback_reason}; estimated_prompt_tokens={estimated_prompt_tokens}")
                } else {
                    fallback_reason
                },
            );
        }
    };

    let mut response = Response::builder()
        .status(status)
        .body(Body::from_stream(stream))
        .unwrap_or_else(|_| Response::new(Body::empty()));
    response.headers_mut().insert(
        CONTENT_TYPE,
        content_type.unwrap_or_else(|| HeaderValue::from_static("text/event-stream")),
    );
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    insert_request_id_header(&mut response, &response_request_id);
    insert_upstream_request_id_header(&mut response, &response_upstream_request_id);
    response
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn build_chat_bridged_responses_stream_response(
    upstream: BoxStream<'static, anyhow::Result<ChatCompletionChunk>>,
    token_info: crate::service::token::TokenInfo,
    pre_consumed: i64,
    model_config: ModelConfigInfo,
    group_ratio: f64,
    channel: crate::relay::channel_router::SelectedChannel,
    requested_model: String,
    estimated_prompt_tokens: i32,
    start_elapsed: i64,
    client_ip: String,
    log_svc: LogService,
    channel_svc: ChannelService,
    rate_limiter: RateLimitEngine,
    billing: BillingEngine,
    request_id: String,
    upstream_request_id: String,
    user_agent: String,
    response_bridge: ResponseBridgeService,
    input_snapshot: serde_json::Value,
    request_svc: RequestService,
    tracking: RequestTrackingIds,
) -> Response {
    let response_request_id = request_id.clone();
    let response_upstream_request_id = upstream_request_id.clone();

    let stream = async_stream::stream! {
        let start = std::time::Instant::now();
        let mut upstream = upstream;
        let mut first_token_time = None;
        let mut usage = None;
        let mut response_id = String::new();
        let mut upstream_model = String::new();
        let mut created_at = 0_i64;
        let mut output_text = String::new();
        let mut stream_error = None;
        let mut emitted_created = false;

        while let Some(item) = upstream.next().await {
            match item {
                Ok(chunk) => {
                    if response_id.is_empty() {
                        response_id = chunk.id.clone();
                        created_at = chunk.created;
                    }
                    if upstream_model.is_empty() && !chunk.model.is_empty() {
                        upstream_model = chunk.model.clone();
                    }

                    if !emitted_created {
                        emitted_created = true;
                        yield Ok::<Bytes, std::convert::Infallible>(responses_sse_bytes(&serde_json::json!({
                            "type": "response.created",
                            "response": {
                                "id": response_id,
                                "object": "response",
                                "created_at": created_at,
                                "model": if upstream_model.is_empty() { requested_model.clone() } else { upstream_model.clone() },
                                "status": "in_progress"
                            }
                        })));
                    }

                    for choice in &chunk.choices {
                        if let Some(text) = choice.delta.content.as_ref()
                            && !text.is_empty()
                        {
                            if first_token_time.is_none() {
                                first_token_time = Some(start.elapsed().as_millis() as i64);
                            }
                            output_text.push_str(text);
                            yield Ok(responses_sse_bytes(&serde_json::json!({
                                "type": "response.output_text.delta",
                                "delta": text,
                            })));
                        }
                    }

                    if let Some(chunk_usage) = chunk.usage {
                        usage = Some(chunk_usage);
                    }
                }
                Err(error) => {
                    tracing::error!("responses bridge stream read error: {error}");
                    stream_error = Some(error.to_string());
                    break;
                }
            }
        }

        let total_elapsed = start_elapsed + start.elapsed().as_millis() as i64;
        let completed_model = if upstream_model.is_empty() {
            requested_model.clone()
        } else {
            upstream_model.clone()
        };

        if let Some(usage) = usage {
            let bridged_response = ResponsesResponse {
                id: response_id.clone(),
                object: "response".into(),
                created_at,
                model: completed_model.clone(),
                status: "completed".into(),
                usage: Some(response_usage_from_usage(&usage)),
                output_text: (!output_text.is_empty()).then_some(output_text.clone()),
                extra: serde_json::Map::new(),
            };
            if let Err(error) = response_bridge
                .store(
                    &token_info,
                    bridged_response.clone(),
                    &input_snapshot,
                    &upstream_request_id,
                )
                .await
            {
                tracing::warn!("failed to store bridged response snapshot: {error}");
            }
            yield Ok(responses_sse_bytes(&serde_json::json!({
                "type": "response.completed",
                "response": bridged_response
            })));
            yield Ok(Bytes::from_static(b"data: [DONE]\n\n"));
            request_svc
                .try_update_execution_status(
                    tracking.execution_id,
                    ExecutionStatusUpdate {
                        status: ExecutionStatus::Success,
                        error_message: None,
                        duration_ms: Some(total_elapsed as i32),
                        first_token_ms: Some(first_token_time.unwrap_or(0) as i32),
                        response_status_code: Some(200),
                        response_body: Some(
                            serde_json::to_value(&bridged_response)
                                .unwrap_or(serde_json::Value::Null),
                        ),
                        upstream_request_id: Some(upstream_request_id.clone()),
                    },
                )
                .await;
            request_svc
                .try_update_request_status(
                    tracking.request_id,
                    RequestStatusUpdate {
                        status: RequestStatus::Success,
                        error_message: None,
                        duration_ms: Some(total_elapsed as i32),
                        first_token_ms: Some(first_token_time.unwrap_or(0) as i32),
                        response_status_code: Some(200),
                        response_body: Some(
                            serde_json::to_value(&bridged_response)
                                .unwrap_or(serde_json::Value::Null),
                        ),
                        upstream_model: Some(completed_model.clone()),
                    },
                )
                .await;

            spawn_usage_accounting_task(
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
                completed_model,
                client_ip,
                user_agent,
                "responses",
                "openai/responses",
                total_elapsed,
                first_token_time.unwrap_or(0) as i32,
                true,
            );
        } else {
            let fallback_reason = stream_error.unwrap_or_else(|| "response bridge stream ended without usage".into());
            request_svc
                .try_update_execution_status(
                    tracking.execution_id,
                    ExecutionStatusUpdate {
                        status: ExecutionStatus::Failed,
                        error_message: Some(fallback_reason.clone()),
                        duration_ms: Some(total_elapsed as i32),
                        first_token_ms: Some(first_token_time.unwrap_or(0) as i32),
                        response_status_code: Some(0),
                        response_body: None,
                        upstream_request_id: Some(upstream_request_id.clone()),
                    },
                )
                .await;
            request_svc
                .try_update_request_status(
                    tracking.request_id,
                    RequestStatusUpdate {
                        status: RequestStatus::Failed,
                        error_message: Some(fallback_reason.clone()),
                        duration_ms: Some(total_elapsed as i32),
                        first_token_ms: Some(first_token_time.unwrap_or(0) as i32),
                        response_status_code: Some(0),
                        response_body: None,
                        upstream_model: (!completed_model.is_empty()).then_some(completed_model),
                    },
                )
                .await;
            billing.refund_later(request_id.clone(), token_info.token_id, pre_consumed);
            let rl = rate_limiter.clone();
            let request_id_for_task = request_id.clone();
            tokio::spawn(async move {
                if let Err(error) = rl.finalize_failure_with_retry(&request_id_for_task).await {
                    tracing::warn!("failed to finalize bridged responses rate limit failure: {error}");
                }
            });
            channel_svc.record_relay_failure_async(
                channel.channel_id,
                channel.account_id,
                total_elapsed,
                0,
                if estimated_prompt_tokens > 0 {
                    format!("{fallback_reason}; estimated_prompt_tokens={estimated_prompt_tokens}")
                } else {
                    fallback_reason
                },
            );
        }
    };

    let mut response = Response::builder()
        .status(StatusCode::OK)
        .body(Body::from_stream(stream))
        .unwrap_or_else(|_| Response::new(Body::empty()));
    response
        .headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static("text/event-stream"));
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    insert_request_id_header(&mut response, &response_request_id);
    insert_upstream_request_id_header(&mut response, &response_upstream_request_id);
    response
}
