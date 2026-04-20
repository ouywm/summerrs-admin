//! OpenAI converter——canonical 就是 OpenAI-flat 格式，所以 `to_canonical` / `from_canonical`
//! 都是 identity。流式是唯一一处非 identity：canonical `ChatStreamEvent` 被重组成
//! [`OpenAIStreamChunk`]（`data: {json}\n\n` + 末尾 `data: [DONE]\n\n` 由
//! `stream_driver` 负责）。

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use summer_ai_core::{
    AdapterError, AdapterResult, ChatRequest, ChatResponse, ChatStreamEvent, FinishReason,
    ToolCallDelta, Usage,
};

use super::{IngressConverter, IngressCtx, IngressFormat, OpenAIStreamState, StreamConvertState};

/// OpenAI 入口协议的 converter（非流 identity / 流式重组）。
pub struct OpenAIIngress;

impl IngressConverter for OpenAIIngress {
    type ClientRequest = ChatRequest;
    type ClientResponse = ChatResponse;
    type ClientStreamEvent = OpenAIStreamChunk;

    const FORMAT: IngressFormat = IngressFormat::OpenAI;

    fn to_canonical(req: Self::ClientRequest, _ctx: &IngressCtx) -> AdapterResult<ChatRequest> {
        Ok(req)
    }

    fn from_canonical(
        resp: ChatResponse,
        _ctx: &IngressCtx,
    ) -> AdapterResult<Self::ClientResponse> {
        Ok(resp)
    }

    fn from_canonical_stream_event(
        event: ChatStreamEvent,
        state: &mut StreamConvertState,
        ctx: &IngressCtx,
    ) -> AdapterResult<Vec<Self::ClientStreamEvent>> {
        let StreamConvertState::Openai(s) = state else {
            return Err(AdapterError::Unsupported {
                adapter: "openai_ingress",
                feature: "stream_convert_state_mismatch",
            });
        };
        Ok(from_canonical_stream_event_impl(event, s, ctx))
    }
}

// ---------------------------------------------------------------------------
// OpenAI chunk wire 类型
// ---------------------------------------------------------------------------

