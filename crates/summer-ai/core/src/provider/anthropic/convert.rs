use crate::convert::{joined_text_value, serialize_arguments};
use crate::types::chat::{ChatCompletionResponse, Choice};
use crate::types::common::{FinishReason, FunctionCall, Message, ToolCall, Usage};

use super::protocol::{AnthropicResponse, AnthropicUsage};

pub(super) fn convert_response(response: AnthropicResponse) -> ChatCompletionResponse {
    let mut texts = Vec::new();
    let mut tool_calls = Vec::new();

    for block in response.content {
        match block.kind.as_str() {
            "text" if !block.text.is_empty() => texts.push(block.text),
            "tool_use" => tool_calls.push(ToolCall {
                id: block.id,
                r#type: "function".into(),
                function: FunctionCall {
                    name: block.name,
                    arguments: serialize_arguments(block.input),
                },
            }),
            _ => {}
        }
    }

    let finish_reason =
        map_anthropic_finish_reason(response.stop_reason.as_deref(), !tool_calls.is_empty());

    ChatCompletionResponse {
        id: response.id,
        object: "chat.completion".into(),
        created: super::unix_timestamp(),
        model: response.model,
        choices: vec![Choice {
            index: 0,
            message: Message {
                role: "assistant".into(),
                content: joined_text_value(texts),
                name: None,
                tool_calls: (!tool_calls.is_empty()).then_some(tool_calls),
                tool_call_id: None,
            },
            finish_reason,
        }],
        usage: usage_from_anthropic(response.usage),
    }
}

pub(super) fn usage_from_anthropic(usage: AnthropicUsage) -> Usage {
    let total_tokens = usage.input_tokens + usage.output_tokens;
    Usage {
        prompt_tokens: usage.input_tokens,
        completion_tokens: usage.output_tokens,
        total_tokens,
        cached_tokens: usage.cache_read_input_tokens + usage.cache_creation_input_tokens,
        reasoning_tokens: 0,
    }
}

pub(super) fn merge_anthropic_usage(state: &mut Usage, usage: AnthropicUsage) {
    if usage.input_tokens > 0 || state.prompt_tokens == 0 {
        state.prompt_tokens = usage.input_tokens;
    }
    if usage.output_tokens > 0 || state.completion_tokens == 0 {
        state.completion_tokens = usage.output_tokens;
    }

    let cached_tokens = usage.cache_read_input_tokens + usage.cache_creation_input_tokens;
    if cached_tokens > 0 || state.cached_tokens == 0 {
        state.cached_tokens = cached_tokens;
    }

    state.total_tokens = state.prompt_tokens + state.completion_tokens;
}

pub(super) fn map_anthropic_finish_reason(
    finish_reason: Option<&str>,
    has_tool_calls: bool,
) -> Option<FinishReason> {
    if has_tool_calls {
        return Some(FinishReason::ToolCalls);
    }

    match finish_reason {
        Some("max_tokens") => Some(FinishReason::Length),
        Some("tool_use") => Some(FinishReason::ToolCalls),
        Some("content_filter" | "refusal") => Some(FinishReason::ContentFilter),
        Some("end_turn" | "stop_sequence") => Some(FinishReason::Stop),
        Some(_) | None => Some(FinishReason::Stop),
    }
}

pub(super) fn map_anthropic_stream_finish_reason(
    finish_reason: Option<&str>,
    has_tool_calls: bool,
) -> Option<FinishReason> {
    finish_reason.and_then(|reason| map_anthropic_finish_reason(Some(reason), has_tool_calls))
}
