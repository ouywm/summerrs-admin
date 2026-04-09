use std::io;
use std::pin::Pin;
use std::task::{Context as TaskContext, Poll};
use std::time::Instant;

use anyhow::Context;
use futures::Stream;
use futures::stream::BoxStream;
use summer_ai_billing::service::engine::BillingEngine;
use summer_ai_core::types::common::Usage;

use super::{ChatBillingContext, ChatLogContext};
use crate::plugin::RelayStreamTaskTracker;
use crate::service::log::LogService;
use crate::service::shared::stream::driver::{
    BoxFinalizeFuture, RelayStreamAdapter, RelayStreamFinalizer,
};
use crate::service::shared::stream::usage_tracking_finalize::{
    UsageStreamBillingContext, UsageStreamFinalizeContext, UsageStreamFinalizeMeta,
    UsageStreamFinalizeSettlement, UsageStreamLogContext,
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

type ChatStreamFinalizeContext = UsageStreamFinalizeContext<ChatLogContext, ChatBillingContext>;

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
                args.upstream_model,
                args.upstream_request_id,
                args.response_status_code,
            ),
            args.started_at,
            args.tracking,
            args.billing,
            args.billing_context,
            args.log,
            args.log_context,
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
    billing: &ChatBillingContext,
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
            status_code: super::stream_error_status_code(error),
            message: super::stream_error_message(error),
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

impl UsageStreamBillingContext for ChatBillingContext {
    fn token_id(&self) -> i64 {
        self.token_id
    }

    fn unlimited_quota(&self) -> bool {
        self.unlimited_quota
    }

    fn group_ratio(&self) -> f64 {
        self.group_ratio
    }

    fn pre_consumed(&self) -> i64 {
        self.pre_consumed
    }

    fn price(&self) -> &summer_ai_billing::service::channel_model_price::ResolvedModelPrice {
        &self.price
    }
}

impl UsageStreamLogContext for ChatLogContext {
    fn token_info(&self) -> &crate::service::token::TokenInfo {
        &self.token_info
    }

    fn channel_id(&self) -> i64 {
        self.channel_id
    }

    fn channel_name(&self) -> &str {
        self.channel_name.as_str()
    }

    fn account_id(&self) -> i64 {
        self.account_id
    }

    fn account_name(&self) -> &str {
        self.account_name.as_str()
    }

    fn execution_id(&self) -> i64 {
        self.execution_id
    }

    fn requested_model(&self) -> &str {
        self.requested_model.as_str()
    }

    fn client_ip(&self) -> &str {
        self.client_ip.as_str()
    }

    fn user_agent(&self) -> &str {
        self.user_agent.as_str()
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
        let billing = ChatBillingContext {
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
        };
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
