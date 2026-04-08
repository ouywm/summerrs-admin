use std::collections::{HashMap, HashSet};

use anyhow::Result;

use crate::convert::serialize_arguments;
use crate::provider::{ProviderErrorInfo, ProviderErrorKind, ProviderStreamError};
use crate::stream::{ChatStreamItem, SseEvent, StreamEventMapper};
use crate::types::chat::{ChatCompletionChunk, ChunkChoice};
use crate::types::common::{Delta, FunctionCallDelta, ToolCallDelta, Usage};

use super::convert::{map_gemini_stream_finish_reason, usage_from_gemini};
use super::protocol::GeminiResponse;

#[derive(Debug, Default)]
pub(super) struct GeminiStreamState {
    id: String,
    model: String,
    created: i64,
    role_emitted: HashSet<i32>,
    next_tool_call_index: HashMap<i32, i32>,
    saw_tool_call: HashSet<i32>,
    active_tool_call_parts: HashMap<(i32, usize), i32>,
    stopped: bool,
}

#[derive(Debug, Clone)]
pub(super) struct GeminiStreamMapper {
    model: String,
}

impl GeminiStreamMapper {
    pub(super) fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
        }
    }
}

impl StreamEventMapper for GeminiStreamMapper {
    type State = GeminiStreamState;

    fn map_event(&self, state: &mut Self::State, event: SseEvent) -> Vec<Result<ChatStreamItem>> {
        initialize_state(state, &self.model);

        if event.data.is_empty() || event.data == "[DONE]" {
            return Vec::new();
        }

        let payload: serde_json::Value = match serde_json::from_str(&event.data) {
            Ok(payload) => payload,
            Err(error) => {
                tracing::warn!(
                    "failed to parse gemini SSE event: {error}, data: {}",
                    event.data
                );
                return Vec::new();
            }
        };

        if let Some(info) = parse_gemini_stream_error(&payload) {
            let code = info.code.clone();
            state.stopped = true;
            return vec![Err(anyhow::Error::new(ProviderStreamError::new(info))
                .context(format!("gemini stream error [{code}]")))];
        }

        let response: GeminiResponse = match serde_json::from_value(payload) {
            Ok(response) => response,
            Err(error) => {
                tracing::warn!(
                    "failed to parse gemini SSE event: {error}, data: {}",
                    event.data
                );
                return Vec::new();
            }
        };

        map_response_chunks(state, response)
    }

    fn should_stop(&self, state: &Self::State) -> bool {
        state.stopped
    }
}

pub(super) fn gemini_error_kind(error_code: &str) -> Option<ProviderErrorKind> {
    match error_code {
        "INVALID_ARGUMENT" | "NOT_FOUND" => Some(ProviderErrorKind::InvalidRequest),
        "FAILED_PRECONDITION" => Some(ProviderErrorKind::Api),
        "UNAUTHENTICATED" | "PERMISSION_DENIED" => Some(ProviderErrorKind::Authentication),
        "RESOURCE_EXHAUSTED" => Some(ProviderErrorKind::RateLimit),
        "INTERNAL" | "UNAVAILABLE" | "DEADLINE_EXCEEDED" => Some(ProviderErrorKind::Server),
        _ => None,
    }
}

pub(super) fn parse_gemini_stream_error(payload: &serde_json::Value) -> Option<ProviderErrorInfo> {
    let error_obj = payload.get("error")?;
    let code = error_obj
        .get("status")
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("UNKNOWN");
    let message = error_obj
        .get("message")
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("gemini stream returned an error event");
    let kind = gemini_error_kind(code).unwrap_or(ProviderErrorKind::Api);

    Some(ProviderErrorInfo::new(kind, message, code))
}

fn initialize_state(state: &mut GeminiStreamState, model: &str) {
    if state.created == 0 {
        state.created = super::unix_timestamp();
    }
    if state.id.is_empty() {
        state.id = format!("gemini-{}", state.created);
    }
    if state.model.is_empty() {
        state.model = model.to_string();
    }
}

