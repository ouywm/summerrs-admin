//! OpenAI Responses API (`/v1/responses`) adapter。

use std::collections::HashMap;
use std::future::Future;

use bytes::Bytes;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue};
use serde_json::Value;

use crate::adapter::{
    Adapter, AdapterKind, AuthStrategy, Capabilities, CostProfile, ServiceType, WebRequestData,
};
use crate::error::{AdapterError, AdapterResult};
use crate::resolver::{Endpoint, ServiceTarget};
use crate::types::ingress_wire::openai_responses::{
    OpenAIResponsesContextManagement, OpenAIResponsesConversation, OpenAIResponsesFunctionCallItem,
    OpenAIResponsesFunctionCallOutput, OpenAIResponsesFunctionCallOutputItem, OpenAIResponsesInput,
    OpenAIResponsesInputContentPart, OpenAIResponsesInputItem, OpenAIResponsesMessageContent,
    OpenAIResponsesMessageItem, OpenAIResponsesOutputContentPart, OpenAIResponsesOutputItem,
    OpenAIResponsesPrompt, OpenAIResponsesReasoning, OpenAIResponsesReasoningEffort,
    OpenAIResponsesReasoningSummary, OpenAIResponsesRequest, OpenAIResponsesResponse,
    OpenAIResponsesStreamEvent, OpenAIResponsesStreamOptions, OpenAIResponsesTextConfig,
    OpenAIResponsesTextFormat, OpenAIResponsesTool, OpenAIResponsesToolChoice,
    OpenAIResponsesToolChoiceFunction, OpenAIResponsesUsage,
};
use crate::types::{
    ChatChoice, ChatMessage, ChatRequest, ChatResponse, ChatStreamEvent, ContentPart, FinishReason,
    MessageContent, ModelList, ReasoningEffort, ResponseFormat, Role, ServiceTier, StreamEnd,
    StreamError, ToolCall, ToolCallDelta, ToolChoice as CanonicalToolChoice, Usage, Verbosity,
};

/// OpenAI 官方 `/v1/responses` 协议。
pub struct OpenAIRespAdapter;

impl OpenAIRespAdapter {
    pub const API_KEY_DEFAULT_ENV_NAME: &'static str = "OPENAI_API_KEY";
    const BASE_URL: &'static str = "https://api.openai.com/v1/";
}

impl Adapter for OpenAIRespAdapter {
    const KIND: AdapterKind = AdapterKind::OpenAIResp;
    const DEFAULT_API_KEY_ENV_NAME: Option<&'static str> = Some(Self::API_KEY_DEFAULT_ENV_NAME);

    fn default_endpoint() -> Option<Endpoint> {
        Some(Endpoint::from_static(Self::BASE_URL))
    }

    fn capabilities() -> Capabilities {
        let mut caps = Capabilities::openai_like();
        caps.reasoning = true;
        caps
    }

    fn auth_strategy() -> AuthStrategy {
        AuthStrategy::Bearer
    }

    fn cost_profile() -> CostProfile {
        CostProfile::openai_like()
    }

    fn build_chat_request(
        target: &ServiceTarget,
        service: ServiceType,
        req: &ChatRequest,
    ) -> AdapterResult<WebRequestData> {
        Self::validate_chat_request(req)?;

        let stream = match service {
            ServiceType::Responses => false,
            ServiceType::ResponsesStream => true,
            ServiceType::Chat | ServiceType::ChatStream => {
                return Err(AdapterError::Unsupported {
                    adapter: Self::KIND.as_str(),
                    feature: "chat",
                });
            }
        };

        let wire = canonical_to_responses_request(target, stream, req)?;
        let payload = serde_json::to_value(&wire).map_err(AdapterError::SerializeRequest)?;
        Ok(WebRequestData {
            url: build_responses_url(target.endpoint.trimmed()),
            headers: build_headers(target)?,
            payload,
        })
    }

    fn parse_chat_response(_target: &ServiceTarget, body: Bytes) -> AdapterResult<ChatResponse> {
        let resp: OpenAIResponsesResponse =
            serde_json::from_slice(&body).map_err(AdapterError::DeserializeResponse)?;
        Ok(responses_response_to_chat_response(resp))
    }

