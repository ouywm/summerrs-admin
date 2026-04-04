use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use bytes::Bytes;
use futures::StreamExt;
use futures::stream::BoxStream;
use reqwest::header::HeaderMap;
use serde::{Deserialize, Serialize};

use crate::types::chat::{
    ChatCompletionChunk, ChatCompletionRequest, ChatCompletionResponse, Choice, ChunkChoice,
};
use crate::types::common::{
    Delta, FinishReason, FunctionCall, FunctionCallDelta, Message, Tool, ToolCall, ToolCallDelta,
    Usage,
};
use crate::types::sse_parser::SseParser;

use super::{
    ProviderAdapter, ProviderErrorInfo, ProviderErrorKind, ProviderStreamError,
    ResponsesRuntimeMode, merge_extra_body_fields, responses_request_to_chat_request,
    status_to_provider_error_kind,
};

pub struct AnthropicAdapter;

#[derive(Debug, Serialize)]
struct AnthropicRequest {
    model: String,
    messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    max_tokens: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_sequences: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<AnthropicTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    stream: bool,
}

#[derive(Debug, Serialize)]
struct AnthropicMessage {
    role: String,
    content: Vec<serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct AnthropicTool {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    input_schema: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    id: String,
    model: String,
    #[serde(default)]
    content: Vec<AnthropicContentBlock>,
    #[serde(default)]
    stop_reason: Option<String>,
    #[serde(default)]
    usage: AnthropicUsage,
}

#[derive(Debug, Deserialize)]
struct AnthropicContentBlock {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    _thinking: String,
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    input: serde_json::Value,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct AnthropicUsage {
    #[serde(default)]
    input_tokens: i32,
    #[serde(default)]
    output_tokens: i32,
    #[serde(default)]
    cache_read_input_tokens: i32,
    #[serde(default)]
    cache_creation_input_tokens: i32,
}

#[derive(Debug, Default)]
struct AnthropicStreamState {
    id: String,
    model: String,
    created: i64,
    usage: Usage,
    role_emitted: bool,
    next_tool_call_index: i32,
    block_tool_call_index: HashMap<u64, i32>,
}

#[derive(Debug, Deserialize)]
struct AnthropicStreamEnvelope {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    index: Option<u64>,
    #[serde(default)]
    message: Option<AnthropicStreamMessage>,
    #[serde(default)]
    content_block: Option<AnthropicStreamContentBlock>,
    #[serde(default)]
    delta: Option<AnthropicStreamDelta>,
    #[serde(default)]
    usage: Option<AnthropicUsage>,
    #[serde(default)]
    error: Option<AnthropicStreamError>,
}

#[derive(Debug, Deserialize)]
struct AnthropicStreamMessage {
    id: String,
    model: String,
    #[serde(default)]
    usage: AnthropicUsage,
}

#[derive(Debug, Deserialize)]
struct AnthropicStreamContentBlock {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
}

#[derive(Debug, Deserialize)]
struct AnthropicStreamDelta {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    thinking: String,
    #[serde(default, rename = "partial_json")]
    partial_json: String,
    #[serde(default)]
    stop_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnthropicStreamError {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    message: String,
}

impl ProviderAdapter for AnthropicAdapter {
    fn build_request(
        &self,
        client: &reqwest::Client,
        base_url: &str,
        api_key: &str,
        req: &ChatCompletionRequest,
        actual_model: &str,
    ) -> Result<reqwest::RequestBuilder> {
        let omit_tools_for_none = tool_choice_is_none(req.tool_choice.as_ref());
        let mut body = serde_json::to_value(AnthropicRequest {
            model: actual_model.to_string(),
            messages: convert_messages(&req.messages),
            system: collect_system_prompt(&req.messages),
            max_tokens: req.max_tokens.unwrap_or(4096),
            temperature: req.temperature,
            top_p: req.top_p,
            stop_sequences: convert_stop_sequences(req.stop.as_ref()),
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

        Ok(client
            .post(build_anthropic_url(base_url))
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body))
    }

    fn parse_response(&self, body: Bytes, _model: &str) -> Result<ChatCompletionResponse> {
        let response: AnthropicResponse =
            serde_json::from_slice(&body).context("failed to deserialize anthropic response")?;

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

        Ok(ChatCompletionResponse {
            id: response.id,
            object: "chat.completion".into(),
            created: unix_timestamp(),
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
        })
    }

    fn parse_stream(
        &self,
        response: reqwest::Response,
        _model: &str,
    ) -> Result<BoxStream<'static, Result<ChatCompletionChunk>>> {
        let stream = async_stream::stream! {
            let mut byte_stream = response.bytes_stream();
            let mut parser = SseParser::new();
            let mut state = AnthropicStreamState {
                created: unix_timestamp(),
                ..Default::default()
            };

            while let Some(chunk_result) = byte_stream.next().await {
                let chunk = match chunk_result {
                    Ok(chunk) => chunk,
                    Err(error) => {
                        yield Err(anyhow::anyhow!("anthropic stream read error: {error}"));
                        break;
                    }
                };

                let events = match parser.feed(&chunk) {
                    Ok(events) => events,
                    Err(error) => {
                        yield Err(error.context("failed to parse anthropic SSE event bytes"));
                        break;
                    }
                };

                for event_text in events {
                    let Some((event_name, data)) = parse_sse_event(&event_text) else {
                        continue;
                    };
                    if data.is_empty() || data == "[DONE]" {
                        continue;
                    }

                    let envelope: AnthropicStreamEnvelope = match serde_json::from_str(&data) {
                        Ok(envelope) => envelope,
                        Err(error) => {
                            tracing::warn!("failed to parse anthropic SSE event: {error}, data: {data}");
                            continue;
                        }
                    };

                    let kind = if envelope.kind.is_empty() {
                        event_name.as_deref().unwrap_or_default()
                    } else {
                        envelope.kind.as_str()
                    };

                    match kind {
                        "message_start" => {
                            if let Some(message) = envelope.message {
                                state.id = message.id;
                                state.model = message.model;
                                merge_anthropic_usage(&mut state.usage, message.usage);
                                if !state.role_emitted {
                                    state.role_emitted = true;
                                    yield Ok(chunk_with_delta(
                                        &state,
                                        Delta {
                                            role: Some("assistant".into()),
                                            content: None,
                                            reasoning_content: None,
                                            tool_calls: None,
                                        },
                                        None,
                                        None,
                                    ));
                                }
                            }
                        }
                        "content_block_start" => {
                            if let Some(block) = envelope.content_block
                                && block.kind == "tool_use"
                            {
                                let index = state.next_tool_call_index;
                                state.next_tool_call_index += 1;
                                if let Some(block_index) = envelope.index {
                                    state.block_tool_call_index.insert(block_index, index);
                                }
                                yield Ok(chunk_with_delta(
                                    &state,
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
                                ));
                            }
                        }
                        "content_block_delta" => {
                            if let Some(delta) = envelope.delta {
                                match delta.kind.as_str() {
                                    "text_delta" if !delta.text.is_empty() => {
                                        yield Ok(chunk_with_delta(
                                            &state,
                                            Delta {
                                                role: None,
                                                content: Some(delta.text),
                                                reasoning_content: None,
                                                tool_calls: None,
                                            },
                                            None,
                                            None,
                                        ));
                                    }
                                    "thinking_delta" if !delta.thinking.is_empty() => {
                                        yield Ok(chunk_with_delta(
                                            &state,
                                            Delta {
                                                role: None,
                                                content: None,
                                                reasoning_content: Some(delta.thinking),
                                                tool_calls: None,
                                            },
                                            None,
                                            None,
                                        ));
                                    }
                                    "input_json_delta" if !delta.partial_json.is_empty() => {
                                        if let Some(block_index) = envelope.index
                                            && let Some(tool_index) = state.block_tool_call_index.get(&block_index).copied()
                                        {
                                            yield Ok(chunk_with_delta(
                                                &state,
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
                                            ));
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        "message_delta" => {
                            if let Some(usage) = envelope.usage {
                                merge_anthropic_usage(&mut state.usage, usage);
                            }

                            let finish_reason = envelope.delta.and_then(|delta| {
                                map_anthropic_stream_finish_reason(delta.stop_reason.as_deref(), false)
                            });

                            yield Ok(chunk_with_delta(
                                &state,
                                Delta {
                                    role: None,
                                    content: None,
                                    reasoning_content: None,
                                    tool_calls: None,
                                },
                                finish_reason,
                                Some(state.usage.clone()),
                            ));
                        }
                        "error" => {
                            if let Some(error) = envelope.error {
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
                                yield Err(anyhow::Error::new(ProviderStreamError::new(info))
                                    .context(format!("anthropic stream error [{kind}]")));
                                return;
                            }
                        }
                        "message_stop" => return,
                        _ => {}
                    }
                }
            }
        };

        Ok(Box::pin(stream))
    }

    fn build_responses_request(
        &self,
        client: &reqwest::Client,
        base_url: &str,
        api_key: &str,
        req: &serde_json::Value,
        actual_model: &str,
    ) -> Result<reqwest::RequestBuilder> {
        let req: crate::types::responses::ResponsesRequest = serde_json::from_value(req.clone())
            .context("failed to deserialize responses request")?;
        let chat_req = responses_request_to_chat_request(&req);
        self.build_request(client, base_url, api_key, &chat_req, actual_model)
    }

    fn responses_runtime_mode(&self) -> ResponsesRuntimeMode {
        ResponsesRuntimeMode::ChatBridge
    }

    fn parse_error(&self, status: u16, _headers: &HeaderMap, body: &[u8]) -> ProviderErrorInfo {
        let payload: serde_json::Value =
            serde_json::from_slice(body).unwrap_or_else(|_| serde_json::json!({}));
        let error_obj = payload.get("error").unwrap_or(&payload);
        let error_type = error_obj
            .get("type")
            .and_then(|value| value.as_str())
            .unwrap_or_else(|| default_anthropic_error_code(status));
        let message = error_obj
            .get("message")
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| String::from_utf8_lossy(body).trim().to_string());

        let kind = anthropic_error_kind(error_type)
            .unwrap_or_else(|| status_to_provider_error_kind(status));

        ProviderErrorInfo::new(kind, message, error_type)
    }
}

fn build_anthropic_url(base_url: &str) -> String {
    let base = base_url.trim_end_matches('/');
    if base.ends_with("/v1") {
        format!("{base}/messages")
    } else {
        format!("{base}/v1/messages")
    }
}

fn default_anthropic_error_code(status: u16) -> &'static str {
    match status_to_provider_error_kind(status) {
        ProviderErrorKind::InvalidRequest => "invalid_request_error",
        ProviderErrorKind::Authentication => "authentication_error",
        ProviderErrorKind::RateLimit => "rate_limit_error",
        ProviderErrorKind::Server => "api_error",
        ProviderErrorKind::Api => "api_error",
    }
}

fn collect_system_prompt(messages: &[Message]) -> Option<String> {
    let prompt = messages
        .iter()
        .filter(|message| message.role == "system")
        .filter_map(|message| extract_text_segments(&message.content))
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");

    (!prompt.is_empty()).then_some(prompt)
}

fn convert_messages(messages: &[Message]) -> Vec<AnthropicMessage> {
    let mut converted = Vec::new();

    for message in messages {
        match message.role.as_str() {
            "system" => {}
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

fn convert_stop_sequences(stop: Option<&serde_json::Value>) -> Option<Vec<String>> {
    let stop = stop?;

    if let Some(stop) = stop.as_str() {
        return Some(vec![stop.to_string()]);
    }

    stop.as_array().map(|items| {
        items
            .iter()
            .filter_map(|item| item.as_str().map(ToOwned::to_owned))
            .collect()
    })
}

fn content_blocks_from_value(content: &serde_json::Value) -> Vec<serde_json::Value> {
    match content {
        serde_json::Value::Null => Vec::new(),
        serde_json::Value::String(text) => {
            if text.is_empty() {
                Vec::new()
            } else {
                vec![serde_json::json!({ "type": "text", "text": text })]
            }
        }
        serde_json::Value::Array(items) => items
            .iter()
            .filter_map(content_block_from_openai_part)
            .collect(),
        serde_json::Value::Object(_) => content_block_from_openai_part(content)
            .into_iter()
            .collect(),
        other => vec![serde_json::json!({ "type": "text", "text": other.to_string() })],
    }
}

fn content_block_from_openai_part(part: &serde_json::Value) -> Option<serde_json::Value> {
    match part {
        serde_json::Value::String(text) => Some(serde_json::json!({
            "type": "text",
            "text": text,
        })),
        serde_json::Value::Object(map) => match map.get("type").and_then(|value| value.as_str()) {
            Some("text") => map
                .get("text")
                .and_then(|value| value.as_str())
                .map(|text| {
                    serde_json::json!({
                        "type": "text",
                        "text": text,
                    })
                }),
            Some("image_url") => {
                let url = map
                    .get("image_url")
                    .and_then(|value| value.get("url"))
                    .and_then(|value| value.as_str())
                    .or_else(|| map.get("image_url").and_then(|value| value.as_str()));

                url.and_then(anthropic_image_block_from_url)
            }
            _ => map
                .get("text")
                .and_then(|value| value.as_str())
                .map(|text| {
                    serde_json::json!({
                        "type": "text",
                        "text": text,
                    })
                }),
        },
        _ => None,
    }
}

fn anthropic_image_block_from_url(url: &str) -> Option<serde_json::Value> {
    if let Some((media_type, data)) = parse_data_url(url) {
        return Some(serde_json::json!({
            "type": "image",
            "source": {
                "type": "base64",
                "media_type": media_type,
                "data": data,
            },
        }));
    }

    Some(serde_json::json!({
        "type": "image",
        "source": {
            "type": "url",
            "url": url,
        },
    }))
}

fn parse_data_url(url: &str) -> Option<(&str, &str)> {
    let data = url.strip_prefix("data:")?;
    let (meta, payload) = data.split_once(',')?;
    let media_type = meta.strip_suffix(";base64")?;
    Some((media_type, payload))
}

fn extract_text_segments(content: &serde_json::Value) -> Option<String> {
    match content {
        serde_json::Value::String(text) => Some(text.clone()),
        serde_json::Value::Array(items) => {
            let text = items
                .iter()
                .filter_map(|item| {
                    item.get("text")
                        .and_then(|value| value.as_str())
                        .map(ToOwned::to_owned)
                })
                .collect::<Vec<_>>()
                .join("\n");
            (!text.is_empty()).then_some(text)
        }
        _ => None,
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

fn parse_function_arguments(arguments: &str) -> serde_json::Value {
    serde_json::from_str(arguments).unwrap_or_else(|e| {
        tracing::warn!(arguments, error = %e, "failed to parse tool call arguments as JSON, passing as raw string");
        serde_json::Value::String(arguments.into())
    })
}

fn serialize_arguments(arguments: serde_json::Value) -> String {
    match arguments {
        serde_json::Value::String(arguments) => arguments,
        other => serde_json::to_string(&other).unwrap_or_else(|_| "{}".into()),
    }
}

fn usage_from_anthropic(usage: AnthropicUsage) -> Usage {
    let total_tokens = usage.input_tokens + usage.output_tokens;
    Usage {
        prompt_tokens: usage.input_tokens,
        completion_tokens: usage.output_tokens,
        total_tokens,
        cached_tokens: usage.cache_read_input_tokens + usage.cache_creation_input_tokens,
        reasoning_tokens: 0,
    }
}

fn merge_anthropic_usage(state: &mut Usage, usage: AnthropicUsage) {
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

fn joined_text_value(texts: Vec<String>) -> serde_json::Value {
    if texts.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::Value::String(texts.join(""))
    }
}

fn map_anthropic_finish_reason(
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

fn map_anthropic_stream_finish_reason(
    finish_reason: Option<&str>,
    has_tool_calls: bool,
) -> Option<FinishReason> {
    finish_reason.and_then(|reason| map_anthropic_finish_reason(Some(reason), has_tool_calls))
}

fn anthropic_error_kind(error_type: &str) -> Option<ProviderErrorKind> {
    match error_type {
        "invalid_request_error" | "not_found_error" => Some(ProviderErrorKind::InvalidRequest),
        "authentication_error" | "permission_error" => Some(ProviderErrorKind::Authentication),
        "rate_limit_error" => Some(ProviderErrorKind::RateLimit),
        "overloaded_error" | "api_error" => Some(ProviderErrorKind::Server),
        _ => None,
    }
}

fn parse_sse_event(event_text: &str) -> Option<(Option<String>, String)> {
    let mut event_name = None;
    let mut data_lines = Vec::new();

    for line in event_text.lines() {
        let line = line.trim_end_matches('\r');
        if let Some(rest) = line.strip_prefix("event:") {
            event_name = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("data:") {
            data_lines.push(rest.trim_start().to_string());
        }
    }

    if data_lines.is_empty() {
        None
    } else {
        Some((event_name, data_lines.join("\n")))
    }
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

fn unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[cfg(test)]
mod tests;
