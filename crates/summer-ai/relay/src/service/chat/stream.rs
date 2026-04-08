use std::io;
use std::pin::Pin;
use std::task::{Context as TaskContext, Poll};
use std::time::Instant;

use anyhow::Context;
use futures::Stream;
use futures::stream::BoxStream;

use crate::service::tracking::TrackingService;

const DOWNSTREAM_CLIENT_CLOSED_STATUS_CODE: i32 = 499;
const DOWNSTREAM_CLIENT_CLOSED_MESSAGE: &str = "chat stream dropped before completion";

#[derive(Clone, Debug, Default)]
struct ChatStreamProgress {
    first_token_ms: Option<i32>,
    last_usage: Option<summer_ai_core::types::common::Usage>,
    saw_terminal_finish_reason: bool,
    saw_any_chunk: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ChatStreamSettlement {
    Success,
    Failure { status_code: i32, message: String },
}

pub(super) struct TrackedChatSseStreamArgs {
    pub(super) inner:
        BoxStream<'static, anyhow::Result<summer_ai_core::types::chat::ChatCompletionChunk>>,
    pub(super) tracking: Option<TrackingService>,
    pub(super) tracked_request_id: Option<i64>,
    pub(super) tracked_execution_id: Option<i64>,
    pub(super) request_id: String,
    pub(super) started_at: Instant,
    pub(super) upstream_model: String,
    pub(super) upstream_request_id: Option<String>,
    pub(super) response_status_code: i32,
}

pub(super) struct TrackedChatSseStream {
    inner: BoxStream<'static, anyhow::Result<summer_ai_core::types::chat::ChatCompletionChunk>>,
    tracking: Option<TrackingService>,
    tracked_request_id: Option<i64>,
    tracked_execution_id: Option<i64>,
    request_id: String,
    started_at: Instant,
    upstream_model: String,
    upstream_request_id: Option<String>,
    response_status_code: i32,
    progress: ChatStreamProgress,
    stream_settled: bool,
    done_emitted: bool,
}

impl TrackedChatSseStream {
    pub(super) fn new(args: TrackedChatSseStreamArgs) -> Self {
        Self {
            inner: args.inner,
            tracking: args.tracking,
            tracked_request_id: args.tracked_request_id,
            tracked_execution_id: args.tracked_execution_id,
            request_id: args.request_id,
            started_at: args.started_at,
            upstream_model: args.upstream_model,
            upstream_request_id: args.upstream_request_id,
            response_status_code: args.response_status_code,
            progress: ChatStreamProgress::default(),
            stream_settled: false,
            done_emitted: false,
        }
    }

    fn observe_chunk(&mut self, chunk: &summer_ai_core::types::chat::ChatCompletionChunk) {
        self.progress.saw_any_chunk = true;
        if chunk
            .choices
            .iter()
            .any(|choice| choice.finish_reason.is_some())
        {
            self.progress.saw_terminal_finish_reason = true;
        }
        if let Some(usage) = chunk.usage.clone() {
            self.progress.last_usage = Some(usage);
        }
        if self.progress.first_token_ms.is_none() && chunk_contains_visible_output(chunk) {
            let measured = self.started_at.elapsed().as_millis() as i32;
            self.progress.first_token_ms = Some(measured);
            self.spawn_record_first_token(measured);
        }
    }

    fn spawn_record_first_token(&self, first_token_ms: i32) {
        let Some(tracking) = self.tracking.clone() else {
            return;
        };
        let request_id = self.request_id.clone();
        let tracked_request_id = self.tracked_request_id;
        let tracked_execution_id = self.tracked_execution_id;

        tokio::spawn(async move {
            if let Some(request_pk) = tracked_request_id
                && let Err(error) = tracking
                    .record_request_first_token(request_pk, first_token_ms)
                    .await
            {
                tracing::warn!(request_id, error = %error, "failed to update request first_token_ms");
            }

            if let Some(execution_id) = tracked_execution_id
                && let Err(error) = tracking
                    .record_execution_first_token(execution_id, first_token_ms)
                    .await
            {
                tracing::warn!(request_id, error = %error, "failed to update request_execution first_token_ms");
            }
        });
    }

    fn spawn_finalize(&mut self, settlement: ChatStreamSettlement) {
        if self.stream_settled {
            return;
        }
        self.stream_settled = true;

        let Some(tracking) = self.tracking.clone() else {
            return;
        };

        let tracked_request_id = self.tracked_request_id;
        let tracked_execution_id = self.tracked_execution_id;
        let request_id = self.request_id.clone();
        let upstream_model = self.upstream_model.clone();
        let upstream_request_id = self.upstream_request_id.clone();
        let response_status_code = self.response_status_code;
        let duration_ms = self.started_at.elapsed().as_millis() as i32;
        let first_token_ms = self.progress.first_token_ms.unwrap_or(0);

        tokio::spawn(async move {
            match settlement {
                ChatStreamSettlement::Success => {
                    if let Some(request_pk) = tracked_request_id
                        && let Err(error) = tracking
                            .finish_request_stream_success(
                                request_pk,
                                &upstream_model,
                                response_status_code,
                                duration_ms,
                                first_token_ms,
                            )
                            .await
                    {
                        tracing::warn!(request_id, error = %error, "failed to finalize request streaming success");
                    }

                    if let Some(execution_id) = tracked_execution_id
                        && let Err(error) = tracking
                            .finish_execution_stream_success(
                                execution_id,
                                upstream_request_id.as_deref(),
                                response_status_code,
                                duration_ms,
                                first_token_ms,
                            )
                            .await
                    {
                        tracing::warn!(request_id, error = %error, "failed to finalize request_execution streaming success");
                    }
                }
                ChatStreamSettlement::Failure {
                    status_code,
                    message,
                } => {
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
                        tracing::warn!(request_id, error = %error, "failed to finalize request streaming failure");
                    }

                    if let Some(execution_id) = tracked_execution_id
                        && let Err(error) = tracking
                            .finish_execution_stream_failure(
                                execution_id,
                                upstream_request_id.as_deref(),
                                status_code,
                                &message,
                                None,
                                duration_ms,
                                first_token_ms,
                            )
                            .await
                    {
                        tracing::warn!(request_id, error = %error, "failed to finalize request_execution streaming failure");
                    }
                }
            }
        });
    }
}