    fn parse_chat_stream_event(
        _target: &ServiceTarget,
        raw: &str,
    ) -> AdapterResult<Vec<ChatStreamEvent>> {
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed == "[DONE]" {
            return Ok(Vec::new());
        }

        let event: OpenAIResponsesStreamEvent =
            serde_json::from_str(trimmed).map_err(AdapterError::DeserializeResponse)?;
        Ok(match event {
            OpenAIResponsesStreamEvent::ResponseCreated { response, .. }
            | OpenAIResponsesStreamEvent::ResponseInProgress { response, .. } => {
                vec![ChatStreamEvent::Start {
                    adapter: Self::KIND.as_lower_str().to_string(),
                    model: response.model,
                }]
            }
            OpenAIResponsesStreamEvent::OutputItemAdded {
                output_index, item, ..
            } => match item {
                OpenAIResponsesOutputItem::FunctionCall(fc) => {
                    vec![ChatStreamEvent::ToolCallDelta(ToolCallDelta {
                        index: output_index as i32,
                        id: Some(fc.call_id),
                        name: Some(fc.name),
                        arguments_delta: None,
                    })]
                }
                _ => Vec::new(),
            },
            OpenAIResponsesStreamEvent::OutputTextDelta { delta, .. } => {
                vec![ChatStreamEvent::TextDelta { text: delta }]
            }
            OpenAIResponsesStreamEvent::FunctionCallArgumentsDelta {
                output_index,
                delta,
                ..
            } => vec![ChatStreamEvent::ToolCallDelta(ToolCallDelta {
                index: output_index as i32,
                id: None,
                name: None,
                arguments_delta: Some(delta),
            })],
            OpenAIResponsesStreamEvent::ResponseCompleted { response, .. } => {
                let usage = response.usage.as_ref().map(responses_usage_to_canonical);
                let finish_reason = map_responses_finish_reason(&response.status, &response.output);
                vec![ChatStreamEvent::End(StreamEnd {
                    finish_reason,
                    usage,
                })]
            }
            OpenAIResponsesStreamEvent::ResponseFailed { response, .. } => {
                vec![ChatStreamEvent::Error(StreamError {
                    message: response
                        .error
                        .as_ref()
                        .and_then(|e| e.message.clone())
                        .unwrap_or_else(|| "responses stream failed".to_string()),
                    kind: response.error.as_ref().and_then(|e| e.code.clone()),
                })]
            }
            OpenAIResponsesStreamEvent::Error { code, message, .. } => {
                vec![ChatStreamEvent::Error(StreamError {
                    message,
                    kind: code,
                })]
            }
            OpenAIResponsesStreamEvent::ContentPartAdded { .. }
            | OpenAIResponsesStreamEvent::OutputTextDone { .. }
            | OpenAIResponsesStreamEvent::ContentPartDone { .. }
            | OpenAIResponsesStreamEvent::OutputItemDone { .. }
            | OpenAIResponsesStreamEvent::FunctionCallArgumentsDone { .. }
            | OpenAIResponsesStreamEvent::ResponseIncomplete { .. } => Vec::new(),
        })
    }

    fn fetch_model_names(
        target: &ServiceTarget,
        http: &reqwest::Client,
    ) -> impl Future<Output = AdapterResult<Vec<String>>> + Send {
        let target = target.clone();
        let http = http.clone();
        async move {
            let response = http
                .get(build_models_url(target.endpoint.trimmed()))
                .headers(build_headers(&target)?)
                .send()
                .await
                .map_err(|e| AdapterError::Network(e.to_string()))?;

            if !response.status().is_success() {
                let status = response.status().as_u16();
                let body = response.bytes().await.unwrap_or_default();
                return Err(AdapterError::UpstreamStatus {
                    status,
                    message: String::from_utf8_lossy(&body).to_string(),
                });
            }

            let body = response
                .bytes()
                .await
                .map_err(|e| AdapterError::Network(e.to_string()))?;
            let list: ModelList =
                serde_json::from_slice(&body).map_err(AdapterError::DeserializeResponse)?;
            Ok(list.data.into_iter().map(|m| m.id).collect())
        }
    }
}

