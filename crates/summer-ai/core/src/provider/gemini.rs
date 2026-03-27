use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use bytes::Bytes;
use futures::stream::BoxStream;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use summer_web::axum::http::{HeaderMap, StatusCode};

use crate::types::chat::{
    ChatCompletionChunk, ChatCompletionRequest, ChatCompletionResponse, Choice, ChunkChoice,
};
use crate::types::common::{
    Delta, FinishReason, FunctionCall, FunctionCallDelta, Message, Tool, ToolCall, ToolCallDelta,
    Usage,
};

use super::{
    ProviderAdapter, ProviderErrorInfo, ProviderErrorKind, ProviderStreamError,
    status_to_provider_error_kind,
};

pub struct GeminiAdapter;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiRequest {
    contents: Vec<GeminiRequestContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation_config: Option<GeminiGenerationConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<GeminiTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<GeminiSystemInstruction>,
}

#[derive(Debug, Serialize)]
struct GeminiRequestContent {
    role: String,
    parts: Vec<GeminiRequestPart>,
}

#[derive(Debug, Serialize)]
struct GeminiRequestPart {
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(rename = "functionCall", skip_serializing_if = "Option::is_none")]
    function_call: Option<GeminiFunctionCall>,
    #[serde(rename = "functionResponse", skip_serializing_if = "Option::is_none")]
    function_response: Option<GeminiFunctionResponse>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct GeminiFunctionCall {
    name: String,
    args: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct GeminiFunctionResponse {
    name: String,
    response: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct GeminiTool {
    #[serde(rename = "functionDeclarations")]
    function_declarations: Vec<GeminiFunctionDeclaration>,
}

#[derive(Debug, Serialize)]
struct GeminiFunctionDeclaration {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    parameters: serde_json::Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiGenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_sequences: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_mime_type: Option<String>,
}

#[derive(Debug, Serialize)]
struct GeminiSystemInstruction {
    parts: Vec<GeminiTextPart>,
}

#[derive(Debug, Serialize)]
struct GeminiTextPart {
    text: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiResponse {
    #[serde(default)]
    candidates: Vec<GeminiCandidate>,
    #[serde(default)]
    usage_metadata: Option<GeminiUsageMetadata>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiCandidate {
    #[serde(default)]
    content: Option<GeminiResponseContent>,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GeminiResponseContent {
    #[serde(default)]
    parts: Vec<GeminiResponsePart>,
}

#[derive(Debug, Deserialize)]
struct GeminiResponsePart {
    #[serde(default)]
    text: Option<String>,
    #[serde(rename = "functionCall", default)]
    function_call: Option<GeminiFunctionCall>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiUsageMetadata {
    #[serde(default)]
    prompt_token_count: i32,
    #[serde(default)]
    candidates_token_count: i32,
    #[serde(default)]
    total_token_count: i32,
}

#[derive(Debug, Default)]
struct GeminiStreamState {
    id: String,
    model: String,
    created: i64,
    role_emitted: bool,
    next_tool_call_index: i32,
    saw_tool_call: bool,
}

impl ProviderAdapter for GeminiAdapter {
    fn build_request(
        &self,
        client: &reqwest::Client,
        base_url: &str,
        api_key: &str,
        req: &ChatCompletionRequest,
        actual_model: &str,
    ) -> Result<reqwest::RequestBuilder> {
        let body = GeminiRequest {
            contents: convert_contents(&req.messages),
            generation_config: build_generation_config(req),
            tools: convert_tools(req.tools.as_ref()),
            system_instruction: collect_system_instruction(&req.messages),
        };

        let url = if req.stream {
            format!(
                "{}?alt=sse",
                build_gemini_url(base_url, actual_model, true)
            )
        } else {
            build_gemini_url(base_url, actual_model, false)
        };

        Ok(client
            .post(url)
            .header("x-goog-api-key", api_key)
            .json(&body))
    }

    fn parse_response(&self, body: Bytes, model: &str) -> Result<ChatCompletionResponse> {
        let response: GeminiResponse =
            serde_json::from_slice(&body).context("failed to deserialize gemini response")?;
        convert_response(response, model)
    }

    fn parse_stream(
        &self,
        response: reqwest::Response,
        model: &str,
    ) -> Result<BoxStream<'static, Result<ChatCompletionChunk>>> {
        let model = model.to_string();
        let stream = async_stream::stream! {
            let mut byte_stream = response.bytes_stream();
            let mut buffer = String::new();
            let mut state = GeminiStreamState {
                id: format!("gemini-{}", unix_timestamp()),
                model,
                created: unix_timestamp(),
                ..Default::default()
            };

            while let Some(chunk_result) = byte_stream.next().await {
                let chunk = match chunk_result {
                    Ok(chunk) => chunk,
                    Err(error) => {
                        yield Err(anyhow::anyhow!("gemini stream read error: {error}"));
                        break;
                    }
                };

                buffer.push_str(&String::from_utf8_lossy(&chunk));

                while let Some(pos) = buffer.find("\n\n") {
                    let event_text = buffer[..pos].to_string();
                    buffer = buffer[pos + 2..].to_string();

                    let Some(data) = parse_gemini_sse_data(&event_text) else {
                        continue;
                    };
                    if data.is_empty() || data == "[DONE]" {
                        continue;
                    }

                    let payload: serde_json::Value = match serde_json::from_str(&data) {
                        Ok(payload) => payload,
                        Err(error) => {
                            tracing::warn!("failed to parse gemini SSE event: {error}, data: {data}");
                            continue;
                        }
                    };

                    if let Some(info) = parse_gemini_stream_error(&payload) {
                        let code = info.code.clone();
                        yield Err(anyhow::Error::new(ProviderStreamError::new(info))
                            .context(format!("gemini stream error [{code}]")));
                        return;
                    }

                    let response: GeminiResponse = match serde_json::from_value(payload) {
                        Ok(response) => response,
                        Err(error) => {
                            tracing::warn!("failed to parse gemini SSE event: {error}, data: {data}");
                            continue;
                        }
                    };

                    let usage = response.usage_metadata.clone().map(usage_from_gemini);
                    let candidate = response.candidates.into_iter().next();
                    let mut saw_tool_call = false;

                    if let Some(candidate) = candidate {
                        if let Some(content) = candidate.content {
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

                            for part in content.parts {
                                if let Some(text) = part.text
                                    && !text.is_empty()
                                {
                                    yield Ok(chunk_with_delta(
                                        &state,
                                        Delta {
                                            role: None,
                                            content: Some(text),
                                            reasoning_content: None,
                                            tool_calls: None,
                                        },
                                        None,
                                        None,
                                    ));
                                }

                                if let Some(function_call) = part.function_call {
                                    saw_tool_call = true;
                                    state.saw_tool_call = true;
                                    let tool_index = state.next_tool_call_index;
                                    state.next_tool_call_index += 1;
                                    yield Ok(chunk_with_delta(
                                        &state,
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
                                    ));
                                }
                            }
                        }

                        let finish_reason = map_gemini_stream_finish_reason(
                            candidate.finish_reason.as_deref(),
                            state.saw_tool_call || saw_tool_call,
                        );
                        if finish_reason.is_some() || usage.is_some() {
                            yield Ok(chunk_with_delta(
                                &state,
                                Delta {
                                    role: None,
                                    content: None,
                                    reasoning_content: None,
                                    tool_calls: None,
                                },
                                finish_reason,
                                usage,
                            ));
                        }
                    } else if let Some(usage) = usage {
                        yield Ok(chunk_with_delta(
                            &state,
                            Delta {
                                role: None,
                                content: None,
                                reasoning_content: None,
                                tool_calls: None,
                            },
                            None,
                            Some(usage),
                        ));
                    }
                }
            }
        };

        Ok(Box::pin(stream))
    }

    fn parse_error(
        &self,
        status: StatusCode,
        _headers: &HeaderMap,
        body: &[u8],
    ) -> ProviderErrorInfo {
        let payload: serde_json::Value =
            serde_json::from_slice(body).unwrap_or_else(|_| serde_json::json!({}));
        let error_obj = payload.get("error").unwrap_or(&payload);
        let error_code = error_obj
            .get("status")
            .and_then(|value| value.as_str())
            .unwrap_or_else(|| default_gemini_error_code(status));
        let message = error_obj
            .get("message")
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| String::from_utf8_lossy(body).trim().to_string());

        let kind =
            gemini_error_kind(error_code).unwrap_or_else(|| status_to_provider_error_kind(status));

        ProviderErrorInfo::new(kind, message, error_code)
    }
}

fn build_gemini_url(base_url: &str, model: &str, stream: bool) -> String {
    let base = base_url.trim_end_matches('/');
    let version_base = if base.ends_with("/v1beta") || base.ends_with("/v1") {
        base.to_string()
    } else {
        format!("{base}/v1beta")
    };

    let action = if stream {
        "streamGenerateContent"
    } else {
        "generateContent"
    };

    format!("{version_base}/models/{model}:{action}")
}

fn default_gemini_error_code(status: StatusCode) -> &'static str {
    match status_to_provider_error_kind(status) {
        ProviderErrorKind::InvalidRequest => "INVALID_ARGUMENT",
        ProviderErrorKind::Authentication => "UNAUTHENTICATED",
        ProviderErrorKind::RateLimit => "RESOURCE_EXHAUSTED",
        ProviderErrorKind::Server => "INTERNAL",
        ProviderErrorKind::Api => "UNKNOWN",
    }
}

fn gemini_error_kind(error_code: &str) -> Option<ProviderErrorKind> {
    match error_code {
        "INVALID_ARGUMENT" | "NOT_FOUND" => Some(ProviderErrorKind::InvalidRequest),
        "FAILED_PRECONDITION" => Some(ProviderErrorKind::Api),
        "UNAUTHENTICATED" | "PERMISSION_DENIED" => Some(ProviderErrorKind::Authentication),
        "RESOURCE_EXHAUSTED" => Some(ProviderErrorKind::RateLimit),
        "INTERNAL" | "UNAVAILABLE" | "DEADLINE_EXCEEDED" => Some(ProviderErrorKind::Server),
        _ => None,
    }
}

fn parse_gemini_stream_error(payload: &serde_json::Value) -> Option<ProviderErrorInfo> {
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

fn build_generation_config(req: &ChatCompletionRequest) -> Option<GeminiGenerationConfig> {
    let response_mime_type = req
        .response_format
        .as_ref()
        .and_then(|value| value.get("type"))
        .and_then(|value| value.as_str())
        .filter(|value| *value == "json_object")
        .map(|_| "application/json".to_string());

    let stop_sequences = req.stop.as_ref().and_then(convert_stop_sequences);

    if req.temperature.is_none()
        && req.top_p.is_none()
        && req.max_tokens.is_none()
        && stop_sequences.is_none()
        && response_mime_type.is_none()
    {
        return None;
    }

    Some(GeminiGenerationConfig {
        temperature: req.temperature,
        top_p: req.top_p,
        max_output_tokens: req.max_tokens,
        stop_sequences,
        response_mime_type,
    })
}

fn collect_system_instruction(messages: &[Message]) -> Option<GeminiSystemInstruction> {
    let text = messages
        .iter()
        .filter(|message| message.role == "system")
        .filter_map(|message| extract_text_segments(&message.content))
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");

    (!text.is_empty()).then_some(GeminiSystemInstruction {
        parts: vec![GeminiTextPart { text }],
    })
}

fn convert_contents(messages: &[Message]) -> Vec<GeminiRequestContent> {
    let mut contents = Vec::new();
    let mut tool_call_names = HashMap::new();

    for message in messages {
        match message.role.as_str() {
            "system" => {}
            "assistant" => {
                let mut parts = parts_from_content(&message.content);
                if let Some(tool_calls) = message.tool_calls.as_ref() {
                    parts.extend(tool_calls.iter().map(|tool_call| {
                        tool_call_names.insert(tool_call.id.clone(), tool_call.function.name.clone());
                        GeminiRequestPart {
                            text: None,
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
                let content_text = extract_tool_result_content(&message.content);
                if !content_text.is_empty() {
                    contents.push(GeminiRequestContent {
                        role: "user".into(),
                        parts: vec![GeminiRequestPart {
                            text: None,
                            function_call: None,
                            function_response: Some(GeminiFunctionResponse {
                                name: message
                                    .tool_call_id
                                    .as_ref()
                                    .and_then(|tool_call_id| tool_call_names.get(tool_call_id))
                                    .cloned()
                                    .or_else(|| message.tool_call_id.clone())
                                    .unwrap_or_else(|| "tool_result".into()),
                                response: serde_json::json!({ "content": content_text }),
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

fn parts_from_content(content: &serde_json::Value) -> Vec<GeminiRequestPart> {
    match content {
        serde_json::Value::Null => Vec::new(),
        serde_json::Value::String(text) => {
            if text.is_empty() {
                Vec::new()
            } else {
                vec![GeminiRequestPart {
                    text: Some(text.clone()),
                    function_call: None,
                    function_response: None,
                }]
            }
        }
        serde_json::Value::Array(items) => {
            items.iter().filter_map(part_from_openai_content).collect()
        }
        serde_json::Value::Object(_) => part_from_openai_content(content).into_iter().collect(),
        other => vec![GeminiRequestPart {
            text: Some(other.to_string()),
            function_call: None,
            function_response: None,
        }],
    }
}

fn part_from_openai_content(value: &serde_json::Value) -> Option<GeminiRequestPart> {
    match value {
        serde_json::Value::String(text) => Some(GeminiRequestPart {
            text: Some(text.clone()),
            function_call: None,
            function_response: None,
        }),
        serde_json::Value::Object(map) => match map.get("type").and_then(|value| value.as_str()) {
            Some("text") => map
                .get("text")
                .and_then(|value| value.as_str())
                .map(|text| GeminiRequestPart {
                    text: Some(text.to_string()),
                    function_call: None,
                    function_response: None,
                }),
            Some("image_url") => {
                let url = map
                    .get("image_url")
                    .and_then(|value| value.get("url"))
                    .and_then(|value| value.as_str())
                    .or_else(|| map.get("image_url").and_then(|value| value.as_str()));

                url.map(|url| GeminiRequestPart {
                    text: Some(format!("Image URL: {url}")),
                    function_call: None,
                    function_response: None,
                })
            }
            _ => map
                .get("text")
                .and_then(|value| value.as_str())
                .map(|text| GeminiRequestPart {
                    text: Some(text.to_string()),
                    function_call: None,
                    function_response: None,
                }),
        },
        _ => None,
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

fn convert_stop_sequences(stop: &serde_json::Value) -> Option<Vec<String>> {
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

fn convert_response(response: GeminiResponse, model: &str) -> Result<ChatCompletionResponse> {
    let usage = response
        .usage_metadata
        .map(usage_from_gemini)
        .unwrap_or_default();
    let candidate = response
        .candidates
        .into_iter()
        .next()
        .context("gemini response did not contain any candidates")?;

    let mut texts = Vec::new();
    let mut tool_calls = Vec::new();

    if let Some(content) = candidate.content {
        for part in content.parts {
            if let Some(text) = part.text
                && !text.is_empty()
            {
                texts.push(text);
            }

            if let Some(function_call) = part.function_call {
                let index = tool_calls.len() as i32;
                tool_calls.push(ToolCall {
                    id: format!("call_{index}"),
                    r#type: "function".into(),
                    function: FunctionCall {
                        name: function_call.name,
                        arguments: serialize_arguments(function_call.args),
                    },
                });
            }
        }
    }

    let finish_reason =
        map_gemini_finish_reason(candidate.finish_reason.as_deref(), !tool_calls.is_empty());

    Ok(ChatCompletionResponse {
        id: format!("gemini-{}", unix_timestamp()),
        object: "chat.completion".into(),
        created: unix_timestamp(),
        model: model.to_string(),
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
        usage,
    })
}

fn map_gemini_finish_reason(
    finish_reason: Option<&str>,
    has_tool_calls: bool,
) -> Option<FinishReason> {
    if has_tool_calls {
        return Some(FinishReason::ToolCalls);
    }

    match finish_reason {
        Some("MAX_TOKENS" | "MAX_OUTPUT_TOKENS") => Some(FinishReason::Length),
        Some("SAFETY" | "RECITATION" | "BLOCKLIST") => Some(FinishReason::ContentFilter),
        Some(_) | None => Some(FinishReason::Stop),
    }
}

fn map_gemini_stream_finish_reason(
    finish_reason: Option<&str>,
    has_tool_calls: bool,
) -> Option<FinishReason> {
    finish_reason.and_then(|reason| map_gemini_finish_reason(Some(reason), has_tool_calls))
}

fn usage_from_gemini(usage: GeminiUsageMetadata) -> Usage {
    let total_tokens = if usage.total_token_count > 0 {
        usage.total_token_count
    } else {
        usage.prompt_token_count + usage.candidates_token_count
    };

    Usage {
        prompt_tokens: usage.prompt_token_count,
        completion_tokens: usage.candidates_token_count,
        total_tokens,
        cached_tokens: 0,
        reasoning_tokens: 0,
    }
}

fn parse_gemini_sse_data(event_text: &str) -> Option<String> {
    let data = event_text
        .lines()
        .filter_map(|line| line.trim_end_matches('\r').strip_prefix("data:"))
        .map(|line| line.trim_start().to_string())
        .collect::<Vec<_>>()
        .join("\n");

    (!data.is_empty()).then_some(data)
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

fn parse_function_arguments(arguments: &str) -> serde_json::Value {
    serde_json::from_str(arguments).unwrap_or_else(|_| serde_json::Value::String(arguments.into()))
}

fn serialize_arguments(arguments: serde_json::Value) -> String {
    match arguments {
        serde_json::Value::String(arguments) => arguments,
        other => serde_json::to_string(&other).unwrap_or_else(|_| "{}".into()),
    }
}

fn extract_tool_result_content(content: &serde_json::Value) -> String {
    match content {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn joined_text_value(texts: Vec<String>) -> serde_json::Value {
    if texts.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::Value::String(texts.join(""))
    }
}

fn chunk_with_delta(
    state: &GeminiStreamState,
    delta: Delta,
    finish_reason: Option<FinishReason>,
    usage: Option<Usage>,
) -> ChatCompletionChunk {
    ChatCompletionChunk {
        id: state.id.clone(),
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
mod tests {
    use super::*;

    fn sample_request() -> ChatCompletionRequest {
        serde_json::from_value(serde_json::json!({
            "model": "gemini-2.5-pro",
            "messages": [
                {"role": "system", "content": "You are helpful."},
                {"role": "user", "content": "Hello"}
            ]
        }))
        .unwrap()
    }

    #[test]
    fn build_request_targets_generate_content_endpoint() {
        let client = reqwest::Client::new();
        let adapter = GeminiAdapter;
        let builder = adapter
            .build_request(
                &client,
                "https://generativelanguage.googleapis.com",
                "gem-key",
                &sample_request(),
                "gemini-2.5-pro",
            )
            .unwrap();

        let request = builder.build().unwrap();
        assert_eq!(
            request.url().as_str(),
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-pro:generateContent"
        );
        assert_eq!(request.headers().get("x-goog-api-key").unwrap(), "gem-key");
    }

    #[test]
    fn build_stream_request_targets_stream_generate_content_sse_endpoint() {
        let client = reqwest::Client::new();
        let adapter = GeminiAdapter;
        let mut request = sample_request();
        request.stream = true;

        let builder = adapter
            .build_request(
                &client,
                "https://generativelanguage.googleapis.com",
                "gem-key",
                &request,
                "gemini-2.5-pro",
            )
            .unwrap();

        let request = builder.build().unwrap();
        assert_eq!(
            request.url().as_str(),
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-pro:streamGenerateContent?alt=sse"
        );
        assert_eq!(request.headers().get("x-goog-api-key").unwrap(), "gem-key");
    }

    #[test]
    fn build_request_respects_explicit_v1_base_url() {
        let client = reqwest::Client::new();
        let adapter = GeminiAdapter;
        let builder = adapter
            .build_request(
                &client,
                "https://generativelanguage.googleapis.com/v1",
                "gem-key",
                &sample_request(),
                "gemini-2.5-pro",
            )
            .unwrap();

        let request = builder.build().unwrap();
        assert_eq!(
            request.url().as_str(),
            "https://generativelanguage.googleapis.com/v1/models/gemini-2.5-pro:generateContent"
        );
    }

    #[test]
    fn parse_response_converts_text_and_usage() {
        let adapter = GeminiAdapter;
        let body = Bytes::from(
            serde_json::to_vec(&serde_json::json!({
                "candidates": [{
                    "content": {
                        "parts": [{"text": "Hello from Gemini"}]
                    },
                    "finishReason": "STOP"
                }],
                "usageMetadata": {
                    "promptTokenCount": 4,
                    "candidatesTokenCount": 6,
                    "totalTokenCount": 10
                }
            }))
            .unwrap(),
        );

        let response = adapter.parse_response(body, "gemini-2.5-pro").unwrap();
        assert_eq!(response.model, "gemini-2.5-pro");
        assert_eq!(
            response.choices[0].message.content,
            serde_json::Value::String("Hello from Gemini".into())
        );
        assert_eq!(response.usage.total_tokens, 10);
    }

    #[tokio::test]
    async fn parse_stream_emits_text_and_usage() {
        let adapter = GeminiAdapter;
        let sse_body =
            "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"Hello\"}]},\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":4,\"candidatesTokenCount\":6,\"totalTokenCount\":10}}\n\n";

        let mock_response = http::Response::builder()
            .status(200)
            .body(sse_body.to_string())
            .unwrap();
        let response = reqwest::Response::from(mock_response);

        let chunks: Vec<_> = adapter
            .parse_stream(response, "gemini-2.5-pro")
            .unwrap()
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .filter_map(Result::ok)
            .collect();

        assert_eq!(chunks[0].choices[0].delta.role.as_deref(), Some("assistant"));
        assert_eq!(chunks[1].choices[0].delta.content.as_deref(), Some("Hello"));
        assert_eq!(chunks[2].usage.as_ref().map(|usage| usage.total_tokens), Some(10));
        assert!(matches!(
            chunks[2].choices[0].finish_reason,
            Some(FinishReason::Stop)
        ));
    }

    #[tokio::test]
    async fn parse_stream_emits_function_call_deltas() {
        let adapter = GeminiAdapter;
        let sse_body =
            "data: {\"candidates\":[{\"content\":{\"parts\":[{\"functionCall\":{\"name\":\"get_weather\",\"args\":{\"city\":\"Paris\"}}}]},\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":4,\"candidatesTokenCount\":2,\"totalTokenCount\":6}}\n\n";

        let mock_response = http::Response::builder()
            .status(200)
            .body(sse_body.to_string())
            .unwrap();
        let response = reqwest::Response::from(mock_response);

        let chunks: Vec<_> = adapter
            .parse_stream(response, "gemini-2.5-pro")
            .unwrap()
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .filter_map(Result::ok)
            .collect();

        let tool_calls = chunks[1].choices[0].delta.tool_calls.as_ref().unwrap();
        assert_eq!(
            tool_calls[0].function.as_ref().unwrap().name.as_deref(),
            Some("get_weather")
        );
        assert_eq!(
            tool_calls[0]
                .function
                .as_ref()
                .unwrap()
                .arguments
                .as_deref(),
            Some("{\"city\":\"Paris\"}")
        );
        assert!(matches!(
            chunks[2].choices[0].finish_reason,
            Some(FinishReason::ToolCalls)
        ));
    }

    #[tokio::test]
    async fn parse_stream_keeps_tool_call_finish_reason_across_events() {
        let adapter = GeminiAdapter;
        let sse_body = concat!(
            "data: {\"candidates\":[{\"content\":{\"parts\":[{\"functionCall\":{\"name\":\"get_weather\",\"args\":{\"city\":\"Paris\"}}}]}}]}\n\n",
            "data: {\"candidates\":[{\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":4,\"candidatesTokenCount\":2,\"totalTokenCount\":6}}\n\n"
        );

        let mock_response = http::Response::builder()
            .status(200)
            .body(sse_body.to_string())
            .unwrap();
        let response = reqwest::Response::from(mock_response);

        let chunks: Vec<_> = adapter
            .parse_stream(response, "gemini-2.5-pro")
            .unwrap()
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .filter_map(Result::ok)
            .collect();

        let final_chunk = chunks
            .iter()
            .rfind(|chunk| chunk.choices[0].finish_reason.is_some())
            .expect("expected terminal chunk");
        assert!(matches!(
            final_chunk.choices[0].finish_reason,
            Some(FinishReason::ToolCalls)
        ));
    }

    #[tokio::test]
    async fn parse_stream_does_not_emit_terminal_chunk_before_finish_reason() {
        let adapter = GeminiAdapter;
        let sse_body = concat!(
            "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"Hel\"}]}}]}\n\n",
            "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"lo\"}]},\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":4,\"candidatesTokenCount\":2,\"totalTokenCount\":6}}\n\n"
        );

        let mock_response = http::Response::builder()
            .status(200)
            .body(sse_body.to_string())
            .unwrap();
        let response = reqwest::Response::from(mock_response);

        let chunks: Vec<_> = adapter
            .parse_stream(response, "gemini-2.5-pro")
            .unwrap()
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .filter_map(Result::ok)
            .collect();

        assert_eq!(
            chunks
                .iter()
                .filter(|chunk| chunk.choices[0].finish_reason.is_some())
                .count(),
            1
        );
        let final_chunk = chunks
            .iter()
            .rfind(|chunk| chunk.choices[0].finish_reason.is_some())
            .expect("expected final terminal chunk");
        assert!(matches!(
            final_chunk.choices[0].finish_reason,
            Some(FinishReason::Stop)
        ));
    }

    #[tokio::test]
    async fn parse_stream_returns_error_for_gemini_error_event() {
        let adapter = GeminiAdapter;
        let sse_body = concat!(
            "event: error\n",
            "data: {\"error\":{\"status\":\"INVALID_ARGUMENT\",\"message\":\"bad tool schema\"}}\n\n"
        );

        let mock_response = http::Response::builder()
            .status(200)
            .body(sse_body.to_string())
            .unwrap();
        let response = reqwest::Response::from(mock_response);

        let results = adapter
            .parse_stream(response, "gemini-2.5-pro")
            .unwrap()
            .collect::<Vec<_>>()
            .await;

        let error = results
            .into_iter()
            .find_map(Result::err)
            .expect("expected gemini stream error");
        let stream_error = error
            .downcast_ref::<super::super::ProviderStreamError>()
            .expect("expected provider stream error");
        assert_eq!(stream_error.info.kind, ProviderErrorKind::InvalidRequest);
        assert_eq!(stream_error.info.code, "INVALID_ARGUMENT");
        assert_eq!(stream_error.info.message, "bad tool schema");
        assert!(error.to_string().contains("gemini stream error [INVALID_ARGUMENT]"));
        assert!(error.chain().any(|cause| cause.to_string().contains("bad tool schema")));
    }

    #[test]
    fn convert_contents_uses_function_name_for_tool_response() {
        let messages = vec![
            Message {
                role: "assistant".into(),
                content: serde_json::Value::Null,
                name: None,
                tool_calls: Some(vec![ToolCall {
                    id: "call_123".into(),
                    r#type: "function".into(),
                    function: FunctionCall {
                        name: "get_weather".into(),
                        arguments: "{\"city\":\"Paris\"}".into(),
                    },
                }]),
                tool_call_id: None,
            },
            Message {
                role: "tool".into(),
                content: serde_json::Value::String("sunny".into()),
                name: None,
                tool_calls: None,
                tool_call_id: Some("call_123".into()),
            },
        ];

        let contents = convert_contents(&messages);
        let tool_response = contents[1].parts[0]
            .function_response
            .as_ref()
            .expect("expected function response");
        assert_eq!(tool_response.name, "get_weather");
    }

    #[test]
    fn parse_error_treats_failed_precondition_as_account_level_api_error() {
        let info = GeminiAdapter.parse_error(
            StatusCode::BAD_REQUEST,
            &HeaderMap::new(),
            br#"{"error":{"status":"FAILED_PRECONDITION","message":"project is not configured"}}"#,
        );

        assert_eq!(info.kind, ProviderErrorKind::Api);
        assert_eq!(info.code, "FAILED_PRECONDITION");
        assert_eq!(info.message, "project is not configured");
    }
}
