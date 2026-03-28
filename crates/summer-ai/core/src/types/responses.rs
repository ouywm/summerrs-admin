use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::chat::{ChatCompletionRequest, ChatCompletionResponse};
use super::common::{FinishReason, FunctionDef, Message, Tool, ToolCall, Usage};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ResponsesRequest {
    pub model: String,
    #[serde(default)]
    pub input: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(default)]
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<ResponsesTextConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ResponsesTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncation: Option<String>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ResponsesTextConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verbosity: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ResponsesTool {
    pub r#type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<serde_json::Value>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ResponsesResponse {
    pub id: String,
    pub object: String,
    pub created_at: i64,
    pub model: String,
    pub status: String,
    pub output: Vec<ResponsesOutputItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_text: Option<String>,
    pub usage: ResponsesUsage,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub incomplete_details: Option<ResponsesIncompleteDetails>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<ResponsesOutputTextConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ResponsesOutputTextConfig {
    pub format: ResponsesOutputTextFormat,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ResponsesOutputTextFormat {
    pub r#type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ResponsesIncompleteDetails {
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ResponsesOutputItem {
    pub id: String,
    pub r#type: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<Vec<ResponsesOutputContent>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ResponsesOutputContent {
    pub r#type: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ResponsesUsage {
    pub input_tokens: i32,
    pub output_tokens: i32,
    pub total_tokens: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_tokens_details: Option<ResponsesInputTokensDetails>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_tokens_details: Option<ResponsesOutputTokensDetails>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ResponsesInputTokensDetails {
    pub cached_tokens: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ResponsesOutputTokensDetails {
    pub reasoning_tokens: i32,
}

impl ResponsesRequest {
    pub fn to_chat_completion_request(&self) -> anyhow::Result<ChatCompletionRequest> {
        let mut messages = Vec::new();

        if let Some(instructions) = self.instructions.as_ref().map(|value| value.trim())
            && !instructions.is_empty()
        {
            messages.push(Message {
                role: "system".into(),
                content: serde_json::Value::String(instructions.to_string()),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            });
        }

        messages.extend(parse_responses_input(&self.input)?);

        if messages.is_empty() {
            anyhow::bail!("responses input cannot be empty");
        }

        let tools = self.tools.as_ref().map(|tools| {
            tools
                .iter()
                .filter_map(|tool| match tool.r#type.as_str() {
                    "function" => Some(Tool {
                        r#type: "function".into(),
                        function: FunctionDef {
                            name: tool.name.clone().unwrap_or_default(),
                            description: tool.description.clone(),
                            parameters: tool.parameters.clone(),
                        },
                    }),
                    _ => None,
                })
                .collect::<Vec<_>>()
        });

        Ok(ChatCompletionRequest {
            model: self.model.clone(),
            messages,
            stream: self.stream,
            temperature: self.temperature,
            max_tokens: self.max_output_tokens,
            top_p: self.top_p,
            frequency_penalty: None,
            presence_penalty: None,
            stop: None,
            tools,
            tool_choice: self.tool_choice.clone(),
            response_format: self.text.as_ref().and_then(|text| text.format.clone()),
            stream_options: None,
            extra: self.extra.clone(),
        })
    }
}

impl ResponsesResponse {
    pub fn from_chat_completion(
        request: &ResponsesRequest,
        response: &ChatCompletionResponse,
    ) -> Self {
        let mut output = Vec::new();
        let mut output_text_parts = Vec::new();
        let mut incomplete_details = None;
        let mut status = "completed".to_string();

        for (choice_index, choice) in response.choices.iter().enumerate() {
            match choice.finish_reason {
                Some(FinishReason::ToolCalls) => {
                    if let Some(tool_calls) = choice.message.tool_calls.as_ref() {
                        for (tool_index, tool_call) in tool_calls.iter().enumerate() {
                            output.push(function_call_output(
                                response,
                                choice_index,
                                tool_index,
                                tool_call,
                            ));
                        }
                    }
                }
                _ => {
                    let text = extract_message_text(&choice.message);
                    if !text.is_empty() {
                        output_text_parts.push(text.clone());
                    }
                    output.push(message_output(response, choice_index, text));
                }
            }

            match choice.finish_reason {
                Some(FinishReason::Length) => {
                    status = "incomplete".into();
                    incomplete_details = Some(ResponsesIncompleteDetails {
                        reason: "max_output_tokens".into(),
                    });
                }
                Some(FinishReason::ContentFilter) => {
                    status = "failed".into();
                    incomplete_details = Some(ResponsesIncompleteDetails {
                        reason: "content_filter".into(),
                    });
                }
                _ => {}
            }
        }

        Self {
            id: response.id.clone(),
            object: "response".into(),
            created_at: response.created,
            model: response.model.clone(),
            status,
            output,
            output_text: join_output_text(output_text_parts),
            usage: ResponsesUsage::from_usage(&response.usage),
            incomplete_details,
            text: Some(ResponsesOutputTextConfig {
                format: ResponsesOutputTextFormat {
                    r#type: request
                        .text
                        .as_ref()
                        .and_then(|text| text.format.as_ref())
                        .and_then(|format| format.get("type"))
                        .and_then(|format| format.as_str())
                        .unwrap_or("text")
                        .to_string(),
                },
            }),
        }
    }
}

impl ResponsesUsage {
    pub fn from_usage(usage: &Usage) -> Self {
        Self {
            input_tokens: usage.prompt_tokens,
            output_tokens: usage.completion_tokens,
            total_tokens: usage.total_tokens,
            input_tokens_details: Some(ResponsesInputTokensDetails {
                cached_tokens: usage.cached_tokens,
            }),
            output_tokens_details: if usage.reasoning_tokens > 0 {
                Some(ResponsesOutputTokensDetails {
                    reasoning_tokens: usage.reasoning_tokens,
                })
            } else {
                None
            },
        }
    }
}

fn parse_responses_input(input: &serde_json::Value) -> anyhow::Result<Vec<Message>> {
    if input.is_null() {
        return Ok(Vec::new());
    }

    if let Some(text) = input.as_str() {
        return Ok(vec![Message {
            role: "user".into(),
            content: serde_json::Value::String(text.to_string()),
            name: None,
            tool_calls: None,
            tool_call_id: None,
        }]);
    }

    let items = input
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("responses input must be a string or array"))?;

    let mut messages = Vec::new();
    for item in items {
        let item_type = item
            .get("type")
            .and_then(|value| value.as_str())
            .unwrap_or("message");

        match item_type {
            "message" => messages.push(parse_message_item(item)?),
            "function_call_output" => messages.push(parse_function_call_output(item)),
            "function_call" => messages.push(parse_function_call(item)?),
            _ => {}
        }
    }

    Ok(messages)
}

fn parse_message_item(item: &serde_json::Value) -> anyhow::Result<Message> {
    let role = item
        .get("role")
        .and_then(|value| value.as_str())
        .unwrap_or("user");
    let role = if role == "developer" { "system" } else { role };
    let content = match item.get("content") {
        Some(content) => parse_message_content(content)?,
        None => serde_json::Value::String(String::new()),
    };

    Ok(Message {
        role: role.to_string(),
        content,
        name: None,
        tool_calls: None,
        tool_call_id: None,
    })
}

fn parse_function_call_output(item: &serde_json::Value) -> Message {
    let content = item
        .get("output")
        .cloned()
        .unwrap_or_else(|| serde_json::Value::String(String::new()));

    Message {
        role: "tool".into(),
        content: stringify_content_value(content),
        name: None,
        tool_calls: None,
        tool_call_id: item
            .get("call_id")
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned),
    }
}

fn parse_function_call(item: &serde_json::Value) -> anyhow::Result<Message> {
    let call_id = item
        .get("call_id")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string();
    let name = item
        .get("name")
        .and_then(|value| value.as_str())
        .ok_or_else(|| anyhow::anyhow!("responses function_call name is required"))?
        .to_string();
    let arguments = item
        .get("arguments")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string();

    Ok(Message {
        role: "assistant".into(),
        content: serde_json::Value::String(String::new()),
        name: None,
        tool_calls: Some(vec![ToolCall {
            id: call_id,
            r#type: "function".into(),
            function: super::common::FunctionCall { name, arguments },
        }]),
        tool_call_id: None,
    })
}

fn parse_message_content(content: &serde_json::Value) -> anyhow::Result<serde_json::Value> {
    if let Some(text) = content.as_str() {
        return Ok(serde_json::Value::String(text.to_string()));
    }

    let parts = content
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("responses message content must be a string or array"))?;
    let mapped = parts
        .iter()
        .filter_map(map_content_part)
        .collect::<Vec<serde_json::Value>>();
    Ok(serde_json::Value::Array(mapped))
}

fn map_content_part(part: &serde_json::Value) -> Option<serde_json::Value> {
    let part_type = part.get("type").and_then(|value| value.as_str())?;

    match part_type {
        "input_text" | "output_text" | "summary_text" => Some(serde_json::json!({
            "type": "text",
            "text": part.get("text").and_then(|value| value.as_str()).unwrap_or_default(),
        })),
        "input_image" => {
            let image_url = part
                .get("image_url")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            let detail = part
                .get("detail")
                .cloned()
                .unwrap_or(serde_json::Value::String("auto".into()));
            Some(serde_json::json!({
                "type": "image_url",
                "image_url": {
                    "url": image_url,
                    "detail": detail
                }
            }))
        }
        "input_file" => Some(serde_json::json!({
            "type": "file",
            "file": {
                "file_id": part.get("file_id").cloned().unwrap_or(serde_json::Value::Null),
                "file_data": part.get("file_data").cloned().unwrap_or(serde_json::Value::Null),
                "filename": part.get("filename").cloned().or_else(|| part.get("file_name").cloned()).unwrap_or(serde_json::Value::Null),
                "file_url": part.get("file_url").cloned().unwrap_or(serde_json::Value::Null)
            }
        })),
        _ => None,
    }
}

fn stringify_content_value(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::String(text) => serde_json::Value::String(text),
        other => serde_json::Value::String(other.to_string()),
    }
}