fn canonical_to_responses_request(
    target: &ServiceTarget,
    stream: bool,
    req: &ChatRequest,
) -> AdapterResult<OpenAIResponsesRequest> {
    let instructions = extract_instructions(req);
    let input_items = canonical_messages_to_input_items(&req.messages)?;
    let input = if input_items.len() == 1 {
        if let OpenAIResponsesInputItem::Message(message) = &input_items[0] {
            if message.role == "user" {
                if let OpenAIResponsesMessageContent::Text(text) = &message.content {
                    OpenAIResponsesInput::Text(text.clone())
                } else {
                    OpenAIResponsesInput::Items(input_items)
                }
            } else {
                OpenAIResponsesInput::Items(input_items)
            }
        } else {
            OpenAIResponsesInput::Items(input_items)
        }
    } else {
        OpenAIResponsesInput::Items(input_items)
    };

    let tools = req
        .tools
        .clone()
        .unwrap_or_default()
        .into_iter()
        .map(OpenAIResponsesTool::from)
        .collect();

    let reasoning_summary = req
        .responses_extras
        .as_ref()
        .and_then(|e| e.reasoning_summary.clone())
        .and_then(|s| {
            serde_json::from_value::<OpenAIResponsesReasoningSummary>(serde_json::json!(s)).ok()
        });
    let reasoning = if req.reasoning_effort.is_some() || reasoning_summary.is_some() {
        Some(OpenAIResponsesReasoning {
            effort: req
                .reasoning_effort
                .as_ref()
                .map(reasoning_effort_to_openai),
            summary: reasoning_summary,
            generate_summary: None,
        })
    } else {
        None
    };

    let mut extra = req
        .extra
        .clone()
        .into_iter()
        .collect::<HashMap<String, Value>>();

    let background = take_extra_typed::<bool>(&mut extra, "background");
    let context_management =
        take_extra_typed::<Vec<OpenAIResponsesContextManagement>>(&mut extra, "context_management")
            .unwrap_or_default();
    let conversation = take_extra_typed::<OpenAIResponsesConversation>(&mut extra, "conversation");
    let include = take_extra_typed::<Vec<String>>(&mut extra, "include").unwrap_or_default();
    let prompt = take_extra_typed::<OpenAIResponsesPrompt>(&mut extra, "prompt");
    let prompt_cache_key = take_extra_typed::<String>(&mut extra, "prompt_cache_key");
    let prompt_cache_retention = take_extra_typed::<String>(&mut extra, "prompt_cache_retention");
    let safety_identifier = take_extra_typed::<String>(&mut extra, "safety_identifier");
    let stream_options =
        take_extra_typed::<OpenAIResponsesStreamOptions>(&mut extra, "stream_options");
    let truncation = take_extra_typed::<String>(&mut extra, "truncation");
    let max_tool_calls = take_extra_typed::<i64>(&mut extra, "max_tool_calls");
    let text = build_text_config(req.response_format.as_ref(), req.verbosity.as_ref());

    Ok(OpenAIResponsesRequest {
        model: target.actual_model().to_string(),
        input,
        instructions,
        tools,
        tool_choice: req.tool_choice.as_ref().map(|tc| match tc {
            CanonicalToolChoice::Mode(mode) => OpenAIResponsesToolChoice::Simple(mode.clone()),
            CanonicalToolChoice::Named(value) => {
                // 尝试解析为具体的 tool_choice 类型
                if let Some(name) = value.get("name").and_then(|n| n.as_str()) {
                    if value.get("type").and_then(|t| t.as_str()) == Some("function") {
                        OpenAIResponsesToolChoice::Function(OpenAIResponsesToolChoiceFunction {
                            name: name.to_string(),
                            type_: "function".to_string(),
                        })
                    } else {
                        // 其他复杂类型，用 serde 反序列化
                        serde_json::from_value(value.clone()).unwrap_or_else(|_| {
                            OpenAIResponsesToolChoice::Simple("auto".to_string())
                        })
                    }
                } else {
                    serde_json::from_value(value.clone())
                        .unwrap_or_else(|_| OpenAIResponsesToolChoice::Simple("auto".to_string()))
                }
            }
        }),
        temperature: req.temperature,
        top_p: req.top_p,
        max_output_tokens: req.max_completion_tokens.or(req.max_tokens),
        stream,
        stream_options,
        parallel_tool_calls: req.parallel_tool_calls,
        previous_response_id: req
            .responses_extras
            .as_ref()
            .and_then(|e| e.previous_response_id.clone()),
        reasoning,
        store: req.store,
        user: req.user.clone(),
        metadata: req.metadata.clone(),
        background,
        context_management,
        conversation,
        include,
        prompt,
        prompt_cache_key,
        prompt_cache_retention,
        safety_identifier,
        service_tier: req.service_tier.as_ref().map(service_tier_to_wire),
        text,
        top_logprobs: req.top_logprobs,
        truncation,
        max_tool_calls,
        extra,
    })
}

