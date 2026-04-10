use std::io;
use std::pin::Pin;
use std::task::{Context as TaskContext, Poll};
use std::time::Instant;

use anyhow::Context;
use futures::Stream;
use futures::stream::BoxStream;
use summer_ai_billing::service::engine::BillingEngine;
use summer_ai_core::provider::{ProviderKind, ProviderRegistry};
use summer_ai_core::types::common::Usage;
use summer_ai_core::types::error::OpenAiErrorResponse;
use summer_web::axum::body::Body;
use summer_web::axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use summer_web::axum::http::{HeaderValue, StatusCode};
use summer_web::axum::response::Response;

use super::{
    ChatBillingContext, ChatLogContext, ChatRelayService, PreparedChatRelay, RelayChatContext,
    UpstreamChatStreamResponse, build_chat_log_context, build_tracking_upstream_body,
};
use crate::plugin::RelayStreamTaskTracker;
use crate::service::log::LogService;
use crate::service::shared::relay::{
    RELAY_MAX_UPSTREAM_ATTEMPTS, advance_relay_retry, build_retry_attempt_payload,
    complete_relay_retry_success, extract_upstream_request_id, is_retryable_upstream_error,
    provider_error_to_openai_response, stream_error_message, stream_error_status_code,
};
use crate::service::shared::stream::driver::{
    BoxFinalizeFuture, RelayStreamAdapter, RelayStreamFinalizer,
};
use crate::service::shared::stream::usage_tracking_finalize::{
    UsageStreamBillingSnapshot, UsageStreamFinalizeContext, UsageStreamFinalizeMeta,
    UsageStreamFinalizeSettlement, UsageStreamLogSnapshot,
};
use crate::service::tracking::TrackingService;

const DOWNSTREAM_CLIENT_CLOSED_STATUS_CODE: i32 = 499;
const DOWNSTREAM_CLIENT_CLOSED_MESSAGE: &str = "chat stream dropped before completion";

#[derive(Clone, Debug, Default)]
struct ChatStreamProgress {
    first_token_ms: Option<i32>,
    saw_explicit_terminal_signal: bool,
    saw_any_chunk: bool,
    last_usage: Option<Usage>,
    estimated_completion_tokens: i32,
    estimated_reasoning_tokens: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ChatStreamSettlement {
    Success,
    Failure { status_code: i32, message: String },
}

pub(super) struct TrackedChatSseStreamArgs {
    pub(super) inner: BoxStream<'static, anyhow::Result<summer_ai_core::stream::ChatStreamItem>>,
    pub(super) task_tracker: RelayStreamTaskTracker,
    pub(super) tracking: Option<TrackingService>,
    pub(super) billing: Option<BillingEngine>,
    pub(super) billing_context: Option<ChatBillingContext>,
    pub(super) log: Option<LogService>,
    pub(super) log_context: Option<ChatLogContext>,
    pub(super) tracked_request_id: Option<i64>,
    pub(super) tracked_execution_id: Option<i64>,
    pub(super) request_id: String,
    pub(super) requested_model: String,
    pub(super) trace_id: Option<i64>,
    pub(super) started_at: Instant,
    pub(super) upstream_model: String,
    pub(super) upstream_request_id: Option<String>,
    pub(super) response_status_code: i32,
}

type ChatTrackedInner = crate::service::shared::stream::driver::TrackedRelayStream<
    BoxStream<'static, anyhow::Result<summer_ai_core::stream::ChatStreamItem>>,
    ChatSseAdapter,
    ChatStreamFinalizeContext,
>;

type ChatStreamFinalizeContext = UsageStreamFinalizeContext;

#[derive(Clone)]
struct ChatSseAdapter {
    request_id: String,
    started_at: Instant,
}

pub(super) struct TrackedChatSseStream {
    inner: ChatTrackedInner,
}

impl TrackedChatSseStream {
    pub(super) fn new(args: TrackedChatSseStreamArgs) -> Self {
        let adapter = ChatSseAdapter {
            request_id: args.request_id.clone(),
            started_at: args.started_at,
        };
        let finalizer = ChatStreamFinalizeContext::new(
            UsageStreamFinalizeMeta::new(
                "/v1/chat/completions",
                "openai/chat_completions",
                args.request_id,
                args.requested_model,
                args.upstream_model,
                args.upstream_request_id,
                args.response_status_code,
            ),
            args.started_at,
            args.tracking,
            args.billing,
            args.billing_context.map(build_chat_billing_snapshot),
            args.log,
            args.log_context.map(build_chat_log_snapshot),
            args.trace_id,
            args.tracked_request_id,
            args.tracked_execution_id,
        );

        Self {
            inner: crate::service::shared::stream::driver::TrackedRelayStream::new(
                args.inner,
                adapter,
                args.task_tracker,
                finalizer,
            ),
        }
    }
}

impl Stream for TrackedChatSseStream {
    type Item = Result<bytes::Bytes, io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.inner).poll_next(cx)
    }
}

