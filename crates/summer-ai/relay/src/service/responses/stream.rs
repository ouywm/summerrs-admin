use std::collections::VecDeque;
use std::io;
use std::pin::Pin;
use std::task::{Context as TaskContext, Poll};
use std::time::Instant;

use anyhow::Context;
use bytes::Bytes;
use futures::Stream;
use futures::stream::BoxStream;
use summer_ai_billing::service::engine::BillingEngine;
use summer_ai_core::stream::{ChatStreamItem, SseEvent, SseParser};
use summer_ai_core::types::common::Usage;
use summer_ai_core::types::responses::{
    ResponseInputTokensDetails, ResponseOutputTokensDetails, ResponseUsage, ResponsesResponse,
    extract_response_model, extract_response_usage, is_output_text_delta_event,
};

use super::{ResponsesBillingContext, ResponsesLogContext};
use crate::plugin::RelayStreamTaskTracker;
use crate::service::log::LogService;
use crate::service::shared::stream::driver::{
    BoxFinalizeFuture, RelayStreamAdapter, RelayStreamFinalizer,
};
use crate::service::shared::stream::usage_tracking_finalize::{
    UsageStreamBillingSnapshot, UsageStreamFinalizeContext, UsageStreamFinalizeMeta,
    UsageStreamFinalizeSettlement, UsageStreamLogSnapshot,
};
use crate::service::tracking::TrackingService;

const DOWNSTREAM_CLIENT_CLOSED_STATUS_CODE: i32 = 499;
const DOWNSTREAM_CLIENT_CLOSED_MESSAGE: &str = "responses stream dropped before completion";

pub(super) struct ResponsesStreamCommonArgs {
    pub(super) task_tracker: RelayStreamTaskTracker,
    pub(super) tracking: Option<TrackingService>,
    pub(super) billing: Option<BillingEngine>,
    pub(super) billing_context: Option<ResponsesBillingContext>,
    pub(super) log: Option<LogService>,
    pub(super) log_context: Option<ResponsesLogContext>,
    pub(super) trace_id: Option<i64>,
    pub(super) tracked_request_id: Option<i64>,
    pub(super) tracked_execution_id: Option<i64>,
    pub(super) request_id: String,
    pub(super) started_at: Instant,
    pub(super) requested_model: String,
    pub(super) upstream_model: String,
    pub(super) upstream_request_id: Option<String>,
    pub(super) response_status_code: i32,
}

pub(super) struct NativeResponsesSseStreamArgs {
    pub(super) inner: BoxStream<'static, anyhow::Result<Bytes>>,
    pub(super) common: ResponsesStreamCommonArgs,
}

pub(super) struct BridgeResponsesSseStreamArgs {
    pub(super) inner: BoxStream<'static, anyhow::Result<ChatStreamItem>>,
    pub(super) common: ResponsesStreamCommonArgs,
}

pub(super) enum TrackedResponsesSseStream {
    Native(TrackedNativeResponsesSseStream),
    Bridge(TrackedBridgeResponsesSseStream),
}

impl TrackedResponsesSseStream {
    pub(super) fn native(args: NativeResponsesSseStreamArgs) -> Self {
        Self::Native(TrackedNativeResponsesSseStream::new(args))
    }

    pub(super) fn bridge(args: BridgeResponsesSseStreamArgs) -> Self {
        Self::Bridge(TrackedBridgeResponsesSseStream::new(args))
    }
}

impl Stream for TrackedResponsesSseStream {
    type Item = Result<Bytes, io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<Option<Self::Item>> {
        match self.as_mut().get_mut() {
            Self::Native(stream) => Pin::new(stream).poll_next(cx),
            Self::Bridge(stream) => Pin::new(stream).poll_next(cx),
        }
    }
}