fn extract_instructions(req: &ChatRequest) -> Option<String> {
    if let Some(instructions) = req
        .responses_extras
        .as_ref()
        .and_then(|e| e.instructions.clone())
        .filter(|s| !s.is_empty())
    {
        return Some(instructions);
    }

    let merged = req
        .messages
        .iter()
        .filter(|m| matches!(m.role, Role::System | Role::Developer))
        .filter_map(render_message_text)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");

    (!merged.is_empty()).then_some(merged)
}

fn canonical_messages_to_input_items(
    messages: &[ChatMessage],
) -> AdapterResult<Vec<OpenAIResponsesInputItem>> {
    let mut items = Vec::new();
    for message in messages {
        if matches!(message.role, Role::System | Role::Developer) {
            continue;
        }

        match message.role {
            Role::Tool => items.push(OpenAIResponsesInputItem::FunctionCallOutput(
                OpenAIResponsesFunctionCallOutputItem {
                    id: None,
                    call_id: message.tool_call_id.clone().unwrap_or_default(),
                    output: OpenAIResponsesFunctionCallOutput::Text(
                        render_message_text(message).unwrap_or_default(),
                    ),
                    status: None,
                },
            )),
            Role::Assistant => {
                if message.content.is_some() || message.refusal.is_some() {
                    items.push(OpenAIResponsesInputItem::Message(chat_message_to_item(
                        message,
                    )?));
                }
                if let Some(tool_calls) = &message.tool_calls {
                    for call in tool_calls {
                        items.push(OpenAIResponsesInputItem::FunctionCall(
                            OpenAIResponsesFunctionCallItem {
                                id: None,
                                call_id: call.id.clone(),
                                name: call.function.name.clone(),
                                arguments: call.function.arguments.clone(),
                                status: None,
                                namespace: None,
                            },
                        ));
                    }
                }
            }
            Role::User => items.push(OpenAIResponsesInputItem::Message(chat_message_to_item(
                message,
            )?)),
            Role::System | Role::Developer => {}
        }
    }
    Ok(items)
}

fn chat_message_to_item(message: &ChatMessage) -> AdapterResult<OpenAIResponsesMessageItem> {
    Ok(OpenAIResponsesMessageItem {
        id: None,
        role: role_to_wire(message.role).to_string(),
        status: None,
        content: chat_message_content_to_wire(message),
        phase: None,
    })
}

fn chat_message_content_to_wire(message: &ChatMessage) -> OpenAIResponsesMessageContent {
    if let Some(refusal) = &message.refusal {
        return OpenAIResponsesMessageContent::Text(refusal.clone());
    }

    match message.content.clone() {
        Some(MessageContent::Text(text)) => OpenAIResponsesMessageContent::Text(text),
        Some(MessageContent::Parts(parts)) => OpenAIResponsesMessageContent::Parts(
            parts.into_iter().map(content_part_to_wire).collect(),
        ),
        None => OpenAIResponsesMessageContent::Text(String::new()),
    }
}

fn content_part_to_wire(part: ContentPart) -> OpenAIResponsesInputContentPart {
    match part {
        ContentPart::Text { text } => OpenAIResponsesInputContentPart::InputText { text },
        ContentPart::ImageUrl { image_url } => OpenAIResponsesInputContentPart::InputImage {
            image_url: Some(image_url.url),
            file_id: None,
            detail: image_url.detail,
        },
        ContentPart::InputAudio { input_audio } => OpenAIResponsesInputContentPart::InputText {
            text: format!("[input_audio format={}]", input_audio.format),
        },
    }
}

