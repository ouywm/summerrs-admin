use anyhow::{Context, Result};

use crate::convert::message::merge_extra_body_fields;
use crate::convert::{
    NormalizedContentPart, join_message_text_by_roles, normalize_openai_content_parts,
    parse_function_arguments, stop_sequences_from_option,
};
use crate::types::chat::ChatCompletionRequest;
use crate::types::common::{Message, Tool};

use super::protocol::{AnthropicMessage, AnthropicRequest, AnthropicTool};

pub(super) fn build_anthropic_body(
    req: &ChatCompletionRequest,
    actual_model: &str,
) -> Result<serde_json::Value> {
    let omit_tools_for_none = tool_choice_is_none(req.tool_choice.as_ref());
    let mut body = serde_json::to_value(AnthropicRequest {
        model: actual_model.to_string(),
        messages: convert_messages(&req.messages),
        system: collect_system_prompt(&req.messages),
        max_tokens: req.max_tokens.unwrap_or(4096),
        temperature: req.temperature,
        top_p: req.top_p,
        stop_sequences: stop_sequences_from_option(req.stop.as_ref()),
        tools: (!omit_tools_for_none)
            .then(|| convert_tools(req.tools.as_ref()))
            .flatten(),
        tool_choice: (!omit_tools_for_none)
            .then(|| convert_tool_choice(req.tool_choice.as_ref()))
            .flatten(),
        stream: req.stream,
    })
    .context("failed to serialize anthropic request")?;
    merge_extra_body_fields(&mut body, &req.extra);
    Ok(body)
}

pub(super) fn build_anthropic_url(base_url: &str) -> String {
    let base = base_url.trim_end_matches('/');
    if base.ends_with("/v1") {
        format!("{base}/messages")
    } else {
        format!("{base}/v1/messages")
    }
}

fn collect_system_prompt(messages: &[Message]) -> Option<String> {
    join_message_text_by_roles(messages, &["system", "developer"])
}

fn convert_messages(messages: &[Message]) -> Vec<AnthropicMessage> {
    let mut converted = Vec::new();

    for message in messages {
        match message.role.as_str() {
            "system" | "developer" => {}
            "tool" => {
                let mut content = Vec::new();
                let tool_result = convert_tool_result_content(&message.content);
                if let Some(tool_use_id) = message.tool_call_id.clone()
                    && let Some(tool_result) = tool_result
                {
                    content.push(serde_json::json!({
                        "type": "tool_result",
                        "tool_use_id": tool_use_id,
                        "content": tool_result,
                    }));
                } else if let Some(tool_result) = tool_result {
                    let text = match tool_result {
                        serde_json::Value::String(text) => text,
                        other => other.to_string(),
                    };
                    content.push(serde_json::json!({
                        "type": "text",
                        "text": text,
                    }));
                }

                if !content.is_empty() {
                    converted.push(AnthropicMessage {
                        role: "user".into(),
                        content,
                    });
                }
            }
            "assistant" => {
                let mut content = content_blocks_from_value(&message.content);
                if let Some(tool_calls) = message.tool_calls.as_ref() {
                    content.extend(tool_calls.iter().map(|tool_call| {
                        serde_json::json!({
                            "type": "tool_use",
                            "id": tool_call.id,
                            "name": tool_call.function.name,
                            "input": parse_function_arguments(&tool_call.function.arguments),
                        })
                    }));
                }
                if !content.is_empty() {
                    converted.push(AnthropicMessage {
                        role: "assistant".into(),
                        content,
                    });
                }
            }
            role => {
                let content = content_blocks_from_value(&message.content);
                if !content.is_empty() {
                    converted.push(AnthropicMessage {
                        role: role.to_string(),
                        content,
                    });
                }
            }
        }
    }

    converted
}

fn convert_tools(tools: Option<&Vec<Tool>>) -> Option<Vec<AnthropicTool>> {
    tools.map(|items| {
        items
            .iter()
            .map(|tool| AnthropicTool {
                name: tool.function.name.clone(),
                description: tool.function.description.clone(),
                input_schema: tool
                    .function
                    .parameters
                    .clone()
                    .unwrap_or_else(|| serde_json::json!({"type": "object"})),
            })
            .collect()
    })
}

fn convert_tool_choice(tool_choice: Option<&serde_json::Value>) -> Option<serde_json::Value> {
    let choice = tool_choice?;

    if let Some(choice) = choice.as_str() {
        return match choice {
            "auto" => Some(serde_json::json!({"type": "auto"})),
            "required" => Some(serde_json::json!({"type": "any"})),
            "none" => Some(serde_json::json!({"type": "none"})),
            _ => None,
        };
    }

    choice
        .get("function")
        .and_then(|function| function.get("name"))
        .and_then(|name| name.as_str())
        .map(|name| serde_json::json!({"type": "tool", "name": name}))
}

fn tool_choice_is_none(tool_choice: Option<&serde_json::Value>) -> bool {
    tool_choice.and_then(serde_json::Value::as_str) == Some("none")
}

fn content_blocks_from_value(content: &serde_json::Value) -> Vec<serde_json::Value> {
    normalize_openai_content_parts(content)
        .into_iter()
        .map(anthropic_content_block_from_normalized_part)
        .collect()
}

fn anthropic_content_block_from_normalized_part(part: NormalizedContentPart) -> serde_json::Value {
    match part {
        NormalizedContentPart::Text(text) => serde_json::json!({
            "type": "text",
            "text": text,
        }),
        NormalizedContentPart::ImageData { mime_type, data } => serde_json::json!({
            "type": "image",
            "source": {
                "type": "base64",
                "media_type": mime_type,
                "data": data,
            },
        }),
        NormalizedContentPart::ImageUrl { url, .. } => serde_json::json!({
            "type": "image",
            "source": {
                "type": "url",
                "url": url,
            },
        }),
    }
}

fn convert_tool_result_content(content: &serde_json::Value) -> Option<serde_json::Value> {
    match content {
        serde_json::Value::String(text) if !text.is_empty() => {
            Some(serde_json::Value::String(text.clone()))
        }
        serde_json::Value::Array(items) if !items.is_empty() => {
            Some(serde_json::Value::Array(items.clone()))
        }
        serde_json::Value::Object(map) if map.get("type").is_some() => Some(
            serde_json::Value::Array(vec![serde_json::Value::Object(map.clone())]),
        ),
        serde_json::Value::Null => None,
        other => Some(serde_json::Value::String(other.to_string())),
    }
}