#[derive(Clone, Debug, Default)]
struct ResponsesStreamProgress {
    first_token_ms: Option<i32>,
    saw_any_chunk: bool,
    saw_explicit_terminal_signal: bool,
    saw_completed_event: bool,
    last_usage: Option<Usage>,
    response_id: Option<String>,
    created_at: Option<i64>,
    upstream_model: Option<String>,
    output_text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ResponsesStreamSettlement {
    Success,
    Failure { status_code: i32, message: String },
}

type ResponsesStreamFinalizeContext = UsageStreamFinalizeContext;

fn build_responses_stream_finalize_context(
    args: ResponsesStreamCommonArgs,
) -> (RelayStreamTaskTracker, ResponsesStreamFinalizeContext) {
    let ResponsesStreamCommonArgs {
        task_tracker,
        tracking,
        billing,
        billing_context,
        log,
        log_context,
        trace_id,
        tracked_request_id,
        tracked_execution_id,
        request_id,
        started_at,
        requested_model,
        upstream_model,
        upstream_request_id,
        response_status_code,
    } = args;

    (
        task_tracker,
        ResponsesStreamFinalizeContext::new(
            UsageStreamFinalizeMeta::new(
                "/v1/responses",
                "openai/responses",
                request_id,
                requested_model,
                upstream_model,
                upstream_request_id,
                response_status_code,
            ),
            started_at,
            tracking,
            billing,
            billing_context.map(build_responses_billing_snapshot),
            log,
            log_context.map(build_responses_log_snapshot),
            trace_id,
            tracked_request_id,
            tracked_execution_id,
        ),
    )
}

impl RelayStreamFinalizer<ResponsesStreamProgress, ResponsesStreamSettlement>
    for ResponsesStreamFinalizeContext
{
    fn build_finalize_future(
        &self,
        progress: &ResponsesStreamProgress,
        settlement: ResponsesStreamSettlement,
    ) -> Option<BoxFinalizeFuture> {
        let first_token_ms = progress.first_token_ms.unwrap_or(0);
        let upstream_model =
            effective_responses_upstream_model(progress, self.meta().upstream_model.as_str());
        let shared_settlement = match settlement {
            ResponsesStreamSettlement::Success => UsageStreamFinalizeSettlement::success(
                upstream_model,
                first_token_ms,
                progress.last_usage.clone(),
            ),
            ResponsesStreamSettlement::Failure {
                status_code,
                message,
            } => UsageStreamFinalizeSettlement::failure(
                upstream_model,
                first_token_ms,
                status_code,
                message,
            ),
        };

        self.build_usage_finalize_future(shared_settlement)
    }
}

fn build_responses_billing_snapshot(
    context: ResponsesBillingContext,
) -> UsageStreamBillingSnapshot {
    UsageStreamBillingSnapshot {
        token_id: context.token_id,
        unlimited_quota: context.unlimited_quota,
        group_ratio: context.group_ratio,
        pre_consumed: context.pre_consumed,
        estimated_prompt_tokens: 0,
        price: context.price,
    }
}

fn build_responses_log_snapshot(context: ResponsesLogContext) -> UsageStreamLogSnapshot {
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

type NativeResponsesTrackedInner = crate::service::shared::stream::driver::TrackedRelayStream<
    BoxStream<'static, anyhow::Result<Bytes>>,
    NativeResponsesSseAdapter,
    ResponsesStreamFinalizeContext,
>;

type BridgeResponsesTrackedInner = crate::service::shared::stream::driver::TrackedRelayStream<
    BoxStream<'static, anyhow::Result<ChatStreamItem>>,
    BridgeResponsesSseAdapter,
    ResponsesStreamFinalizeContext,
>;

struct NativeResponsesSseAdapter {
    request_id: String,
    started_at: Instant,
    parser: SseParser,
}

struct BridgeResponsesSseAdapter {
    request_id: String,
    started_at: Instant,
    fallback_upstream_model: String,
    created_emitted: bool,
}

pub(super) struct TrackedNativeResponsesSseStream {
    inner: NativeResponsesTrackedInner,
}

impl TrackedNativeResponsesSseStream {
    fn new(args: NativeResponsesSseStreamArgs) -> Self {
        let (task_tracker, finalize_context) = build_responses_stream_finalize_context(args.common);
        let adapter = NativeResponsesSseAdapter {
            request_id: finalize_context.meta().request_id.clone(),
            started_at: finalize_context.started_at(),
            parser: SseParser::new(),
        };

        Self {
            inner: crate::service::shared::stream::driver::TrackedRelayStream::new(
                args.inner,
                adapter,
                task_tracker,
                finalize_context,
            ),
        }
    }
}

impl Stream for TrackedNativeResponsesSseStream {
    type Item = Result<Bytes, io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.inner).poll_next(cx)
    }
}