fn map_response_chunks(
    state: &mut GeminiStreamState,
    response: GeminiResponse,
) -> Vec<Result<ChatStreamItem>> {
    let usage = response.usage_metadata.clone().map(usage_from_gemini);
    let total_candidates = response.candidates.len();
    let mut emitted_terminal_chunk = false;
    let mut chunks = Vec::new();

    for (choice_index, candidate) in response.candidates.into_iter().enumerate() {
        let choice_index = choice_index as i32;
        let mut saw_tool_call = false;

        if let Some(content) = candidate.content {
            if state.role_emitted.insert(choice_index) {
                chunks.push(Ok(ChatStreamItem::chunk(chunk_with_delta(
                    state,
                    choice_index,
                    Delta {
                        role: Some("assistant".into()),
                        content: None,
                        reasoning_content: None,
                        tool_calls: None,
                    },
                    None,
                    None,
                ))));
            }

            for (part_index, part) in content.parts.into_iter().enumerate() {
                if let Some(text) = part.text
                    && !text.is_empty()
                {
                    chunks.push(Ok(ChatStreamItem::chunk(chunk_with_delta(
                        state,
                        choice_index,
                        Delta {
                            role: None,
                            content: Some(text),
                            reasoning_content: None,
                            tool_calls: None,
                        },
                        None,
                        None,
                    ))));
                }

                if let Some(function_call) = part.function_call {
                    saw_tool_call = true;
                    state.saw_tool_call.insert(choice_index);
                    let tool_index = if let Some(tool_index) = state
                        .active_tool_call_parts
                        .get(&(choice_index, part_index))
                        .copied()
                    {
                        tool_index
                    } else {
                        let tool_index = state
                            .next_tool_call_index
                            .get(&choice_index)
                            .copied()
                            .unwrap_or(0);
                        state
                            .next_tool_call_index
                            .insert(choice_index, tool_index + 1);
                        state
                            .active_tool_call_parts
                            .insert((choice_index, part_index), tool_index);
                        tool_index
                    };
                    chunks.push(Ok(ChatStreamItem::chunk(chunk_with_delta(
                        state,
                        choice_index,
                        Delta {
                            role: None,
                            content: None,
                            reasoning_content: None,
                            tool_calls: Some(vec![ToolCallDelta {
                                index: tool_index,
                                id: Some(format!("call_{tool_index}")),
                                r#type: Some("function".into()),
                                function: Some(FunctionCallDelta {
                                    name: Some(function_call.name),
                                    arguments: Some(serialize_arguments(function_call.args)),
                                }),
                            }]),
                        },
                        None,
                        None,
                    ))));
                }
            }
        }

        let finish_reason = map_gemini_stream_finish_reason(
            candidate.finish_reason.as_deref(),
            state.saw_tool_call.contains(&choice_index) || saw_tool_call,
        );
        if finish_reason.is_some() || usage.is_some() {
            emitted_terminal_chunk = true;
            let choice_usage = if usage.is_some() && choice_index as usize + 1 == total_candidates {
                usage.clone()
            } else {
                None
            };
            chunks.push(Ok(ChatStreamItem::terminal_chunk(chunk_with_delta(
                state,
                choice_index,
                Delta {
                    role: None,
                    content: None,
                    reasoning_content: None,
                    tool_calls: None,
                },
                finish_reason,
                choice_usage,
            ))));
        }
    }

    if !emitted_terminal_chunk && let Some(usage) = usage {
        let choice_index = 0;
        if state.role_emitted.is_empty() {
            state.role_emitted.insert(choice_index);
        }
        chunks.push(Ok(ChatStreamItem::terminal_chunk(chunk_with_delta(
            state,
            choice_index,
            Delta {
                role: None,
                content: None,
                reasoning_content: None,
                tool_calls: None,
            },
            None,
            Some(usage),
        ))));
    }

    chunks
}

fn chunk_with_delta(
    state: &GeminiStreamState,
    choice_index: i32,
    delta: Delta,
    finish_reason: Option<crate::types::common::FinishReason>,
    usage: Option<Usage>,
) -> ChatCompletionChunk {
    ChatCompletionChunk {
        id: state.id.clone(),
        object: "chat.completion.chunk".into(),
        created: state.created,
        model: state.model.clone(),
        choices: vec![ChunkChoice {
            index: choice_index,
            delta,
            finish_reason,
        }],
        usage,
    }
}