fn build_chat_chunk_sse_frame(
    chunk: &summer_ai_core::types::chat::ChatCompletionChunk,
) -> anyhow::Result<bytes::Bytes> {
    let json = serde_json::to_string(chunk).context("failed to serialize chat chunk")?;
    Ok(bytes::Bytes::from(format!("data: {json}\n\n")))
}

fn build_done_sse_frame() -> bytes::Bytes {
    bytes::Bytes::from_static(b"data: [DONE]\n\n")
}

fn resolve_final_chat_stream_usage(
    progress: &ChatStreamProgress,
    billing: &UsageStreamBillingSnapshot,
) -> Usage {
    progress.last_usage.clone().unwrap_or_else(|| Usage {
        prompt_tokens: billing.estimated_prompt_tokens.max(0),
        completion_tokens: progress.estimated_completion_tokens.max(0),
        total_tokens: billing
            .estimated_prompt_tokens
            .max(0)
            .saturating_add(progress.estimated_completion_tokens.max(0)),
        cached_tokens: 0,
        reasoning_tokens: progress.estimated_reasoning_tokens.max(0),
    })
}

fn chunk_contains_visible_output(chunk: &summer_ai_core::types::chat::ChatCompletionChunk) -> bool {
    chunk.choices.iter().any(|choice| {
        choice
            .delta
            .content
            .as_ref()
            .is_some_and(|text| !text.is_empty())
            || choice
                .delta
                .reasoning_content
                .as_ref()
                .is_some_and(|text| !text.is_empty())
            || choice
                .delta
                .tool_calls
                .as_ref()
                .is_some_and(|calls| !calls.is_empty())
    })
}

fn estimate_chunk_output_tokens(chunk: &summer_ai_core::types::chat::ChatCompletionChunk) -> i32 {
    chunk
        .choices
        .iter()
        .map(estimate_choice_output_tokens)
        .sum()
}

fn estimate_choice_output_tokens(choice: &summer_ai_core::types::chat::ChunkChoice) -> i32 {
    estimate_text_tokens(choice.delta.content.as_deref().unwrap_or_default())
        + choice
            .delta
            .tool_calls
            .as_ref()
            .map(|calls| {
                estimate_text_tokens(
                    &serde_json::to_string(calls).unwrap_or_else(|_| String::new()),
                )
            })
            .unwrap_or(0)
}

fn estimate_chunk_reasoning_tokens(
    chunk: &summer_ai_core::types::chat::ChatCompletionChunk,
) -> i32 {
    chunk
        .choices
        .iter()
        .map(|choice| {
            estimate_text_tokens(
                choice
                    .delta
                    .reasoning_content
                    .as_deref()
                    .unwrap_or_default(),
            )
        })
        .sum()
}

fn estimate_text_tokens(text: &str) -> i32 {
    if text.is_empty() {
        return 0;
    }

    (text.chars().count() as f64 / 4.0).ceil() as i32
}