pub(super) struct TrackedBridgeResponsesSseStream {
    inner: BridgeResponsesTrackedInner,
}

impl TrackedBridgeResponsesSseStream {
    fn new(args: BridgeResponsesSseStreamArgs) -> Self {
        let (task_tracker, finalize_context) = build_responses_stream_finalize_context(args.common);
        let adapter = BridgeResponsesSseAdapter {
            request_id: finalize_context.meta().request_id.clone(),
            started_at: finalize_context.started_at(),
            fallback_upstream_model: finalize_context.meta().upstream_model.clone(),
            created_emitted: false,
        };

        Self {
            inner: crate::service::shared::stream::driver::TrackedRelayStream::new(
                args.inner,
                adapter,
                task_tracker,
                finalize_context,
            ),
        }
    }
}

impl Stream for TrackedBridgeResponsesSseStream {
    type Item = Result<Bytes, io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.inner).poll_next(cx)
    }
}

impl RelayStreamAdapter for NativeResponsesSseAdapter {
    type Item = Bytes;
    type Progress = ResponsesStreamProgress;
    type Settlement = ResponsesStreamSettlement;

    fn request_id(&self) -> &str {
        self.request_id.as_str()
    }

    fn observe(
        &mut self,
        progress: &mut Self::Progress,
        item: Self::Item,
        pending_frames: &mut VecDeque<Bytes>,
    ) -> anyhow::Result<()> {
        observe_native_chunk(&item, &mut self.parser, progress, self.started_at)?;
        pending_frames.push_back(item);
        Ok(())
    }

    fn settle_on_error(
        &self,
        progress: &Self::Progress,
        error: &anyhow::Error,
    ) -> Self::Settlement {
        resolve_native_stream_settlement(progress, Some(error))
    }

    fn settle_on_eof(
        &mut self,
        progress: &Self::Progress,
        _pending_frames: &mut VecDeque<Bytes>,
    ) -> anyhow::Result<Self::Settlement> {
        Ok(resolve_native_stream_settlement(progress, None))
    }

    fn settle_on_cancel(&self) -> Self::Settlement {
        ResponsesStreamSettlement::Failure {
            status_code: DOWNSTREAM_CLIENT_CLOSED_STATUS_CODE,
            message: DOWNSTREAM_CLIENT_CLOSED_MESSAGE.to_string(),
        }
    }
}

impl RelayStreamAdapter for BridgeResponsesSseAdapter {
    type Item = ChatStreamItem;
    type Progress = ResponsesStreamProgress;
    type Settlement = ResponsesStreamSettlement;

    fn request_id(&self) -> &str {
        self.request_id.as_str()
    }

    fn observe(
        &mut self,
        progress: &mut Self::Progress,
        item: Self::Item,
        pending_frames: &mut VecDeque<Bytes>,
    ) -> anyhow::Result<()> {
        if item.is_terminal() {
            progress.saw_explicit_terminal_signal = true;
        }
        let Some(chunk) = item.into_chunk() else {
            return Ok(());
        };

        progress.saw_any_chunk = true;
        if progress.response_id.is_none() && !chunk.id.is_empty() {
            progress.response_id = Some(chunk.id.clone());
        }
        if progress.created_at.is_none() && chunk.created > 0 {
            progress.created_at = Some(chunk.created);
        }
        if progress.upstream_model.is_none() && !chunk.model.is_empty() {
            progress.upstream_model = Some(chunk.model.clone());
        }
        if !self.created_emitted && progress.response_id.is_some() {
            self.created_emitted = true;
            pending_frames.push_back(build_sse_frame(&serde_json::json!({
                "type": "response.created",
                "response": {
                    "id": progress.response_id.clone().unwrap_or_default(),
                    "object": "response",
                    "created_at": progress.created_at.unwrap_or_else(current_unix_timestamp),
                    "model": effective_responses_upstream_model(progress, &self.fallback_upstream_model),
                    "status": "in_progress"
                }
            }))?);
        }

        for choice in &chunk.choices {
            if let Some(text) = choice.delta.content.as_ref()
                && !text.is_empty()
            {
                if progress.first_token_ms.is_none() {
                    progress.first_token_ms = Some(self.started_at.elapsed().as_millis() as i32);
                }
                progress.output_text.push_str(text);
                pending_frames.push_back(build_sse_frame(&serde_json::json!({
                    "type": "response.output_text.delta",
                    "delta": text,
                }))?);
            }
        }

        if let Some(usage) = chunk.usage.clone() {
            progress.last_usage = Some(usage);
        }

        Ok(())
    }