/// OpenAI `chat.completion.chunk`。
#[derive(Debug, Clone, Serialize)]
pub struct OpenAIStreamChunk {
    pub id: String,
    pub object: &'static str,
    pub created: i64,
    pub model: String,
    pub choices: Vec<OpenAIStreamChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpenAIStreamChoice {
    pub index: u32,
    pub delta: OpenAIStreamDelta,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct OpenAIStreamDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<OpenAIStreamToolCall>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpenAIStreamToolCall {
    pub index: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub function: OpenAIStreamToolCallFunction,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct OpenAIStreamToolCallFunction {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<String>,
}

// ---------------------------------------------------------------------------
// 重组：canonical event → OpenAI chunk(s)
// ---------------------------------------------------------------------------

fn from_canonical_stream_event_impl(
    event: ChatStreamEvent,
    state: &mut OpenAIStreamState,
    ctx: &IngressCtx,
) -> Vec<OpenAIStreamChunk> {
    match event {
        ChatStreamEvent::Start { model, .. } => {
            if state.chunk_id.is_empty() {
                state.chunk_id = generate_chatcmpl_id();
            }
            if state.created == 0 {
                state.created = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);
            }
            state.model = if !model.is_empty() {
                model
            } else {
                ctx.actual_model.clone()
            };
            state.role_emitted = true;
            vec![chunk(
                state,
                OpenAIStreamDelta {
                    role: Some("assistant".to_string()),
                    ..Default::default()
                },
                None,
            )]
        }
        ChatStreamEvent::TextDelta { text } => {
            let mut chunks = Vec::with_capacity(2);
            if !state.role_emitted {
                // 适配上游没发 Start 的情况——自动先补一个 role chunk。
                prime_state_if_needed(state, ctx);
                chunks.push(chunk(
                    state,
                    OpenAIStreamDelta {
                        role: Some("assistant".to_string()),
                        ..Default::default()
                    },
                    None,
                ));
                state.role_emitted = true;
            }
            chunks.push(chunk(
                state,
                OpenAIStreamDelta {
                    content: Some(text),
                    ..Default::default()
                },
                None,
            ));
            chunks
        }
        ChatStreamEvent::ReasoningDelta { .. } => {
            // OpenAI Chat Completions wire 不透传 reasoning delta——丢弃。
            // 后续需要暴露给客户端时，再在这里映射到 `delta.reasoning` 或额外字段。
            Vec::new()
        }
        ChatStreamEvent::ToolCallDelta(tc) => {
            let ToolCallDelta {
                index,
                id,
                name,
                arguments_delta,
            } = tc;
            vec![chunk(
                state,
                OpenAIStreamDelta {
                    tool_calls: Some(vec![OpenAIStreamToolCall {
                        index,
                        id,
                        kind: "function",
                        function: OpenAIStreamToolCallFunction {
                            name,
                            arguments: arguments_delta,
                        },
                    }]),
                    ..Default::default()
                },
                None,
            )]
        }
        ChatStreamEvent::End(end) => {
            prime_state_if_needed(state, ctx);
            let finish = end
                .finish_reason
                .map(finish_reason_to_wire)
                .unwrap_or_else(|| "stop".to_string());
            let mut chunks = Vec::with_capacity(2);
            chunks.push(chunk(state, OpenAIStreamDelta::default(), Some(finish)));
            if let Some(usage) = end.usage {
                chunks.push(OpenAIStreamChunk {
                    id: state.chunk_id.clone(),
                    object: "chat.completion.chunk",
                    created: state.created,
                    model: state.model.clone(),
                    choices: Vec::new(),
                    usage: Some(usage),
                });
            }
            chunks
        }
    }
}

fn chunk(
    state: &OpenAIStreamState,
    delta: OpenAIStreamDelta,
    finish_reason: Option<String>,
) -> OpenAIStreamChunk {
    OpenAIStreamChunk {
        id: state.chunk_id.clone(),
        object: "chat.completion.chunk",
        created: state.created,
        model: state.model.clone(),
        choices: vec![OpenAIStreamChoice {
            index: 0,
            delta,
            finish_reason,
        }],
        usage: None,
    }
}

fn prime_state_if_needed(state: &mut OpenAIStreamState, ctx: &IngressCtx) {
    if state.chunk_id.is_empty() {
        state.chunk_id = generate_chatcmpl_id();
    }
    if state.created == 0 {
        state.created = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
    }
    if state.model.is_empty() {
        state.model = ctx.actual_model.clone();
    }
}

fn finish_reason_to_wire(reason: FinishReason) -> String {
    match reason {
        FinishReason::Stop => "stop",
        FinishReason::Length => "length",
        FinishReason::ContentFilter => "content_filter",
        FinishReason::ToolCalls => "tool_calls",
        FinishReason::FunctionCall => "function_call",
    }
    .to_string()
}

fn generate_chatcmpl_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("chatcmpl-{ts:x}{seq:x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use summer_ai_core::{AdapterKind, ChatMessage, StreamEnd};

    fn ctx() -> IngressCtx {
        IngressCtx::new(AdapterKind::OpenAI, "gpt-4o-mini", "gpt-4o-mini")
    }

    #[test]
    fn identity_to_canonical_returns_input() {
        let req = ChatRequest::new("gpt-4o-mini", vec![ChatMessage::user("hi")]);
        let model = req.model.clone();
        let out = OpenAIIngress::to_canonical(req, &ctx()).unwrap();
        assert_eq!(out.model, model);
        assert_eq!(out.messages.len(), 1);
    }

    #[test]
    fn start_emits_role_chunk_and_primes_state() {
        let mut state = StreamConvertState::for_format(IngressFormat::OpenAI);
        let evt = ChatStreamEvent::Start {
            adapter: "openai".into(),
            model: "gpt-5.4".into(),
        };
        let out = OpenAIIngress::from_canonical_stream_event(evt, &mut state, &ctx()).unwrap();
        assert_eq!(out.len(), 1);
        let c = &out[0];
        assert_eq!(c.object, "chat.completion.chunk");
        assert_eq!(c.model, "gpt-5.4");
        assert_eq!(c.choices.len(), 1);
        assert_eq!(c.choices[0].delta.role.as_deref(), Some("assistant"));
        assert!(c.choices[0].finish_reason.is_none());
        assert!(c.id.starts_with("chatcmpl-"));
    }

    #[test]
    fn text_delta_emits_content_chunk() {
        let mut state = StreamConvertState::Openai(OpenAIStreamState {
            chunk_id: "chatcmpl-x".into(),
            created: 42,
            model: "m".into(),
            role_emitted: true,
        });
        let evt = ChatStreamEvent::TextDelta { text: "hi".into() };
        let out = OpenAIIngress::from_canonical_stream_event(evt, &mut state, &ctx()).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].choices[0].delta.content.as_deref(), Some("hi"));
        assert!(out[0].choices[0].delta.role.is_none());
    }