fn render_message_text(message: &ChatMessage) -> Option<String> {
    if let Some(refusal) = &message.refusal {
        return Some(refusal.clone());
    }

    match message.content.as_ref()? {
        MessageContent::Text(text) => Some(text.clone()),
        MessageContent::Parts(parts) => {
            let buf = parts
                .iter()
                .filter_map(|part| match part {
                    ContentPart::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n");
            Some(buf)
        }
    }
}

fn role_to_wire(role: Role) -> &'static str {
    match role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::Tool => "tool",
        Role::Developer => "developer",
    }
}

fn reasoning_effort_to_openai(effort: &ReasoningEffort) -> OpenAIResponsesReasoningEffort {
    match effort {
        ReasoningEffort::None => OpenAIResponsesReasoningEffort::None,
        ReasoningEffort::Minimal => OpenAIResponsesReasoningEffort::Minimal,
        ReasoningEffort::Low => OpenAIResponsesReasoningEffort::Low,
        ReasoningEffort::Medium => OpenAIResponsesReasoningEffort::Medium,
        ReasoningEffort::High => OpenAIResponsesReasoningEffort::High,
        ReasoningEffort::XHigh | ReasoningEffort::Max => OpenAIResponsesReasoningEffort::High,
        ReasoningEffort::Budget(tokens) => match *tokens {
            0 => OpenAIResponsesReasoningEffort::None,
            1..=256 => OpenAIResponsesReasoningEffort::Minimal,
            257..=1024 => OpenAIResponsesReasoningEffort::Low,
            1025..=4096 => OpenAIResponsesReasoningEffort::Medium,
            _ => OpenAIResponsesReasoningEffort::High,
        },
    }
}

fn build_text_config(
    response_format: Option<&ResponseFormat>,
    verbosity: Option<&Verbosity>,
) -> Option<OpenAIResponsesTextConfig> {
    let format = response_format.map(|format| match format {
        ResponseFormat::Text => OpenAIResponsesTextFormat::Text,
        ResponseFormat::JsonObject => OpenAIResponsesTextFormat::JsonObject,
        ResponseFormat::JsonSchema { json_schema } => OpenAIResponsesTextFormat::JsonSchema {
            name: json_schema.name.clone(),
            schema: json_schema.schema.clone(),
            description: json_schema.description.clone(),
            strict: json_schema.strict,
        },
    });
    let verbosity = verbosity.map(|verbosity| match verbosity {
        Verbosity::Low => "low".to_string(),
        Verbosity::Medium => "medium".to_string(),
        Verbosity::High => "high".to_string(),
    });
    if format.is_none() && verbosity.is_none() {
        None
    } else {
        Some(OpenAIResponsesTextConfig { format, verbosity })
    }
}

fn service_tier_to_wire(tier: &ServiceTier) -> String {
    match tier {
        ServiceTier::Auto => "auto",
        ServiceTier::Default => "default",
        ServiceTier::Flex => "flex",
        ServiceTier::Priority => "priority",
        ServiceTier::Scale => "scale",
    }
    .to_string()
}

fn take_extra_typed<T: serde::de::DeserializeOwned>(
    extra: &mut HashMap<String, Value>,
    key: &str,
) -> Option<T> {
    extra
        .remove(key)
        .and_then(|value| serde_json::from_value(value).ok())
}