    fn settle_on_error(
        &self,
        progress: &Self::Progress,
        error: &anyhow::Error,
    ) -> Self::Settlement {
        resolve_bridge_stream_settlement(progress, Some(error))
    }

    fn settle_on_eof(
        &mut self,
        progress: &Self::Progress,
        pending_frames: &mut VecDeque<Bytes>,
    ) -> anyhow::Result<Self::Settlement> {
        let settlement = resolve_bridge_stream_settlement(progress, None);
        if settlement == ResponsesStreamSettlement::Success {
            pending_frames.push_back(build_sse_frame(&serde_json::json!({
                "type": "response.completed",
                "response": build_completed_bridge_response(
                    progress,
                    &self.request_id,
                    &self.fallback_upstream_model,
                ),
            }))?);
            pending_frames.push_back(build_done_sse_frame());
        }
        Ok(settlement)
    }

    fn settle_on_cancel(&self) -> Self::Settlement {
        ResponsesStreamSettlement::Failure {
            status_code: DOWNSTREAM_CLIENT_CLOSED_STATUS_CODE,
            message: DOWNSTREAM_CLIENT_CLOSED_MESSAGE.to_string(),
        }
    }
}

fn observe_native_chunk(
    chunk: &Bytes,
    parser: &mut SseParser,
    progress: &mut ResponsesStreamProgress,
    started_at: Instant,
) -> anyhow::Result<()> {
    let events = parser
        .feed(chunk)
        .context("failed to parse responses SSE bytes")?;
    for event_text in events {
        let Some(event) = parse_sse_event(&event_text) else {
            continue;
        };
        progress.saw_any_chunk = true;
        observe_native_event(progress, &event, started_at);
    }
    Ok(())
}

fn observe_native_event(
    progress: &mut ResponsesStreamProgress,
    event: &SseEvent,
    started_at: Instant,
) {
    let data = event.data.trim();
    if data.is_empty() {
        return;
    }
    if data == "[DONE]" {
        progress.saw_explicit_terminal_signal = true;
        return;
    }

    let Ok(payload) = serde_json::from_str::<serde_json::Value>(data) else {
        return;
    };

    if progress.first_token_ms.is_none() && is_output_text_delta_event(&payload) {
        progress.first_token_ms = Some(started_at.elapsed().as_millis() as i32);
    }
    if progress.upstream_model.is_none()
        && let Some(model) = extract_response_model(&payload)
    {
        progress.upstream_model = Some(model);
    }
    if progress.response_id.is_none()
        && let Some(response_id) = extract_response_id(&payload)
    {
        progress.response_id = Some(response_id);
    }
    if progress.created_at.is_none()
        && let Some(created_at) = extract_response_created_at(&payload)
    {
        progress.created_at = Some(created_at);
    }
    if payload
        .get("type")
        .and_then(|value| value.as_str())
        .is_some_and(|event_type| event_type == "response.completed")
    {
        progress.saw_completed_event = true;
    }
    if let Some(usage) = extract_response_usage(&payload) {
        progress.last_usage = Some(usage);
    }
}

