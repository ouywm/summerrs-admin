use std::collections::HashMap;

use anyhow::Result;

use crate::provider::{ProviderErrorInfo, ProviderErrorKind, ProviderStreamError};
use crate::stream::{ChatStreamItem, SseEvent, StreamEventMapper};
use crate::types::chat::{ChatCompletionChunk, ChunkChoice};
use crate::types::common::{Delta, FinishReason, FunctionCallDelta, ToolCallDelta, Usage};

use super::convert::{map_anthropic_stream_finish_reason, merge_anthropic_usage};
use super::protocol::{
    AnthropicStreamEnvelope, AnthropicStreamError, AnthropicStreamMessage, anthropic_error_kind,
};

#[derive(Debug)]
pub(super) struct AnthropicStreamState {
    id: String,
    model: String,
    created: i64,
    usage: Usage,
    role_emitted: bool,
    next_tool_call_index: i32,
    block_tool_call_index: HashMap<u64, i32>,
    stopped: bool,
}

impl Default for AnthropicStreamState {
    fn default() -> Self {
        Self {
            id: String::new(),
            model: String::new(),
            created: super::unix_timestamp(),
            usage: Usage::default(),
            role_emitted: false,
            next_tool_call_index: 0,
            block_tool_call_index: HashMap::new(),
            stopped: false,
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub(super) struct AnthropicStreamMapper;

impl StreamEventMapper for AnthropicStreamMapper {
    type State = AnthropicStreamState;

    fn map_event(&self, state: &mut Self::State, event: SseEvent) -> Vec<Result<ChatStreamItem>> {
        if event.data.is_empty() || event.data == "[DONE]" {
            return Vec::new();
        }

        let envelope: AnthropicStreamEnvelope = match serde_json::from_str(&event.data) {
            Ok(envelope) => envelope,
            Err(error) => {
                tracing::warn!(
                    "failed to parse anthropic SSE event: {error}, data: {}",
                    event.data
                );
                return Vec::new();
            }
        };

        let kind = if envelope.kind.is_empty() {
            event.event.as_deref().unwrap_or_default()
        } else {
            envelope.kind.as_str()
        };

        match kind {
            "message_start" => map_message_start(state, envelope.message),
            "content_block_start" => map_content_block_start(state, envelope),
            "content_block_delta" => map_content_block_delta(state, envelope),
            "message_delta" => map_message_delta(state, envelope),
            "error" => map_error_event(state, envelope.error),
            "message_stop" => {
                state.stopped = true;
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn should_stop(&self, state: &Self::State) -> bool {
        state.stopped
    }
}

fn map_message_start(
    state: &mut AnthropicStreamState,
    message: Option<AnthropicStreamMessage>,
) -> Vec<Result<ChatStreamItem>> {
    let Some(message) = message else {
        return Vec::new();
    };

    state.id = message.id;
    state.model = message.model;
    merge_anthropic_usage(&mut state.usage, message.usage);

    if state.role_emitted {
        return Vec::new();
    }
    state.role_emitted = true;

    vec![Ok(ChatStreamItem::chunk(chunk_with_delta(
        state,
        Delta {
            role: Some("assistant".into()),
            content: None,
            reasoning_content: None,
            tool_calls: None,
        },
        None,
        None,
    )))]
}

fn map_content_block_start(
    state: &mut AnthropicStreamState,
    envelope: AnthropicStreamEnvelope,
) -> Vec<Result<ChatStreamItem>> {
    let Some(block) = envelope.content_block else {
        return Vec::new();
    };
    if block.kind != "tool_use" {
        return Vec::new();
    }

    let index = state.next_tool_call_index;
    state.next_tool_call_index += 1;
    if let Some(block_index) = envelope.index {
        state.block_tool_call_index.insert(block_index, index);
    }

    vec![Ok(ChatStreamItem::chunk(chunk_with_delta(
        state,
        Delta {
            role: None,
            content: None,
            reasoning_content: None,
            tool_calls: Some(vec![ToolCallDelta {
                index,
                id: Some(block.id),
                r#type: Some("function".into()),
                function: Some(FunctionCallDelta {
                    name: Some(block.name),
                    arguments: Some(String::new()),
                }),
            }]),
        },
        None,
        None,
    )))]
}

fn map_content_block_delta(
    state: &mut AnthropicStreamState,
    envelope: AnthropicStreamEnvelope,
) -> Vec<Result<ChatStreamItem>> {
    let Some(delta) = envelope.delta else {
        return Vec::new();
    };

    match delta.kind.as_str() {
        "text_delta" if !delta.text.is_empty() => {
            vec![Ok(ChatStreamItem::chunk(chunk_with_delta(
                state,
                Delta {
                    role: None,
                    content: Some(delta.text),
                    reasoning_content: None,
                    tool_calls: None,
                },
                None,
                None,
            )))]
        }
        "thinking_delta" if !delta.thinking.is_empty() => {
            vec![Ok(ChatStreamItem::chunk(chunk_with_delta(
                state,
                Delta {
                    role: None,
                    content: None,
                    reasoning_content: Some(delta.thinking),
                    tool_calls: None,
                },
                None,
                None,
            )))]
        }
        "input_json_delta" if !delta.partial_json.is_empty() => envelope
            .index
            .and_then(|block_index| state.block_tool_call_index.get(&block_index).copied())
            .map(|tool_index| {
                Ok(ChatStreamItem::chunk(chunk_with_delta(
                    state,
                    Delta {
                        role: None,
                        content: None,
                        reasoning_content: None,
                        tool_calls: Some(vec![ToolCallDelta {
                            index: tool_index,
                            id: None,
                            r#type: None,
                            function: Some(FunctionCallDelta {
                                name: None,
                                arguments: Some(delta.partial_json),
                            }),
                        }]),
                    },
                    None,
                    None,
                )))
            })
            .into_iter()
            .collect(),
        _ => Vec::new(),
    }
}

fn map_message_delta(
    state: &mut AnthropicStreamState,
    envelope: AnthropicStreamEnvelope,
) -> Vec<Result<ChatStreamItem>> {
    let has_terminal_usage = envelope.usage.is_some();
    if let Some(usage) = envelope.usage {
        merge_anthropic_usage(&mut state.usage, usage);
    }

    let finish_reason = envelope
        .delta
        .and_then(|delta| map_anthropic_stream_finish_reason(delta.stop_reason.as_deref(), false));

    let chunk = chunk_with_delta(
        state,
        Delta {
            role: None,
            content: None,
            reasoning_content: None,
            tool_calls: None,
        },
        finish_reason.clone(),
        Some(state.usage.clone()),
    );
    if finish_reason.is_some() || has_terminal_usage {
        vec![Ok(ChatStreamItem::terminal_chunk(chunk))]
    } else {
        vec![Ok(ChatStreamItem::chunk(chunk))]
    }
}

fn map_error_event(
    state: &mut AnthropicStreamState,
    error: Option<AnthropicStreamError>,
) -> Vec<Result<ChatStreamItem>> {
    let Some(error) = error else {
        return Vec::new();
    };

    let kind = if error.kind.is_empty() {
        "unknown_error"
    } else {
        error.kind.as_str()
    };
    let message = if error.message.is_empty() {
        "anthropic stream returned an error event"
    } else {
        error.message.as_str()
    };
    let info = ProviderErrorInfo::new(
        anthropic_error_kind(kind).unwrap_or(ProviderErrorKind::Api),
        message,
        kind,
    );
    state.stopped = true;

    vec![Err(anyhow::Error::new(ProviderStreamError::new(info))
        .context(format!("anthropic stream error [{kind}]")))]
}

fn chunk_with_delta(
    state: &AnthropicStreamState,
    delta: Delta,
    finish_reason: Option<FinishReason>,
    usage: Option<Usage>,
) -> ChatCompletionChunk {
    ChatCompletionChunk {
        id: if state.id.is_empty() {
            format!("anthropic-{}", state.created)
        } else {
            state.id.clone()
        },
        object: "chat.completion.chunk".into(),
        created: state.created,
        model: state.model.clone(),
        choices: vec![ChunkChoice {
            index: 0,
            delta,
            finish_reason,
        }],
        usage,
    }
}