fn responses_response_to_chat_response(resp: OpenAIResponsesResponse) -> ChatResponse {
    let usage = resp
        .usage
        .as_ref()
        .map(responses_usage_to_canonical)
        .unwrap_or_default();
    let finish_reason = map_responses_finish_reason(&resp.status, &resp.output);

    let mut text_buf = String::new();
    let mut refusal: Option<String> = None;
    let mut tool_calls = Vec::new();

    for item in resp.output {
        match item {
            OpenAIResponsesOutputItem::Message(message) => {
                for part in message.content {
                    match part {
                        OpenAIResponsesOutputContentPart::OutputText { text, .. } => {
                            text_buf.push_str(&text);
                        }
                        OpenAIResponsesOutputContentPart::Refusal { refusal: value } => {
                            if refusal.is_none() {
                                refusal = Some(value);
                            }
                        }
                        OpenAIResponsesOutputContentPart::Unknown => {}
                    }
                }
            }
            OpenAIResponsesOutputItem::FunctionCall(call) => {
                tool_calls.push(ToolCall {
                    id: call.call_id,
                    kind: "function".to_string(),
                    function: crate::types::ToolCallFunction {
                        name: call.name,
                        arguments: call.arguments,
                    },
                    thought_signatures: None,
                });
            }
            // 其他 output item 类型暂时忽略
            _ => {}
        }
    }

    let message = ChatMessage {
        role: Role::Assistant,
        content: (!text_buf.is_empty()).then_some(MessageContent::Text(text_buf)),
        reasoning_content: None,
        refusal,
        name: None,
        tool_calls: (!tool_calls.is_empty()).then_some(tool_calls),
        tool_call_id: None,
        audio: None,
        options: None,
    };

    ChatResponse {
        id: resp.id,
        object: "chat.completion".to_string(),
        created: resp.created_at,
        model: resp.model,
        choices: vec![ChatChoice {
            index: 0,
            message,
            logprobs: None,
            finish_reason,
        }],
        usage,
        system_fingerprint: None,
        service_tier: resp.service_tier.as_deref().and_then(|tier| match tier {
            "auto" => Some(ServiceTier::Auto),
            "default" => Some(ServiceTier::Default),
            "flex" => Some(ServiceTier::Flex),
            "priority" => Some(ServiceTier::Priority),
            "scale" => Some(ServiceTier::Scale),
            _ => None,
        }),
    }
}

fn responses_usage_to_canonical(usage: &OpenAIResponsesUsage) -> Usage {
    Usage {
        prompt_tokens: usage.input_tokens,
        completion_tokens: usage.output_tokens,
        total_tokens: usage.total_tokens,
        ..Default::default()
    }
}

fn map_responses_finish_reason(
    status: &str,
    output: &[OpenAIResponsesOutputItem],
) -> Option<FinishReason> {
    match status {
        "completed" => {
            if output
                .iter()
                .any(|item| matches!(item, OpenAIResponsesOutputItem::FunctionCall(_)))
            {
                Some(FinishReason::ToolCalls)
            } else {
                Some(FinishReason::Stop)
            }
        }
        "incomplete" => Some(FinishReason::Length),
        "failed" | "cancelled" => Some(FinishReason::ContentFilter),
        _ => None,
    }
}

fn build_responses_url(base: &str) -> String {
    let base = base.trim_end_matches('/');
    if base.ends_with("/responses") {
        return base.to_string();
    }
    if let Some(prefix) = base.strip_suffix("/chat/completions") {
        return format!("{prefix}/responses");
    }
    if base.ends_with("/v1") || base.contains("/v1/") {
        format!("{base}/responses")
    } else {
        format!("{base}/v1/responses")
    }
}

fn build_models_url(base: &str) -> String {
    let base = base.trim_end_matches('/');
    if base.ends_with("/models") {
        return base.to_string();
    }
    if base.ends_with("/v1") || base.contains("/v1/") {
        format!("{base}/models")
    } else {
        format!("{base}/v1/models")
    }
}

