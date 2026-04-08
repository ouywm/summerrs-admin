use std::collections::BTreeMap;

use crate::convert::joined_text_value;
use crate::types::chat::{ChatCompletionChunk, ChatCompletionResponse, Choice};
use crate::types::common::{FinishReason, FunctionCall, Message, ToolCall, Usage};

#[derive(Debug, Default, Clone)]
struct AggregatedToolCall {
    id: Option<String>,
    kind: Option<String>,
    name: Option<String>,
    arguments: String,
}

#[derive(Debug, Default, Clone)]
pub struct ChunkAggregator {
    id: Option<String>,
    object: Option<String>,
    created: Option<i64>,
    model: Option<String>,
    role: Option<String>,
    content: String,
    reasoning_content: String,
    finish_reason: Option<FinishReason>,
    usage: Option<Usage>,
    tool_calls: BTreeMap<i32, AggregatedToolCall>,
}

impl ChunkAggregator {
    pub fn push(&mut self, chunk: &ChatCompletionChunk) {
        if self.id.is_none() {
            self.id = Some(chunk.id.clone());
        }
        if self.object.is_none() {
            self.object = Some(chunk.object.clone());
        }
        if self.created.is_none() {
            self.created = Some(chunk.created);
        }
        if self.model.is_none() {
            self.model = Some(chunk.model.clone());
        }

        if let Some(usage) = chunk.usage.clone() {
            self.usage = Some(usage);
        }

        for choice in &chunk.choices {
            if let Some(role) = choice.delta.role.clone() {
                self.role = Some(role);
            }
            if let Some(content) = choice.delta.content.as_deref() {
                self.content.push_str(content);
            }
            if let Some(reasoning) = choice.delta.reasoning_content.as_deref() {
                self.reasoning_content.push_str(reasoning);
            }
            if let Some(tool_calls) = choice.delta.tool_calls.as_ref() {
                for tool_call in tool_calls {
                    let entry = self.tool_calls.entry(tool_call.index).or_default();
                    if let Some(id) = tool_call.id.clone() {
                        entry.id = Some(id);
                    }
                    if let Some(kind) = tool_call.r#type.clone() {
                        entry.kind = Some(kind);
                    }
                    if let Some(function) = tool_call.function.as_ref() {
                        if let Some(name) = function.name.clone() {
                            entry.name = Some(name);
                        }
                        if let Some(arguments) = function.arguments.as_deref() {
                            entry.arguments.push_str(arguments);
                        }
                    }
                }
            }
            if let Some(finish_reason) = choice.finish_reason.clone() {
                self.finish_reason = Some(finish_reason);
            }
        }
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn reasoning_content(&self) -> &str {
        &self.reasoning_content
    }

    pub fn finish_reason(&self) -> Option<&FinishReason> {
        self.finish_reason.as_ref()
    }

    pub fn usage(&self) -> Option<&Usage> {
        self.usage.as_ref()
    }

    pub fn into_response(self) -> Option<ChatCompletionResponse> {
        let id = self.id?;
        let created = self.created?;
        let model = self.model?;
        let object = self
            .object
            .unwrap_or_else(|| "chat.completion.chunk".into());

        let tool_calls = (!self.tool_calls.is_empty()).then(|| {
            self.tool_calls
                .into_iter()
                .map(|(_, tool_call)| ToolCall {
                    id: tool_call.id.unwrap_or_else(|| "call_0".into()),
                    r#type: tool_call.kind.unwrap_or_else(|| "function".into()),
                    function: FunctionCall {
                        name: tool_call.name.unwrap_or_else(|| "tool".into()),
                        arguments: tool_call.arguments,
                    },
                })
                .collect::<Vec<_>>()
        });

        Some(ChatCompletionResponse {
            id,
            object: if object == "chat.completion.chunk" {
                "chat.completion".into()
            } else {
                object
            },
            created,
            model,
            choices: vec![Choice {
                index: 0,
                message: Message {
                    role: self.role.unwrap_or_else(|| "assistant".into()),
                    content: joined_text_value(
                        (!self.content.is_empty())
                            .then_some(self.content)
                            .into_iter()
                            .collect(),
                    ),
                    name: None,
                    tool_calls,
                    tool_call_id: None,
                },
                finish_reason: self.finish_reason,
            }],
            usage: self.usage.unwrap_or_default(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::chat::ChunkChoice;
    use crate::types::common::{Delta, FunctionCallDelta, ToolCallDelta};

    #[test]
    fn aggregator_merges_content_reasoning_tool_calls_and_usage() {
        let mut aggregator = ChunkAggregator::default();
        aggregator.push(&ChatCompletionChunk {
            id: "chatcmpl-1".into(),
            object: "chat.completion.chunk".into(),
            created: 1,
            model: "gpt-4o".into(),
            choices: vec![ChunkChoice {
                index: 0,
                delta: Delta {
                    role: Some("assistant".into()),
                    content: Some("Hel".into()),
                    reasoning_content: Some("think-1".into()),
                    tool_calls: Some(vec![ToolCallDelta {
                        index: 0,
                        id: Some("call_1".into()),
                        r#type: Some("function".into()),
                        function: Some(FunctionCallDelta {
                            name: Some("get_weather".into()),
                            arguments: Some("{".into()),
                        }),
                    }]),
                },
                finish_reason: None,
            }],
            usage: None,
        });
        aggregator.push(&ChatCompletionChunk {
            id: "chatcmpl-1".into(),
            object: "chat.completion.chunk".into(),
            created: 1,
            model: "gpt-4o".into(),
            choices: vec![ChunkChoice {
                index: 0,
                delta: Delta {
                    role: None,
                    content: Some("lo".into()),
                    reasoning_content: Some("think-2".into()),
                    tool_calls: Some(vec![ToolCallDelta {
                        index: 0,
                        id: None,
                        r#type: None,
                        function: Some(FunctionCallDelta {
                            name: None,
                            arguments: Some("\"city\":\"Paris\"}".into()),
                        }),
                    }]),
                },
                finish_reason: Some(FinishReason::ToolCalls),
            }],
            usage: Some(Usage {
                prompt_tokens: 10,
                completion_tokens: 5,
                total_tokens: 15,
                cached_tokens: 0,
                reasoning_tokens: 0,
            }),
        });

        assert_eq!(aggregator.content(), "Hello");
        assert_eq!(aggregator.reasoning_content(), "think-1think-2");
        assert!(matches!(
            aggregator.finish_reason(),
            Some(FinishReason::ToolCalls)
        ));
        assert_eq!(aggregator.usage().unwrap().total_tokens, 15);

        let response = aggregator.into_response().unwrap();
        assert_eq!(
            response.choices[0].message.content,
            serde_json::json!("Hello")
        );
        assert_eq!(
            response.choices[0].message.tool_calls.as_ref().unwrap()[0]
                .function
                .arguments,
            r#"{"city":"Paris"}"#
        );
    }
}