fn resolve_native_stream_settlement(
    progress: &ResponsesStreamProgress,
    stream_error: Option<&anyhow::Error>,
) -> ResponsesStreamSettlement {
    if let Some(error) = stream_error {
        return ResponsesStreamSettlement::Failure {
            status_code: super::stream_error_status_code(error),
            message: super::stream_error_message(error),
        };
    }

    if progress.saw_completed_event && progress.last_usage.is_some() {
        ResponsesStreamSettlement::Success
    } else if progress.saw_any_chunk {
        ResponsesStreamSettlement::Failure {
            status_code: 0,
            message: "responses stream ended without completed usage event".to_string(),
        }
    } else {
        ResponsesStreamSettlement::Failure {
            status_code: 0,
            message: "responses stream ended before any relay chunk".to_string(),
        }
    }
}

fn resolve_bridge_stream_settlement(
    progress: &ResponsesStreamProgress,
    stream_error: Option<&anyhow::Error>,
) -> ResponsesStreamSettlement {
    if let Some(error) = stream_error {
        return ResponsesStreamSettlement::Failure {
            status_code: super::stream_error_status_code(error),
            message: super::stream_error_message(error),
        };
    }

    if progress.saw_explicit_terminal_signal && progress.last_usage.is_some() {
        ResponsesStreamSettlement::Success
    } else if progress.saw_any_chunk {
        ResponsesStreamSettlement::Failure {
            status_code: 0,
            message: "responses bridge stream ended without terminal usage chunk".to_string(),
        }
    } else {
        ResponsesStreamSettlement::Failure {
            status_code: 0,
            message: "responses bridge stream ended before any relay chunk".to_string(),
        }
    }
}

fn effective_responses_upstream_model(
    progress: &ResponsesStreamProgress,
    fallback_upstream_model: &str,
) -> String {
    progress
        .upstream_model
        .clone()
        .filter(|model| !model.is_empty())
        .unwrap_or_else(|| fallback_upstream_model.to_string())
}

fn build_completed_bridge_response(
    progress: &ResponsesStreamProgress,
    fallback_request_id: &str,
    fallback_upstream_model: &str,
) -> ResponsesResponse {
    ResponsesResponse {
        id: progress
            .response_id
            .clone()
            .unwrap_or_else(|| fallback_request_id.to_string()),
        object: "response".into(),
        created_at: progress.created_at.unwrap_or_else(current_unix_timestamp),
        model: effective_responses_upstream_model(progress, fallback_upstream_model),
        status: "completed".into(),
        usage: progress.last_usage.as_ref().map(response_usage_from_usage),
        output_text: (!progress.output_text.is_empty()).then_some(progress.output_text.clone()),
        extra: serde_json::Map::new(),
    }
}

fn response_usage_from_usage(usage: &Usage) -> ResponseUsage {
    ResponseUsage {
        input_tokens: usage.prompt_tokens,
        output_tokens: usage.completion_tokens,
        total_tokens: usage.total_tokens,
        input_tokens_details: (usage.cached_tokens > 0).then_some(ResponseInputTokensDetails {
            cached_tokens: usage.cached_tokens,
        }),
        output_tokens_details: (usage.reasoning_tokens > 0).then_some(
            ResponseOutputTokensDetails {
                reasoning_tokens: usage.reasoning_tokens,
            },
        ),
    }
}

fn extract_response_id(payload: &serde_json::Value) -> Option<String> {
    payload
        .get("response")
        .and_then(|response| response.get("id"))
        .or_else(|| payload.get("id"))
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned)
}

fn extract_response_created_at(payload: &serde_json::Value) -> Option<i64> {
    payload
        .get("response")
        .and_then(|response| {
            response
                .get("created_at")
                .or_else(|| response.get("created"))
        })
        .or_else(|| payload.get("created_at").or_else(|| payload.get("created")))
        .and_then(|value| value.as_i64())
}

fn parse_sse_event(event_text: &str) -> Option<SseEvent> {
    let mut event_name = None;
    let mut data_lines = Vec::new();

    for line in event_text.lines() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() || line.starts_with(':') {
            continue;
        }
        if let Some(value) = line.strip_prefix("event:") {
            event_name = Some(value.trim().to_string());
            continue;
        }
        if let Some(value) = line.strip_prefix("data:") {
            data_lines.push(value.trim().to_string());
        }
    }

    if data_lines.is_empty() {
        None
    } else {
        Some(SseEvent {
            event: event_name,
            data: data_lines.join("\n"),
        })
    }
}

