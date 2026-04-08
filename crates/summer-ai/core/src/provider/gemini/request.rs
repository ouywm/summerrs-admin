use std::collections::HashMap;

use anyhow::{Context, Result};

use crate::convert::message::merge_extra_body_fields;
use crate::convert::{
    NormalizedContentPart, join_message_text_by_roles, normalize_openai_content_parts,
    parse_function_arguments, stop_sequences_from_value,
};
use crate::types::chat::ChatCompletionRequest;
use crate::types::common::{Message, Tool};

use super::embedding::gemini_version_base;
use super::protocol::{
    GeminiFileData, GeminiFunctionCall, GeminiFunctionCallingConfig, GeminiFunctionDeclaration,
    GeminiFunctionResponse, GeminiGenerationConfig, GeminiInlineData, GeminiRequest,
    GeminiRequestContent, GeminiRequestPart, GeminiSystemInstruction, GeminiTextPart, GeminiTool,
    GeminiToolConfig,
};

pub(super) fn build_gemini_chat_body(req: &ChatCompletionRequest) -> Result<serde_json::Value> {
    let mut body = serde_json::to_value(GeminiRequest {
        contents: convert_contents(&req.messages),
        generation_config: build_generation_config(req),
        tools: convert_tools(req.tools.as_ref()),
        tool_config: convert_tool_config(req.tool_choice.as_ref()),
        system_instruction: collect_system_instruction(&req.messages),
    })
    .context("failed to serialize gemini request")?;
    merge_extra_body_fields(&mut body, &req.extra);
    Ok(body)
}

pub(super) fn build_gemini_url(base_url: &str, model: &str, stream: bool) -> String {
    let action = if stream {
        "streamGenerateContent"
    } else {
        "generateContent"
    };

    format!("{}/models/{model}:{action}", gemini_version_base(base_url))
}

pub(super) fn convert_contents(messages: &[Message]) -> Vec<GeminiRequestContent> {
    let mut contents = Vec::new();
    let mut tool_call_names = HashMap::new();

    for message in messages {
        match message.role.as_str() {
            "system" | "developer" => {}
            "assistant" => {
                let mut parts = parts_from_content(&message.content);
                if let Some(tool_calls) = message.tool_calls.as_ref() {
                    parts.extend(tool_calls.iter().map(|tool_call| {
                        tool_call_names
                            .insert(tool_call.id.clone(), tool_call.function.name.clone());
                        GeminiRequestPart {
                            text: None,
                            inline_data: None,
                            file_data: None,
                            function_call: Some(GeminiFunctionCall {
                                name: tool_call.function.name.clone(),
                                args: parse_function_arguments(&tool_call.function.arguments),
                            }),
                            function_response: None,
                        }
                    }));
                }
                if !parts.is_empty() {
                    contents.push(GeminiRequestContent {
                        role: "model".into(),
                        parts,
                    });
                }
            }
            "tool" => {
                if let Some(response) = convert_function_response_payload(&message.content) {
                    contents.push(GeminiRequestContent {
                        role: "user".into(),
                        parts: vec![GeminiRequestPart {
                            text: None,
                            inline_data: None,
                            file_data: None,
                            function_call: None,
                            function_response: Some(GeminiFunctionResponse {
                                name: message
                                    .tool_call_id
                                    .as_ref()
                                    .and_then(|tool_call_id| tool_call_names.get(tool_call_id))
                                    .cloned()
                                    .or_else(|| message.tool_call_id.clone())
                                    .unwrap_or_else(|| "tool_result".into()),
                                response,
                            }),
                        }],
                    });
                }
            }
            _ => {
                let parts = parts_from_content(&message.content);
                if !parts.is_empty() {
                    contents.push(GeminiRequestContent {
                        role: "user".into(),
                        parts,
                    });
                }
            }
        }
    }

    contents
}

fn build_generation_config(req: &ChatCompletionRequest) -> Option<GeminiGenerationConfig> {
    let response_format_type = req
        .response_format
        .as_ref()
        .and_then(|value| value.get("type"))
        .and_then(|value| value.as_str());
    let response_mime_type = match response_format_type {
        Some("json_object" | "json_schema") => Some("application/json".to_string()),
        _ => None,
    };
    let response_json_schema = match response_format_type {
        Some("json_schema") => req
            .response_format
            .as_ref()
            .and_then(extract_response_json_schema),
        _ => None,
    };

    let stop_sequences = req.stop.as_ref().and_then(stop_sequences_from_value);

    if req.temperature.is_none()
        && req.top_p.is_none()
        && req.max_tokens.is_none()
        && stop_sequences.is_none()
        && response_mime_type.is_none()
        && response_json_schema.is_none()
    {
        return None;
    }

    Some(GeminiGenerationConfig {
        temperature: req.temperature,
        top_p: req.top_p,
        max_output_tokens: req.max_tokens,
        stop_sequences,
        response_mime_type,
        response_json_schema,
    })
}