fn resolve_chat_stream_settlement(
    progress: &ChatStreamProgress,
    stream_error: Option<&anyhow::Error>,
) -> ChatStreamSettlement {
    if let Some(error) = stream_error {
        return ChatStreamSettlement::Failure {
            status_code: stream_error_status_code(error),
            message: stream_error_message(error),
        };
    }

    if progress.saw_explicit_terminal_signal {
        ChatStreamSettlement::Success
    } else if progress.saw_any_chunk {
        ChatStreamSettlement::Failure {
            status_code: 0,
            message: "chat stream ended without explicit terminal signal".to_string(),
        }
    } else {
        ChatStreamSettlement::Failure {
            status_code: 0,
            message: "chat stream ended before any relay chunk".to_string(),
        }
    }
}

impl RelayStreamAdapter for ChatSseAdapter {
    type Item = summer_ai_core::stream::ChatStreamItem;
    type Progress = ChatStreamProgress;
    type Settlement = ChatStreamSettlement;

    fn request_id(&self) -> &str {
        self.request_id.as_str()
    }

    fn observe(
        &mut self,
        progress: &mut Self::Progress,
        item: Self::Item,
        pending_frames: &mut std::collections::VecDeque<bytes::Bytes>,
    ) -> anyhow::Result<()> {
        if item.is_terminal() {
            progress.saw_explicit_terminal_signal = true;
        }
        let Some(chunk) = item.into_chunk() else {
            return Ok(());
        };

        progress.saw_any_chunk = true;
        if let Some(usage) = chunk.usage.clone() {
            progress.last_usage = Some(usage);
        }
        progress.estimated_completion_tokens += estimate_chunk_output_tokens(&chunk);
        progress.estimated_reasoning_tokens += estimate_chunk_reasoning_tokens(&chunk);
        if progress.first_token_ms.is_none() && chunk_contains_visible_output(&chunk) {
            progress.first_token_ms = Some(self.started_at.elapsed().as_millis() as i32);
        }
        pending_frames.push_back(build_chat_chunk_sse_frame(&chunk)?);
        Ok(())
    }

    fn settle_on_error(
        &self,
        progress: &Self::Progress,
        error: &anyhow::Error,
    ) -> Self::Settlement {
        resolve_chat_stream_settlement(progress, Some(error))
    }

    fn settle_on_eof(
        &mut self,
        progress: &Self::Progress,
        pending_frames: &mut std::collections::VecDeque<bytes::Bytes>,
    ) -> anyhow::Result<Self::Settlement> {
        let settlement = resolve_chat_stream_settlement(progress, None);
        if settlement == ChatStreamSettlement::Success {
            pending_frames.push_back(build_done_sse_frame());
        }
        Ok(settlement)
    }

    fn settle_on_cancel(&self) -> Self::Settlement {
        ChatStreamSettlement::Failure {
            status_code: DOWNSTREAM_CLIENT_CLOSED_STATUS_CODE,
            message: DOWNSTREAM_CLIENT_CLOSED_MESSAGE.to_string(),
        }
    }
}

impl RelayStreamFinalizer<ChatStreamProgress, ChatStreamSettlement> for ChatStreamFinalizeContext {
    fn build_finalize_future(
        &self,
        progress: &ChatStreamProgress,
        settlement: ChatStreamSettlement,
    ) -> Option<BoxFinalizeFuture> {
        let first_token_ms = progress.first_token_ms.unwrap_or(0);
        let final_usage = self
            .billing_context()
            .as_ref()
            .map(|billing_context| resolve_final_chat_stream_usage(progress, billing_context))
            .or_else(|| progress.last_usage.clone());

        let shared_settlement = match settlement {
            ChatStreamSettlement::Success => UsageStreamFinalizeSettlement::success(
                self.meta().upstream_model.clone(),
                first_token_ms,
                final_usage,
            ),
            ChatStreamSettlement::Failure {
                status_code,
                message,
            } => UsageStreamFinalizeSettlement::failure(
                self.meta().upstream_model.clone(),
                first_token_ms,
                status_code,
                message,
            ),
        };

        self.build_usage_finalize_future(shared_settlement)
    }
}