fn build_sse_frame(payload: &serde_json::Value) -> anyhow::Result<Bytes> {
    let json =
        serde_json::to_string(payload).context("failed to serialize responses SSE payload")?;
    Ok(Bytes::from(format!("data: {json}\n\n")))
}

fn build_done_sse_frame() -> Bytes {
    Bytes::from_static(b"data: [DONE]\n\n")
}

fn current_unix_timestamp() -> i64 {
    chrono::Utc::now().timestamp()
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use bytes::Bytes;
    use futures::StreamExt as _;
    use futures::stream;
    use summer_ai_core::stream::ChatStreamItem;
    use summer_ai_core::types::chat::ChatCompletionChunk;
    use summer_ai_core::types::common::{Delta, FinishReason, Usage};

    use super::{
        BridgeResponsesSseStreamArgs, NativeResponsesSseStreamArgs, ResponsesStreamCommonArgs,
        ResponsesStreamProgress, ResponsesStreamSettlement, SseParser, TrackedResponsesSseStream,
        observe_native_chunk, resolve_bridge_stream_settlement, resolve_native_stream_settlement,
    };
    use crate::plugin::RelayStreamTaskTracker;

    #[test]
    fn native_tracker_extracts_completed_usage_from_sse_payload() {
        let mut parser = SseParser::new();
        let mut progress = ResponsesStreamProgress::default();
        let started_at = Instant::now();

        observe_native_chunk(
            &Bytes::from_static(
                b"data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_123\",\"created_at\":1700000000,\"model\":\"gpt-5.4\"}}\n\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"hel\"}\n\ndata: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_123\",\"model\":\"gpt-5.4\",\"usage\":{\"input_tokens\":12,\"output_tokens\":7,\"total_tokens\":19}}}\n\ndata: [DONE]\n\n",
            ),
            &mut parser,
            &mut progress,
            started_at,
        )
        .expect("observe native chunk");

        assert_eq!(progress.response_id.as_deref(), Some("resp_123"));
        assert_eq!(progress.upstream_model.as_deref(), Some("gpt-5.4"));
        assert!(progress.saw_completed_event);
        assert!(progress.saw_explicit_terminal_signal);
        assert_eq!(progress.last_usage.expect("usage").total_tokens, 19);
    }

    #[test]
    fn native_settlement_requires_completed_usage() {
        let success = ResponsesStreamProgress {
            saw_any_chunk: true,
            saw_completed_event: true,
            last_usage: Some(Usage {
                prompt_tokens: 12,
                completion_tokens: 7,
                total_tokens: 19,
                cached_tokens: 0,
                reasoning_tokens: 0,
            }),
            ..Default::default()
        };
        assert_eq!(
            resolve_native_stream_settlement(&success, None),
            ResponsesStreamSettlement::Success
        );

        let truncated = ResponsesStreamProgress {
            saw_any_chunk: true,
            first_token_ms: Some(12),
            ..Default::default()
        };
        assert!(matches!(
            resolve_native_stream_settlement(&truncated, None),
            ResponsesStreamSettlement::Failure { .. }
        ));
    }

    #[test]
    fn bridge_settlement_requires_terminal_usage() {
        let success = ResponsesStreamProgress {
            saw_any_chunk: true,
            saw_explicit_terminal_signal: true,
            last_usage: Some(Usage {
                prompt_tokens: 12,
                completion_tokens: 7,
                total_tokens: 19,
                cached_tokens: 0,
                reasoning_tokens: 0,
            }),
            ..Default::default()
        };
        assert_eq!(
            resolve_bridge_stream_settlement(&success, None),
            ResponsesStreamSettlement::Success
        );

        let missing_terminal = ResponsesStreamProgress {
            saw_any_chunk: true,
            last_usage: success.last_usage,
            ..Default::default()
        };
        assert!(matches!(
            resolve_bridge_stream_settlement(&missing_terminal, None),
            ResponsesStreamSettlement::Failure { .. }
        ));
    }

    #[tokio::test]
    async fn bridge_stream_emits_created_delta_completed_and_done() {
        let chunk_a = ChatCompletionChunk {
            id: "chatcmpl_stream_1".into(),
            object: "chat.completion.chunk".into(),
            created: 1_700_000_000,
            model: "gpt-5.4".into(),
            choices: vec![summer_ai_core::types::chat::ChunkChoice {
                index: 0,
                delta: Delta {
                    role: Some("assistant".into()),
                    content: Some("hel".into()),
                    reasoning_content: None,
                    tool_calls: None,
                },
                finish_reason: None,
            }],
            usage: None,
        };
        let chunk_b = ChatCompletionChunk {
            id: "chatcmpl_stream_1".into(),
            object: "chat.completion.chunk".into(),
            created: 1_700_000_000,
            model: "gpt-5.4".into(),
            choices: vec![summer_ai_core::types::chat::ChunkChoice {
                index: 0,
                delta: Delta {
                    role: None,
                    content: Some("lo".into()),
                    reasoning_content: None,
                    tool_calls: None,
                },
                finish_reason: Some(FinishReason::Stop),
            }],
            usage: Some(Usage {
                prompt_tokens: 12,
                completion_tokens: 7,
                total_tokens: 19,
                cached_tokens: 1,
                reasoning_tokens: 2,
            }),
        };
        let inner = stream::iter(vec![
            Ok(ChatStreamItem::chunk(chunk_a)),
            Ok(ChatStreamItem::terminal_chunk(chunk_b)),
            Ok(ChatStreamItem::terminal()),
        ]);

        let stream = TrackedResponsesSseStream::bridge(BridgeResponsesSseStreamArgs {
            inner: Box::pin(inner),
            common: ResponsesStreamCommonArgs {
                task_tracker: RelayStreamTaskTracker::new(),
                tracking: None,
                billing: None,
                billing_context: None,
                log: None,
                log_context: None,
                trace_id: None,
                tracked_request_id: None,
                tracked_execution_id: None,
                request_id: "req_123".into(),
                started_at: Instant::now(),
                requested_model: "gpt-5.4".into(),
                upstream_model: "gpt-5.4".into(),
                upstream_request_id: Some("up_123".into()),
                response_status_code: 200,
            },
        });

        let frames = stream
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .map(|item| String::from_utf8(item.expect("stream item").to_vec()).expect("utf8"))
            .collect::<Vec<_>>()
            .join("");

        assert!(frames.contains("\"type\":\"response.created\""));
        assert!(frames.contains("\"type\":\"response.output_text.delta\""));
        assert!(frames.contains("\"delta\":\"hel\""));
        assert!(frames.contains("\"delta\":\"lo\""));
        assert!(frames.contains("\"type\":\"response.completed\""));
        assert!(frames.contains("\"output_text\":\"hello\""));
        assert!(frames.contains("data: [DONE]"));
    }

    #[tokio::test]
    async fn native_stream_passthrough_keeps_original_frames() {
        let stream = TrackedResponsesSseStream::native(NativeResponsesSseStreamArgs {
            inner: Box::pin(stream::iter(vec![Ok(Bytes::from_static(
                b"data: {\"type\":\"response.created\"}\n\n",
            ))])),
            common: ResponsesStreamCommonArgs {
                task_tracker: RelayStreamTaskTracker::new(),
                tracking: None,
                billing: None,
                billing_context: None,
                log: None,
                log_context: None,
                trace_id: None,
                tracked_request_id: None,
                tracked_execution_id: None,
                request_id: "req_native".into(),
                started_at: Instant::now(),
                requested_model: "gpt-5.4".into(),
                upstream_model: "gpt-5.4".into(),
                upstream_request_id: None,
                response_status_code: 200,
            },
        });

        let frames = stream
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .map(|item| String::from_utf8(item.expect("stream item").to_vec()).expect("utf8"))
            .collect::<Vec<_>>();
        assert_eq!(frames, vec!["data: {\"type\":\"response.created\"}\n\n"]);
    }
}