fn extract_response_json_schema(response_format: &serde_json::Value) -> Option<serde_json::Value> {
    response_format
        .get("json_schema")
        .and_then(|value| value.get("schema"))
        .cloned()
        .or_else(|| response_format.get("schema").cloned())
}

fn collect_system_instruction(messages: &[Message]) -> Option<GeminiSystemInstruction> {
    join_message_text_by_roles(messages, &["system", "developer"]).map(|text| {
        GeminiSystemInstruction {
            parts: vec![GeminiTextPart { text }],
        }
    })
}

fn parts_from_content(content: &serde_json::Value) -> Vec<GeminiRequestPart> {
    normalize_openai_content_parts(content)
        .into_iter()
        .map(gemini_part_from_normalized_content)
        .collect()
}

fn gemini_part_from_normalized_content(part: NormalizedContentPart) -> GeminiRequestPart {
    match part {
        NormalizedContentPart::Text(text) => GeminiRequestPart {
            text: Some(text),
            inline_data: None,
            file_data: None,
            function_call: None,
            function_response: None,
        },
        NormalizedContentPart::ImageData { mime_type, data } => GeminiRequestPart {
            text: None,
            inline_data: Some(GeminiInlineData { mime_type, data }),
            file_data: None,
            function_call: None,
            function_response: None,
        },
        NormalizedContentPart::ImageUrl {
            url,
            mime_type: Some(mime_type),
        } if !mime_type.is_empty() => GeminiRequestPart {
            text: None,
            inline_data: None,
            file_data: Some(GeminiFileData {
                mime_type,
                file_uri: url,
            }),
            function_call: None,
            function_response: None,
        },
        NormalizedContentPart::ImageUrl { url, .. } => GeminiRequestPart {
            text: Some(format!("Image URL: {url}")),
            inline_data: None,
            file_data: None,
            function_call: None,
            function_response: None,
        },
    }
}

fn convert_tools(tools: Option<&Vec<Tool>>) -> Option<Vec<GeminiTool>> {
    tools.map(|items| {
        vec![GeminiTool {
            function_declarations: items
                .iter()
                .map(|tool| GeminiFunctionDeclaration {
                    name: tool.function.name.clone(),
                    description: tool.function.description.clone(),
                    parameters: tool
                        .function
                        .parameters
                        .clone()
                        .unwrap_or_else(|| serde_json::json!({"type": "object"})),
                })
                .collect(),
        }]
    })
}

fn convert_tool_config(tool_choice: Option<&serde_json::Value>) -> Option<GeminiToolConfig> {
    let choice = tool_choice?;

    if let Some(choice) = choice.as_str() {
        let mode = match choice {
            "auto" => "AUTO",
            "required" => "ANY",
            "none" => "NONE",
            _ => return None,
        };

        return Some(GeminiToolConfig {
            function_calling_config: GeminiFunctionCallingConfig {
                mode: mode.to_string(),
                allowed_function_names: None,
            },
        });
    }

    let name = choice
        .get("function")
        .and_then(|function| function.get("name"))
        .and_then(|name| name.as_str())?;

    Some(GeminiToolConfig {
        function_calling_config: GeminiFunctionCallingConfig {
            mode: "ANY".into(),
            allowed_function_names: Some(vec![name.to_string()]),
        },
    })
}

fn convert_function_response_payload(content: &serde_json::Value) -> Option<serde_json::Value> {
    match content {
        serde_json::Value::Null => None,
        serde_json::Value::Object(map) => Some(serde_json::Value::Object(map.clone())),
        serde_json::Value::String(text) => {
            if text.is_empty() {
                return None;
            }

            match serde_json::from_str::<serde_json::Value>(text) {
                Ok(serde_json::Value::Object(map)) => Some(serde_json::Value::Object(map)),
                Ok(value) => Some(serde_json::json!({ "content": value })),
                Err(_) => Some(serde_json::json!({ "content": text })),
            }
        }
        other => Some(serde_json::json!({ "content": other })),
    }
}