fn build_chat_billing_snapshot(context: ChatBillingContext) -> UsageStreamBillingSnapshot {
    UsageStreamBillingSnapshot {
        token_id: context.token_id,
        unlimited_quota: context.unlimited_quota,
        group_ratio: context.group_ratio,
        pre_consumed: context.pre_consumed,
        estimated_prompt_tokens: context.estimated_prompt_tokens,
        price: context.price,
    }
}

fn build_chat_log_snapshot(context: ChatLogContext) -> UsageStreamLogSnapshot {
    UsageStreamLogSnapshot {
        token_info: context.token_info,
        channel_id: context.channel_id,
        channel_name: context.channel_name,
        account_id: context.account_id,
        account_name: context.account_name,
        execution_id: context.execution_id,
        requested_model: context.requested_model,
        client_ip: context.client_ip,
        user_agent: context.user_agent,
    }
}

impl ChatRelayService {
    pub(super) async fn relay_stream(
        &self,
        ctx: &RelayChatContext,
        request: &summer_ai_core::types::chat::ChatCompletionRequest,
        prepared: PreparedChatRelay,
    ) -> Result<Response, OpenAiErrorResponse> {
        let PreparedChatRelay {
            request_id,
            trace_id,
            started_at,
            tracked_request,
            mut tracked_execution,
            billing,
            mut log_context,
            target,
            provider,
            request_builder,
        } = prepared;
        let retry_request_builder = request_builder.try_clone();
        let mut first_request_builder = Some(request_builder);
        let mut pending_retry_attempt = None;

        for attempt_no in 1..=RELAY_MAX_UPSTREAM_ATTEMPTS {
            let attempt_started_at = Instant::now();
            if attempt_no > 1 {
                tracked_execution = if let Some(tracked_request) = tracked_request.as_ref() {
                    let upstream_body =
                        build_tracking_upstream_body(request, &target.upstream_model);
                    let tracked_execution = match self
                        .tracking
                        .create_chat_execution(
                            tracked_request.id,
                            &request_id,
                            attempt_no,
                            request,
                            target.channel.id,
                            target.account.id,
                            &target.upstream_model,
                            upstream_body.clone(),
                        )
                        .await
                    {
                        Ok(model) => Some(model),
                        Err(error) => {
                            tracing::warn!(request_id, error = %error, attempt_no, "failed to create retry request_execution tracking row");
                            None
                        }
                    };

                    if tracked_request.trace_id > 0
                        && let Err(error) = self
                            .tracking
                            .create_execution_trace_span(
                                tracked_request.trace_id,
                                &request_id,
                                "chat",
                                attempt_no,
                                &request.model,
                                &target.upstream_model,
                                target.channel.id,
                                target.account.id,
                                upstream_body,
                            )
                            .await
                    {
                        tracing::warn!(request_id, error = %error, attempt_no, "failed to create retry trace span tracking row");
                    }

                    tracked_execution
                } else {
                    None
                };

                log_context = build_chat_log_context(
                    ctx,
                    target.channel.id,
                    &target.channel.name,
                    target.account.id,
                    &target.account.name,
                    tracked_execution
                        .as_ref()
                        .map(|model| model.id)
                        .unwrap_or(0),
                    &request.model,
                );
            }

            let error_ctx = self.error_context(
                trace_id,
                request.stream,
                tracked_request.as_ref(),
                tracked_execution.as_ref(),
                Some(&log_context),
                Some(&billing),
                Some(&target.upstream_model),
                &started_at,
            );
            let request_builder = if attempt_no == 1 {
                first_request_builder
                    .take()
                    .expect("first chat stream request builder must exist")
            } else {
                let Some(retry_template) = retry_request_builder.as_ref() else {
                    return Err(error_ctx
                        .finish(
                            None,
                            OpenAiErrorResponse::internal_with(
                                "chat stream request builder is not cloneable for retry",
                                "request builder cannot be cloned",
                            ),
                        )
                        .await);
                };
                let Some(cloned) = retry_template.try_clone() else {
                    return Err(error_ctx
                        .finish(
                            None,
                            OpenAiErrorResponse::internal_with(
                                "failed to clone chat stream request builder for retry",
                                "request builder cannot be cloned",
                            ),
                        )
                        .await);
                };
                cloned
            };

            let upstream_response = match self
                .send_upstream_chat_stream(request_builder, target.provider_kind)
                .await
            {
                Ok(response) => response,
                Err(error) => {
                    let attempt_duration_ms = attempt_started_at.elapsed().as_millis() as i32;
                    self.try_finish_execution_failure(
                        &self.tracking,
                        tracked_request.as_ref().map(|model| model.trace_id),
                        tracked_execution.as_ref(),
                        None,
                        &error,
                        None,
                        attempt_duration_ms,
                    )
                    .await;

                    if advance_relay_retry(
                        &self.tracking,
                        "chat",
                        &request_id,
                        attempt_no,
                        &mut pending_retry_attempt,
                        &error.error.error.message,
                        build_retry_attempt_payload(
                            tracked_execution.as_ref().map(|model| model.id),
                            target.channel.id,
                            target.account.id,
                            &target.upstream_model,
                            error.status as i32,
                            "send_stream_upstream",
                            None,
                        ),
                        true,
                    )
                    .await
                    {
                        continue;
                    }

                    return Err(error_ctx.finish(None, error).await);
                }
            };

            let (response_status_code, upstream_request_id, response) = match upstream_response {
                UpstreamChatStreamResponse::Success {
                    status_code,
                    upstream_request_id,
                    response,
                } => (status_code, upstream_request_id, response),
                UpstreamChatStreamResponse::Failure {
                    upstream_request_id,
                    error,
                } => {
                    let attempt_duration_ms = attempt_started_at.elapsed().as_millis() as i32;
                    self.try_finish_execution_failure(
                        &self.tracking,
                        tracked_request.as_ref().map(|model| model.trace_id),
                        tracked_execution.as_ref(),
                        upstream_request_id.as_deref(),
                        &error,
                        None,
                        attempt_duration_ms,
                    )
                    .await;

                    if advance_relay_retry(
                        &self.tracking,
                        "chat",
                        &request_id,
                        attempt_no,
                        &mut pending_retry_attempt,
                        &error.error.error.message,
                        build_retry_attempt_payload(
                            tracked_execution.as_ref().map(|model| model.id),
                            target.channel.id,
                            target.account.id,
                            &target.upstream_model,
                            error.status as i32,
                            "stream_upstream_status",
                            upstream_request_id.as_deref(),
                        ),
                        is_retryable_upstream_error(&error),
                    )
                    .await
                    {
                        continue;
                    }

                    return Err(error_ctx
                        .finish(upstream_request_id.as_deref(), error)
                        .await);
                }
            };

            let chunk_stream = match provider.parse_chat_stream(response, &target.upstream_model) {
                Ok(stream) => stream,
                Err(error) => {
                    let openai_error = OpenAiErrorResponse::internal_with(
                        "failed to parse upstream chat stream",
                        error,
                    );
                    let attempt_duration_ms = attempt_started_at.elapsed().as_millis() as i32;
                    self.try_finish_execution_failure(
                        &self.tracking,
                        tracked_request.as_ref().map(|model| model.trace_id),
                        tracked_execution.as_ref(),
                        upstream_request_id.as_deref(),
                        &openai_error,
                        None,
                        attempt_duration_ms,
                    )
                    .await;

                    advance_relay_retry(
                        &self.tracking,
                        "chat",
                        &request_id,
                        attempt_no,
                        &mut pending_retry_attempt,
                        &openai_error.error.error.message,
                        build_retry_attempt_payload(
                            tracked_execution.as_ref().map(|model| model.id),
                            target.channel.id,
                            target.account.id,
                            &target.upstream_model,
                            openai_error.status as i32,
                            "parse_stream",
                            upstream_request_id.as_deref(),
                        ),
                        false,
                    )
                    .await;

                    return Err(error_ctx
                        .finish(upstream_request_id.as_deref(), openai_error)
                        .await);
                }
            };

            if attempt_no > 1 {
                complete_relay_retry_success(
                    &self.tracking,
                    &request_id,
                    pending_retry_attempt.as_ref(),
                    build_retry_attempt_payload(
                        tracked_execution.as_ref().map(|model| model.id),
                        target.channel.id,
                        target.account.id,
                        &target.upstream_model,
                        response_status_code,
                        "retry_succeeded",
                        upstream_request_id.as_deref(),
                    ),
                )
                .await;
            }

            let stream = TrackedChatSseStream::new(TrackedChatSseStreamArgs {
                inner: chunk_stream,
                task_tracker: self.stream_task_tracker.clone(),
                tracking: Some(self.tracking.clone()),
                billing: Some(self.billing.clone()),
                billing_context: Some(billing.clone()),
                log: Some(self.log.clone()),
                log_context: Some(log_context.clone()),
                tracked_request_id: tracked_request.as_ref().map(|model| model.id),
                tracked_execution_id: tracked_execution.as_ref().map(|model| model.id),
                request_id: request_id.clone(),
                requested_model: request.model.clone(),
                trace_id,
                started_at,
                upstream_model: target.upstream_model.clone(),
                upstream_request_id: upstream_request_id.clone(),
                response_status_code,
            });

            let mut response = Response::builder()
                .status(StatusCode::OK)
                .body(Body::from_stream(stream))
                .expect("chat stream response");
            response
                .headers_mut()
                .insert(CONTENT_TYPE, HeaderValue::from_static("text/event-stream"));
            response
                .headers_mut()
                .insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));
            if let Ok(value) = HeaderValue::from_str(&request_id) {
                response.headers_mut().insert("x-request-id", value);
            }
            if let Some(upstream_request_id) = upstream_request_id
                && let Ok(value) = HeaderValue::from_str(&upstream_request_id)
            {
                response
                    .headers_mut()
                    .insert("x-upstream-request-id", value);
            }

