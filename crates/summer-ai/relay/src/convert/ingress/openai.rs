//! OpenAI identity converter。
//!
//! canonical 就是 OpenAI-flat 格式（`core/src/types/openai/chat.rs` 的 `ChatRequest`
//! 与官方完全对齐），所以 OpenAI 入口的转换是 **identity**——三个方法都原样透传。
//!
//! 作为 [`IngressConverter`] trait 的**模板实现**，Anthropic / Gemini converter
//! 按同样形状实现即可。

use super::{IngressConverter, IngressCtx, IngressFormat, StreamConvertState};
use summer_ai_core::{AdapterResult, ChatRequest, ChatResponse, ChatStreamEvent};

/// OpenAI 入口协议的 identity converter。
pub struct OpenAIIngress;

impl IngressConverter for OpenAIIngress {
    type ClientRequest = ChatRequest;
    type ClientResponse = ChatResponse;
    type ClientStreamEvent = ChatStreamEvent;

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
        _state: &mut StreamConvertState,
        _ctx: &IngressCtx,
    ) -> AdapterResult<Vec<Self::ClientStreamEvent>> {
        Ok(vec![event])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use summer_ai_core::{AdapterKind, ChatMessage};

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
    fn identity_stream_passthrough() {
        let mut state = StreamConvertState::for_format(IngressFormat::OpenAI);
        let evt = ChatStreamEvent::TextDelta {
            text: "hello".to_string(),
        };
        let out = OpenAIIngress::from_canonical_stream_event(evt, &mut state, &ctx()).unwrap();
        assert_eq!(out.len(), 1);
        match &out[0] {
            ChatStreamEvent::TextDelta { text } => assert_eq!(text, "hello"),
            _ => panic!("expected TextDelta"),
        }
    }
}