fn message_output(
    response: &ChatCompletionResponse,
    choice_index: usize,
    text: String,
) -> ResponsesOutputItem {
    ResponsesOutputItem {
        id: format!("msg_{}_{}", response.id, choice_index),
        r#type: "message".into(),
        status: "completed".into(),
        role: Some("assistant".into()),
        content: Some(vec![ResponsesOutputContent {
            r#type: "output_text".into(),
            text,
        }]),
        call_id: None,
        name: None,
        arguments: None,
    }
}

fn function_call_output(
    response: &ChatCompletionResponse,
    choice_index: usize,
    tool_index: usize,
    tool_call: &ToolCall,
) -> ResponsesOutputItem {
    ResponsesOutputItem {
        id: format!("fc_{}_{}_{}", response.id, choice_index, tool_index),
        r#type: "function_call".into(),
        status: "completed".into(),
        role: None,
        content: None,
        call_id: Some(tool_call.id.clone()),
        name: Some(tool_call.function.name.clone()),
        arguments: Some(tool_call.function.arguments.clone()),
    }
}

fn extract_message_text(message: &Message) -> String {
    match &message.content {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Array(parts) => parts
            .iter()
            .filter_map(|part| {
                if part.get("type").and_then(|value| value.as_str()) == Some("text") {
                    part.get("text").and_then(|value| value.as_str())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join(""),
        other => other.to_string(),
    }
}

fn join_output_text(parts: Vec<String>) -> Option<String> {
    let text = parts.join("");
    if text.is_empty() { None } else { Some(text) }
}

#[cfg(test)]
mod tests {
    use super::super::common::Message;
    use super::*;

    #[test]
    fn responses_request_converts_string_input_to_chat_messages() {
        let request: ResponsesRequest = serde_json::from_value(serde_json::json!({
            "model": "gpt-5.4",
            "input": "hello",
            "instructions": "be concise"
        }))
        .unwrap();

        let chat = request.to_chat_completion_request().unwrap();
        assert_eq!(chat.messages.len(), 2);
        assert_eq!(chat.messages[0].role, "system");
        assert_eq!(chat.messages[1].role, "user");
    }

    #[test]
    fn responses_request_converts_message_and_tool_output_items() {
        let request: ResponsesRequest = serde_json::from_value(serde_json::json!({
            "model": "gpt-5.4",
            "input": [
                {"role": "user", "content": [{"type": "input_text", "text": "weather?"}]},
                {"type": "function_call_output", "call_id": "call_1", "output": {"temp": 25}}
            ],
            "tools": [
                {"type": "function", "name": "get_weather", "description": "Get weather", "parameters": {"type": "object"}}
            ]
        }))
        .unwrap();

        let chat = request.to_chat_completion_request().unwrap();
        assert_eq!(chat.messages.len(), 2);
        assert_eq!(chat.messages[0].role, "user");
        assert_eq!(chat.messages[1].role, "tool");
        assert_eq!(chat.tools.unwrap()[0].function.name, "get_weather");
    }

    #[test]
    fn responses_response_wraps_plain_chat_message() {
        let request: ResponsesRequest = serde_json::from_value(serde_json::json!({
            "model": "gpt-5.4",
            "input": "hello"
        }))
        .unwrap();
        let response = ChatCompletionResponse {
            id: "chatcmpl_123".into(),
            object: "chat.completion".into(),
            created: 1_700_000_000,
            model: "gpt-5.4".into(),
            choices: vec![super::super::chat::Choice {
                index: 0,
                message: Message {
                    role: "assistant".into(),
                    content: serde_json::Value::String("Hi there".into()),
                    name: None,
                    tool_calls: None,
                    tool_call_id: None,
                },
                finish_reason: Some(FinishReason::Stop),
            }],
            usage: Usage {
                prompt_tokens: 10,
                completion_tokens: 5,
                total_tokens: 15,
                cached_tokens: 0,
                reasoning_tokens: 0,
            },
        };

        let wrapped = ResponsesResponse::from_chat_completion(&request, &response);
        assert_eq!(wrapped.object, "response");
        assert_eq!(wrapped.output[0].r#type, "message");
        assert_eq!(wrapped.output_text.as_deref(), Some("Hi there"));
        assert_eq!(wrapped.usage.input_tokens, 10);
    }

    #[test]
    fn responses_response_wraps_tool_calls() {
        let request: ResponsesRequest = serde_json::from_value(serde_json::json!({
            "model": "gpt-5.4",
            "input": "hello"
        }))
        .unwrap();
        let response = ChatCompletionResponse {
            id: "chatcmpl_123".into(),
            object: "chat.completion".into(),
            created: 1_700_000_000,
            model: "gpt-5.4".into(),
            choices: vec![super::super::chat::Choice {
                index: 0,
                message: Message {
                    role: "assistant".into(),
                    content: serde_json::Value::String(String::new()),
                    name: None,
                    tool_calls: Some(vec![ToolCall {
                        id: "call_1".into(),
                        r#type: "function".into(),
                        function: super::super::common::FunctionCall {
                            name: "get_weather".into(),
                            arguments: "{\"city\":\"Shanghai\"}".into(),
                        },
                    }]),
                    tool_call_id: None,
                },
                finish_reason: Some(FinishReason::ToolCalls),
            }],
            usage: Usage {
                prompt_tokens: 10,
                completion_tokens: 5,
                total_tokens: 15,
                cached_tokens: 0,
                reasoning_tokens: 0,
            },
        };

        let wrapped = ResponsesResponse::from_chat_completion(&request, &response);
        assert_eq!(wrapped.output[0].r#type, "function_call");
        assert_eq!(wrapped.output[0].call_id.as_deref(), Some("call_1"));
        assert_eq!(wrapped.status, "completed");
    }
}