    #[test]
    fn text_delta_without_start_auto_primes_and_emits_role_first() {
        let mut state = StreamConvertState::for_format(IngressFormat::OpenAI);
        let evt = ChatStreamEvent::TextDelta { text: "x".into() };
        let out = OpenAIIngress::from_canonical_stream_event(evt, &mut state, &ctx()).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].choices[0].delta.role.as_deref(), Some("assistant"));
        assert_eq!(out[1].choices[0].delta.content.as_deref(), Some("x"));
    }

    #[test]
    fn end_emits_finish_chunk_and_usage_chunk_when_present() {
        let mut state = StreamConvertState::Openai(OpenAIStreamState {
            chunk_id: "chatcmpl-x".into(),
            created: 42,
            model: "m".into(),
            role_emitted: true,
        });
        let mut usage = Usage::default();
        usage.prompt_tokens = 10;
        usage.completion_tokens = 5;
        usage.total_tokens = 15;
        let evt = ChatStreamEvent::End(StreamEnd {
            finish_reason: Some(FinishReason::Stop),
            usage: Some(usage),
        });
        let out = OpenAIIngress::from_canonical_stream_event(evt, &mut state, &ctx()).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].choices[0].finish_reason.as_deref(), Some("stop"));
        assert!(out[0].usage.is_none());
        assert!(out[1].choices.is_empty());
        assert!(out[1].usage.is_some());
    }

    #[test]
    fn end_without_usage_emits_single_chunk() {
        let mut state = StreamConvertState::Openai(OpenAIStreamState {
            chunk_id: "chatcmpl-x".into(),
            created: 42,
            model: "m".into(),
            role_emitted: true,
        });
        let evt = ChatStreamEvent::End(StreamEnd {
            finish_reason: Some(FinishReason::ToolCalls),
            usage: None,
        });
        let out = OpenAIIngress::from_canonical_stream_event(evt, &mut state, &ctx()).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(
            out[0].choices[0].finish_reason.as_deref(),
            Some("tool_calls")
        );
    }

    #[test]
    fn reasoning_delta_is_dropped() {
        let mut state = StreamConvertState::Openai(OpenAIStreamState {
            chunk_id: "chatcmpl-x".into(),
            created: 42,
            model: "m".into(),
            role_emitted: true,
        });
        let evt = ChatStreamEvent::ReasoningDelta {
            text: "thinking".into(),
        };
        let out = OpenAIIngress::from_canonical_stream_event(evt, &mut state, &ctx()).unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn tool_call_delta_maps_to_function_tool_call() {
        let mut state = StreamConvertState::Openai(OpenAIStreamState {
            chunk_id: "chatcmpl-x".into(),
            created: 42,
            model: "m".into(),
            role_emitted: true,
        });
        let evt = ChatStreamEvent::ToolCallDelta(ToolCallDelta {
            index: 0,
            id: Some("call_1".into()),
            name: Some("lookup".into()),
            arguments_delta: Some(r#"{"q":"x"}"#.into()),
        });
        let out = OpenAIIngress::from_canonical_stream_event(evt, &mut state, &ctx()).unwrap();
        assert_eq!(out.len(), 1);
        let tc = out[0].choices[0]
            .delta
            .tool_calls
            .as_ref()
            .and_then(|v| v.first())
            .unwrap();
        assert_eq!(tc.index, 0);
        assert_eq!(tc.id.as_deref(), Some("call_1"));
        assert_eq!(tc.kind, "function");
        assert_eq!(tc.function.name.as_deref(), Some("lookup"));
        assert_eq!(tc.function.arguments.as_deref(), Some(r#"{"q":"x"}"#));
    }
}