impl Stream for TrackedChatSseStream {
    type Item = Result<bytes::Bytes, io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<Option<Self::Item>> {
        if self.done_emitted {
            return Poll::Ready(None);
        }

        match Pin::new(&mut self.inner).poll_next(cx) {
            Poll::Ready(Some(Ok(chunk))) => {
                self.observe_chunk(&chunk);
                match build_chat_chunk_sse_frame(&chunk) {
                    Ok(frame) => Poll::Ready(Some(Ok(frame))),
                    Err(error) => {
                        let message = error.to_string();
                        self.spawn_finalize(ChatStreamSettlement::Failure {
                            status_code: 0,
                            message: message.clone(),
                        });
                        self.done_emitted = true;
                        Poll::Ready(Some(Err(io::Error::other(message))))
                    }
                }
            }
            Poll::Ready(Some(Err(error))) => {
                tracing::warn!(request_id = self.request_id.as_str(), error = %error, "chat stream chunk read failed");
                let settlement = resolve_chat_stream_settlement(&self.progress, Some(&error));
                let message = error.to_string();
                self.spawn_finalize(settlement);
                self.done_emitted = true;
                Poll::Ready(Some(Err(io::Error::other(message))))
            }
            Poll::Ready(None) => {
                let settlement = resolve_chat_stream_settlement(&self.progress, None);
                match settlement {
                    ChatStreamSettlement::Success => {
                        self.spawn_finalize(ChatStreamSettlement::Success);
                        self.done_emitted = true;
                        Poll::Ready(Some(Ok(build_done_sse_frame())))
                    }
                    failure @ ChatStreamSettlement::Failure { .. } => {
                        self.spawn_finalize(failure);
                        Poll::Ready(None)
                    }
                }
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

impl Drop for TrackedChatSseStream {
    fn drop(&mut self) {
        if self.stream_settled {
            return;
        }

        self.spawn_finalize(ChatStreamSettlement::Failure {
            status_code: DOWNSTREAM_CLIENT_CLOSED_STATUS_CODE,
            message: DOWNSTREAM_CLIENT_CLOSED_MESSAGE.to_string(),
        });
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

    if progress.saw_terminal_finish_reason
        || progress.last_usage.is_some()
        || progress.first_token_ms.is_some()
    {
        ChatStreamSettlement::Success
    } else if progress.saw_any_chunk {
        ChatStreamSettlement::Failure {
            status_code: 0,
            message: "chat stream ended without terminal marker or usage".to_string(),
        }
    } else {
        ChatStreamSettlement::Failure {
            status_code: 0,
            message: "chat stream ended before any relay chunk".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use futures::StreamExt as _;
    use futures::stream;
    use summer_ai_core::types::chat::ChatCompletionChunk;
    use summer_ai_core::types::common::{Delta, Usage};

    use super::{
        ChatStreamProgress, ChatStreamSettlement, TrackedChatSseStream, TrackedChatSseStreamArgs,
        build_chat_chunk_sse_frame, chunk_contains_visible_output, resolve_chat_stream_settlement,
    };

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
        let usage = Usage {
            prompt_tokens: 10,
            completion_tokens: 5,
            total_tokens: 15,
            cached_tokens: 0,
            reasoning_tokens: 0,
        };

        assert!(matches!(
            resolve_chat_stream_settlement(
                &ChatStreamProgress {
                    saw_terminal_finish_reason: true,
                    ..Default::default()
                },
                None,
            ),
            ChatStreamSettlement::Success
        ));

        assert!(matches!(
            resolve_chat_stream_settlement(
                &ChatStreamProgress {
                    last_usage: Some(usage.clone()),
                    ..Default::default()
                },
                None,
            ),
            ChatStreamSettlement::Success
        ));

        assert!(matches!(
            resolve_chat_stream_settlement(
                &ChatStreamProgress {
                    first_token_ms: Some(42),
                    saw_any_chunk: true,
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
                Ok(text_chunk),
                Err(anyhow::anyhow!("synthetic stream failure")),
            ])
            .boxed(),
            tracking: None,
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
        let usage_only_chunk: ChatCompletionChunk = serde_json::from_value(serde_json::json!({
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

        let stream = TrackedChatSseStream::new(TrackedChatSseStreamArgs {
            inner: stream::iter(vec![Ok(usage_only_chunk)]).boxed(),
            tracking: None,
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
}