            return Ok(response);
        }

        Err(OpenAiErrorResponse::internal_with(
            "chat stream relay exhausted retry attempts",
            "no retry attempt produced a terminal result",
        ))
    }

    pub(super) async fn send_upstream_chat_stream(
        &self,
        request_builder: reqwest::RequestBuilder,
        provider_kind: ProviderKind,
    ) -> Result<UpstreamChatStreamResponse, OpenAiErrorResponse> {
        let response = request_builder.send().await.map_err(|error| {
            OpenAiErrorResponse::internal_with("failed to call upstream provider", error)
        })?;

        let status = response.status();
        let headers = response.headers().clone();
        let upstream_request_id = extract_upstream_request_id(&headers);

        if status.is_success() {
            Ok(UpstreamChatStreamResponse::Success {
                status_code: status.as_u16() as i32,
                upstream_request_id,
                response,
            })
        } else {
            let body = response.bytes().await.map_err(|error| {
                OpenAiErrorResponse::internal_with("failed to read upstream response", error)
            })?;
            let info =
                ProviderRegistry::get(provider_kind).parse_error(status.as_u16(), &headers, &body);
            Ok(UpstreamChatStreamResponse::Failure {
                upstream_request_id,
                error: provider_error_to_openai_response(status.as_u16(), &info),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use futures::StreamExt as _;
    use futures::stream;
    use summer_ai_billing::service::channel_model_price::ResolvedModelPrice;
    use summer_ai_core::stream::ChatStreamItem;
    use summer_ai_core::types::chat::ChatCompletionChunk;
    use summer_ai_core::types::common::Delta;
    use summer_ai_model::entity::channel_model_price::ChannelModelPriceBillingMode;

    use super::{
        ChatStreamProgress, ChatStreamSettlement, TrackedChatSseStream, TrackedChatSseStreamArgs,
        build_chat_chunk_sse_frame, chunk_contains_visible_output, resolve_chat_stream_settlement,
        resolve_final_chat_stream_usage,
    };
    use crate::plugin::RelayStreamTaskTracker;
    use crate::service::chat::ChatBillingContext;

    #[test]
    fn build_chat_chunk_sse_frame_wraps_json_in_data_prefix() {
        let chunk = ChatCompletionChunk {
            id: "chatcmpl-stream-1".into(),
            object: "chat.completion.chunk".into(),
            created: 1,
            model: "gpt-5.4".into(),
            choices: vec![summer_ai_core::types::chat::ChunkChoice {
                index: 0,
                delta: Delta {
                    role: Some("assistant".into()),
                    content: Some("hello".into()),
                    reasoning_content: None,
                    tool_calls: None,
                },
                finish_reason: None,
            }],
            usage: None,
        };

        let bytes = build_chat_chunk_sse_frame(&chunk).expect("sse bytes");
        assert_eq!(
            std::str::from_utf8(&bytes).expect("utf8"),
            "data: {\"id\":\"chatcmpl-stream-1\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"gpt-5.4\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"hello\"},\"finish_reason\":null}]}\n\n"
        );
    }

    #[test]
    fn chunk_contains_visible_output_ignores_usage_only_and_empty_deltas() {
        let usage_only: ChatCompletionChunk = serde_json::from_value(serde_json::json!({
            "id": "chatcmpl-usage",
            "object": "chat.completion.chunk",
            "created": 1,
            "model": "gpt-5.4",
            "choices": [],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 2,
                "total_tokens": 12
            }
        }))
        .expect("usage chunk");
        assert!(!chunk_contains_visible_output(&usage_only));

        let empty_delta: ChatCompletionChunk = serde_json::from_value(serde_json::json!({
            "id": "chatcmpl-empty",
            "object": "chat.completion.chunk",
            "created": 1,
            "model": "gpt-5.4",
            "choices": [{
                "index": 0,
                "delta": {},
                "finish_reason": null
            }]
        }))
        .expect("empty delta chunk");
        assert!(!chunk_contains_visible_output(&empty_delta));

        let text_delta: ChatCompletionChunk = serde_json::from_value(serde_json::json!({
            "id": "chatcmpl-text",
            "object": "chat.completion.chunk",
            "created": 1,
            "model": "gpt-5.4",
            "choices": [{
                "index": 0,
                "delta": {"content": "hello"},
                "finish_reason": null
            }]
        }))
        .expect("text chunk");
        assert!(chunk_contains_visible_output(&text_delta));
    }

    #[test]
    fn resolve_chat_stream_settlement_accepts_supported_clean_endings() {
        assert!(matches!(
            resolve_chat_stream_settlement(
                &ChatStreamProgress {
                    saw_explicit_terminal_signal: true,
                    ..Default::default()
                },
                None,
            ),
            ChatStreamSettlement::Success
        ));

        assert!(matches!(
            resolve_chat_stream_settlement(
                &ChatStreamProgress {
                    saw_any_chunk: true,
                    ..Default::default()
                },
                None,
            ),
            ChatStreamSettlement::Failure { .. }
        ));
    }

    #[test]
    fn resolve_chat_stream_settlement_rejects_visible_output_without_terminal_signal() {
        assert!(matches!(
            resolve_chat_stream_settlement(
                &ChatStreamProgress {
                    first_token_ms: Some(42),
                    saw_any_chunk: true,
                    ..Default::default()
                },
                None,
            ),
            ChatStreamSettlement::Failure { .. }
        ));
    }

    #[tokio::test]
    async fn tracked_chat_sse_stream_skips_done_when_stream_errors() {
        let text_chunk: ChatCompletionChunk = serde_json::from_value(serde_json::json!({
            "id": "chatcmpl-text",
            "object": "chat.completion.chunk",
            "created": 1,
            "model": "gpt-5.4",
            "choices": [{
                "index": 0,
                "delta": {"content": "hello"},
                "finish_reason": null
            }]
        }))
        .expect("text chunk");

        let stream = TrackedChatSseStream::new(TrackedChatSseStreamArgs {
            inner: stream::iter(vec![
                Ok(ChatStreamItem::chunk(text_chunk)),
                Err(anyhow::anyhow!("synthetic stream failure")),
            ])
            .boxed(),
            task_tracker: RelayStreamTaskTracker::new(),
            tracking: None,
            billing: None,
            billing_context: None,
            log: None,
            log_context: None,
            tracked_request_id: None,
            tracked_execution_id: None,
            request_id: "req_test_stream_error".into(),
            requested_model: "gpt-5.4".into(),
            trace_id: None,
            started_at: Instant::now(),
            upstream_model: "gpt-5.4".into(),
            upstream_request_id: None,
            response_status_code: 200,
        });

        let items: Vec<_> = stream.collect().await;
        assert_eq!(items.len(), 2);
        assert_eq!(
            std::str::from_utf8(items[0].as_ref().expect("first chunk")).expect("utf8"),
            "data: {\"id\":\"chatcmpl-text\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"gpt-5.4\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hello\"},\"finish_reason\":null}]}\n\n"
        );
        assert!(items[1].is_err());
    }

    #[tokio::test]
    async fn tracked_chat_sse_stream_accepts_usage_only_terminal_event() {
        let usage_only_chunk = ChatStreamItem::terminal_chunk(
            serde_json::from_value::<ChatCompletionChunk>(serde_json::json!({
                "id": "chatcmpl-usage",
                "object": "chat.completion.chunk",
                "created": 1,
                "model": "gpt-5.4",
                "choices": [],
                "usage": {
                    "prompt_tokens": 10,
                    "completion_tokens": 2,
                    "total_tokens": 12
                }
            }))
            .expect("usage chunk"),
        );

        let stream = TrackedChatSseStream::new(TrackedChatSseStreamArgs {
            inner: stream::iter(vec![Ok(usage_only_chunk)]).boxed(),
            task_tracker: RelayStreamTaskTracker::new(),
            tracking: None,
            billing: None,
            billing_context: None,
            log: None,
            log_context: None,
            tracked_request_id: None,
            tracked_execution_id: None,
            request_id: "req_test_usage_only".into(),
            requested_model: "gpt-5.4".into(),
            trace_id: None,
            started_at: Instant::now(),
            upstream_model: "gpt-5.4".into(),
            upstream_request_id: None,
            response_status_code: 200,
        });

        let items: Vec<_> = stream.collect().await;
        assert_eq!(items.len(), 2);
        assert_eq!(
            std::str::from_utf8(items[0].as_ref().expect("usage frame")).expect("utf8"),
            "data: {\"id\":\"chatcmpl-usage\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"gpt-5.4\",\"choices\":[],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":2,\"total_tokens\":12}}\n\n"
        );
        assert_eq!(
            std::str::from_utf8(items[1].as_ref().expect("done frame")).expect("utf8"),
            "data: [DONE]\n\n"
        );
    }

    #[test]
    fn resolve_final_chat_stream_usage_estimates_usage_when_terminal_chunk_has_no_usage() {
        let billing = super::build_chat_billing_snapshot(ChatBillingContext {
            token_id: 1,
            unlimited_quota: false,
            group_ratio: 1.0,
            pre_consumed: 10,
            estimated_prompt_tokens: 12,
            price: ResolvedModelPrice {
                model_name: "gpt-5.4".into(),
                billing_mode: ChannelModelPriceBillingMode::ByToken,
                currency: "USD".into(),
                input_ratio: 1.0,
                output_ratio: 1.0,
                cached_input_ratio: 0.0,
                reasoning_ratio: 0.0,
                supported_endpoints: vec!["chat".into()],
                price_reference: String::new(),
            },
        });
        let progress = ChatStreamProgress {
            estimated_completion_tokens: 18,
            estimated_reasoning_tokens: 5,
            ..Default::default()
        };

        let usage = resolve_final_chat_stream_usage(&progress, &billing);

        assert_eq!(usage.prompt_tokens, 12);
        assert_eq!(usage.completion_tokens, 18);
        assert_eq!(usage.reasoning_tokens, 5);
        assert_eq!(usage.total_tokens, 30);
    }
}