fn build_headers(target: &ServiceTarget) -> AdapterResult<HeaderMap> {
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

    if let Some(key) = target.auth.resolve()? {
        let auth_value = HeaderValue::from_str(&format!("Bearer {key}"))
            .map_err(|error| AdapterError::InvalidHeader(error.to_string()))?;
        headers.insert(AUTHORIZATION, auth_value);
    }

    for (name, value) in &target.extra_headers {
        let name = HeaderName::try_from(name.as_str())
            .map_err(|error| AdapterError::InvalidHeader(error.to_string()))?;
        let value = HeaderValue::from_str(value.as_str())
            .map_err(|error| AdapterError::InvalidHeader(error.to_string()))?;
        headers.insert(name, value);
    }
    Ok(headers)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ResponsesExtras, Tool, ToolFunction};

    fn target() -> ServiceTarget {
        ServiceTarget::bearer(
            AdapterKind::OpenAIResp,
            "https://api.openai.com/v1",
            "sk-test",
            "gpt-5",
        )
    }

    #[test]
    fn build_chat_request_maps_messages_to_responses_input_items() {
        let mut assistant = ChatMessage::assistant("thinking");
        assistant.tool_calls = Some(vec![ToolCall {
            id: "call_1".into(),
            kind: "function".into(),
            function: crate::types::ToolCallFunction {
                name: "get_weather".into(),
                arguments: "{}".into(),
            },
            thought_signatures: None,
        }]);
        let req = ChatRequest::new(
            "alias-model",
            vec![
                ChatMessage::system("be concise"),
                ChatMessage::user("hello"),
                assistant,
                ChatMessage::tool_response("call_1", "sunny"),
            ],
        );

        let wire =
            OpenAIRespAdapter::build_chat_request(&target(), ServiceType::Responses, &req).unwrap();
        assert_eq!(wire.url, "https://api.openai.com/v1/responses");
        assert_eq!(wire.payload["instructions"], "be concise");
        let items = wire.payload["input"].as_array().unwrap();
        assert_eq!(items.len(), 4);
        assert_eq!(items[0]["type"], "message");
        assert_eq!(items[0]["role"], "user");
        assert_eq!(items[1]["type"], "message");
        assert_eq!(items[1]["role"], "assistant");
        assert_eq!(items[2]["type"], "function_call");
        assert_eq!(items[2]["call_id"], "call_1");
        assert_eq!(items[3]["type"], "function_call_output");
        assert_eq!(items[3]["output"], "sunny");
    }

    #[test]
    fn build_chat_request_preserves_builtin_tool_and_previous_response_id() {
        let mut req = ChatRequest::new("alias-model", vec![ChatMessage::user("search")]);
        let mut extra = serde_json::Map::new();
        extra.insert("search_context_size".into(), serde_json::json!("medium"));
        req.tools = Some(vec![Tool::builtin("web_search_preview", extra)]);
        req.responses_extras = Some(ResponsesExtras {
            previous_response_id: Some("resp_prev".into()),
            reasoning_summary: Some("auto".into()),
            instructions: None,
        });
        req.reasoning_effort = Some(ReasoningEffort::Budget(5000));

        let wire =
            OpenAIRespAdapter::build_chat_request(&target(), ServiceType::Responses, &req).unwrap();
        let tools = wire.payload["tools"].as_array().unwrap();
        assert_eq!(tools[0]["type"], "web_search_preview");
        assert_eq!(tools[0]["search_context_size"], "medium");
        assert_eq!(wire.payload["previous_response_id"], "resp_prev");
        assert_eq!(wire.payload["reasoning"]["summary"], "auto");
        assert_eq!(wire.payload["reasoning"]["effort"], "high");
    }

    #[test]
    fn build_chat_request_maps_effective_responses_fields() {
        let mut req = ChatRequest::new("alias-model", vec![ChatMessage::user("hello")]);
        req.service_tier = Some(crate::types::ServiceTier::Flex);
        req.top_logprobs = Some(4);
        req.verbosity = Some(crate::types::Verbosity::High);
        req.response_format = Some(crate::types::ResponseFormat::JsonObject);
        req.extra
            .insert("background".into(), serde_json::json!(true));
        req.extra
            .insert("prompt_cache_key".into(), serde_json::json!("cache-key"));

        let wire =
            OpenAIRespAdapter::build_chat_request(&target(), ServiceType::Responses, &req).unwrap();
        assert_eq!(wire.payload["service_tier"], "flex");
        assert_eq!(wire.payload["top_logprobs"], 4);
        assert_eq!(wire.payload["background"], true);
        assert_eq!(wire.payload["prompt_cache_key"], "cache-key");
        assert_eq!(wire.payload["text"]["verbosity"], "high");
        assert_eq!(wire.payload["text"]["format"]["type"], "json_object");
    }

    #[test]
    fn openai_resp_adapter_rejects_chat_service_type() {
        let req = ChatRequest::new("alias", vec![ChatMessage::user("hi")]);
        let err =
            OpenAIRespAdapter::build_chat_request(&target(), ServiceType::Chat, &req).unwrap_err();
        assert!(matches!(
            err,
            AdapterError::Unsupported {
                feature: "chat",
                ..
            }
        ));
    }

    #[test]
    fn parse_chat_response_maps_completed_tool_calls() {
        let body = Bytes::from_static(
            br#"{
                "id":"resp_1",
                "object":"response",
                "created_at":1700000000,
                "model":"gpt-5",
                "status":"completed",
                "output":[
                    {"type":"function_call","id":"fc_1","call_id":"call_1","name":"get_weather","arguments":"{}","status":"completed"}
                ],
                "usage":{"input_tokens":3,"output_tokens":2,"total_tokens":5}
            }"#,
        );
        let resp = OpenAIRespAdapter::parse_chat_response(&target(), body).unwrap();
        assert_eq!(resp.usage.prompt_tokens, 3);
        assert_eq!(resp.usage.completion_tokens, 2);
        assert_eq!(resp.choices[0].finish_reason, Some(FinishReason::ToolCalls));
        let tool_calls = resp.choices[0].message.tool_calls.as_ref().unwrap();
        assert_eq!(tool_calls[0].id, "call_1");
        assert_eq!(tool_calls[0].function.name, "get_weather");
    }

    #[test]
    fn parse_chat_stream_event_maps_text_delta() {
        let events = OpenAIRespAdapter::parse_chat_stream_event(
            &target(),
            r#"{"type":"response.output_text.delta","item_id":"msg_1","output_index":0,"content_index":0,"delta":"hello","sequence_number":1}"#,
        )
        .unwrap();
        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            ChatStreamEvent::TextDelta { text } if text == "hello"
        ));
    }

    #[test]
    fn parse_chat_stream_event_maps_function_call_lifecycle() {
        let added = OpenAIRespAdapter::parse_chat_stream_event(
            &target(),
            r#"{"type":"response.output_item.added","output_index":2,"item":{"type":"function_call","id":"fc_1","call_id":"call_1","name":"weather","arguments":"","status":"in_progress"},"sequence_number":2}"#,
        )
        .unwrap();
        assert!(matches!(
            &added[0],
            ChatStreamEvent::ToolCallDelta(ToolCallDelta {
                index: 2,
                id: Some(id),
                name: Some(name),
                arguments_delta: None,
            }) if id == "call_1" && name == "weather"
        ));

        let delta = OpenAIRespAdapter::parse_chat_stream_event(
            &target(),
            r#"{"type":"response.function_call_arguments.delta","item_id":"fc_1","output_index":2,"delta":"{\"city\":","sequence_number":3}"#,
        )
        .unwrap();
        assert!(matches!(
            &delta[0],
            ChatStreamEvent::ToolCallDelta(ToolCallDelta {
                index: 2,
                arguments_delta: Some(args),
                ..
            }) if args == "{\"city\":"
        ));

        let completed = OpenAIRespAdapter::parse_chat_stream_event(
            &target(),
            r#"{"type":"response.completed","response":{"id":"resp_1","object":"response","created_at":1,"model":"gpt-5","status":"completed","output":[{"type":"function_call","id":"fc_1","call_id":"call_1","name":"weather","arguments":"{}","status":"completed"}],"usage":{"input_tokens":1,"output_tokens":2,"total_tokens":3}},"sequence_number":4}"#,
        )
        .unwrap();
        assert!(matches!(
            &completed[0],
            ChatStreamEvent::End(StreamEnd {
                finish_reason: Some(FinishReason::ToolCalls),
                usage: Some(usage),
            }) if usage.total_tokens == 3
        ));
    }

    #[test]
    fn canonical_tool_to_responses_tool_preserves_function_strict() {
        let tool = Tool::function(ToolFunction {
            name: "weather".into(),
            description: Some("Get weather".into()),
            parameters: Some(serde_json::json!({"type":"object"})),
        })
        .with_strict(true);
        let wire = OpenAIResponsesTool::from(tool);
        let value = serde_json::to_value(wire).unwrap();
        assert_eq!(value["type"], "function");
        assert_eq!(value["strict"], true);
        assert_eq!(value["name"], "weather");
    }
}
