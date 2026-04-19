//! Anthropic Messages API adapter。
//!
//! canonical ↔ Anthropic wire 双向转换，挂 `AdapterDispatcher` 用。
//!
//! # 鉴权
//!
//! - header `x-api-key: {key}`
//! - header `anthropic-version: 2023-06-01`
//!
//! # prompt cache 计费
//!
//! Anthropic 独有 `cache_creation_input_tokens`（写入 1.25x）和
//! `cache_read_input_tokens`（读 0.1x）。响应解析时：
//! - `cache_read_input_tokens` → `Usage.prompt_tokens_details.cached_tokens`
//! - `cache_creation_*` 目前透传到 canonical `extra`（canonical
//!   `PromptTokensDetails` 暂无对应字段）

use bytes::Bytes;
use reqwest::header::{CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue};
use std::future::Future;

use crate::adapter::{
    Adapter, AdapterKind, AuthStrategy, Capabilities, CostProfile, ServiceType, WebRequestData,
};
use crate::error::{AdapterError, AdapterResult};
use crate::resolver::{Endpoint, ServiceTarget};
use crate::types::ingress_wire::anthropic::{
    AnthropicContent, AnthropicContentBlock, AnthropicImageSource, AnthropicMessage,
    AnthropicMessagesRequest, AnthropicResponse, AnthropicStopReason, AnthropicStreamContentBlock,
    AnthropicStreamDelta, AnthropicStreamEvent, AnthropicStreamMessageStart, AnthropicSystem,
    AnthropicSystemBlock, AnthropicTool, AnthropicToolChoice, AnthropicToolResultContent,
    AnthropicUsage,
};
use crate::types::{
    ChatChoice, ChatMessage, ChatRequest, ChatResponse, ChatStreamEvent, ContentPart, FinishReason,
    ImageUrl, MessageContent, PromptTokensDetails, Role, StreamEnd, ToolCall, ToolCallDelta,
    ToolCallFunction, Usage,
};

/// Anthropic Messages API 协议（`api.anthropic.com/v1/messages`）。
pub struct AnthropicAdapter;

impl AnthropicAdapter {
    pub const API_KEY_DEFAULT_ENV_NAME: &'static str = "ANTHROPIC_API_KEY";
    const BASE_URL: &'static str = "https://api.anthropic.com/v1/";
    const API_VERSION: &'static str = "2023-06-01";
}

impl Adapter for AnthropicAdapter {
    const KIND: AdapterKind = AdapterKind::Anthropic;
    const DEFAULT_API_KEY_ENV_NAME: Option<&'static str> = Some(Self::API_KEY_DEFAULT_ENV_NAME);

    fn default_endpoint() -> Option<Endpoint> {
        Some(Endpoint::from_static(Self::BASE_URL))
    }

    fn capabilities() -> Capabilities {
        Capabilities {
            streaming: true,
            tools: true,
            tool_choice: true,
            multimodal_input: true,
            reasoning: true, // extended thinking
            response_format: false,
            multi_choice: false, // n>1 不支持
            prompt_caching: true,
            parallel_tool_calls: true,
        }
    }

    fn auth_strategy() -> AuthStrategy {
        AuthStrategy::XApiKey
    }

    fn cost_profile() -> CostProfile {
        CostProfile::anthropic_like()
    }

    fn build_chat_request(
        target: &ServiceTarget,
        _service: ServiceType,
        req: &ChatRequest,
    ) -> AdapterResult<WebRequestData> {
        Self::validate_chat_request(req)?;

        let url = build_messages_url(target.endpoint.trimmed());
        let wire = canonical_to_claude_request(target, req)?;
        let payload = serde_json::to_value(&wire).map_err(AdapterError::SerializeRequest)?;
        let headers = build_headers(target)?;

        Ok(WebRequestData {
            url,
            headers,
            payload,
        })
    }

    fn parse_chat_response(target: &ServiceTarget, body: Bytes) -> AdapterResult<ChatResponse> {
        let resp: AnthropicResponse =
            serde_json::from_slice(&body).map_err(AdapterError::DeserializeResponse)?;
        Ok(claude_response_to_canonical(resp, target))
    }

    fn parse_chat_stream_event(
        _target: &ServiceTarget,
        raw: &str,
    ) -> AdapterResult<Option<ChatStreamEvent>> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        let event: AnthropicStreamEvent =
            serde_json::from_str(trimmed).map_err(AdapterError::DeserializeResponse)?;
        Ok(claude_stream_event_to_canonical(event))
    }

    fn fetch_model_names(
        _target: &ServiceTarget,
        _http: &reqwest::Client,
    ) -> impl Future<Output = AdapterResult<Vec<String>>> + Send {
        async {
            // Anthropic `/v1/models` 最近才出现且权限要求较高；后续按需启用。
            Err(AdapterError::Unsupported {
                adapter: Self::KIND.as_str(),
                feature: "fetch_model_names",
            })
        }
    }
}

// ---------------------------------------------------------------------------
// canonical → Anthropic wire (request)
// ---------------------------------------------------------------------------

fn canonical_to_claude_request(
    target: &ServiceTarget,
    req: &ChatRequest,
) -> AdapterResult<AnthropicMessagesRequest> {
    // max_tokens 是 Anthropic 必填字段
    let max_tokens = req
        .max_tokens
        .or(req.max_completion_tokens)
        .and_then(|n| u32::try_from(n.max(0)).ok())
        .unwrap_or(4096);

    // system 字段：从 canonical messages 里抽出 system / developer role，其他保留
    let mut system_text_parts: Vec<String> = Vec::new();
    let mut non_system_messages: Vec<&ChatMessage> = Vec::new();
    for msg in &req.messages {
        match msg.role {
            Role::System | Role::Developer => {
                if let Some(text) = message_text(msg) {
                    if !text.is_empty() {
                        system_text_parts.push(text);
                    }
                }
            }
            _ => non_system_messages.push(msg),
        }
    }
    let system = if system_text_parts.is_empty() {
        None
    } else if system_text_parts.len() == 1 {
        Some(AnthropicSystem::Text(
            system_text_parts.into_iter().next().unwrap(),
        ))
    } else {
        Some(AnthropicSystem::Blocks(
            system_text_parts
                .into_iter()
                .map(|text| AnthropicSystemBlock {
                    kind: "text".to_string(),
                    text,
                    cache_control: None,
                })
                .collect(),
        ))
    };

    // messages：把 role:tool 合并到下一个 user 消息的 content 里作 tool_result
    let messages = merge_tool_messages(&non_system_messages)?;

    // tools
    let tools = req
        .tools
        .as_ref()
        .map(|ts| {
            ts.iter()
                .map(|t| AnthropicTool {
                    name: t.function.name.clone(),
                    description: t.function.description.clone(),
                    input_schema: t
                        .function
                        .parameters
                        .clone()
                        .unwrap_or_else(|| serde_json::json!({"type": "object"})),
                    cache_control: None,
                })
                .collect()
        })
        .unwrap_or_default();

    // tool_choice
    let tool_choice = req
        .tool_choice
        .as_ref()
        .and_then(canonical_tool_choice_to_claude);

    // stop_sequences
    let stop_sequences = match &req.stop {
        Some(serde_json::Value::String(s)) => vec![s.clone()],
        Some(serde_json::Value::Array(arr)) => arr
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    };

    // thinking：从 canonical extra 里取（AnthropicIngress 可能已透传）
    let thinking = req
        .extra
        .get("thinking")
        .and_then(|v| serde_json::from_value(v.clone()).ok());

    // top_k 从 extra 取
    let top_k = req
        .extra
        .get("top_k")
        .and_then(|v| v.as_u64())
        .and_then(|v| u32::try_from(v).ok());

    Ok(AnthropicMessagesRequest {
        model: target.actual_model.clone(),
        messages,
        max_tokens,
        system,
        temperature: req.temperature,
        top_p: req.top_p,
        top_k,
        stop_sequences,
        stream: req.stream,
        tools,
        tool_choice,
        thinking,
        metadata: req.user.clone().map(|uid| {
            crate::types::ingress_wire::anthropic::AnthropicMetadata { user_id: Some(uid) }
        }),
        extra: serde_json::Map::new(),
    })
}

fn merge_tool_messages(msgs: &[&ChatMessage]) -> AdapterResult<Vec<AnthropicMessage>> {
    let mut out: Vec<AnthropicMessage> = Vec::with_capacity(msgs.len());
    let mut pending_tool_results: Vec<AnthropicContentBlock> = Vec::new();

    for msg in msgs {
        match msg.role {
            Role::Tool => {
                let tool_use_id = msg.tool_call_id.clone().unwrap_or_default();
                let content_text = message_text(msg).unwrap_or_default();
                pending_tool_results.push(AnthropicContentBlock::ToolResult {
                    tool_use_id,
                    content: Some(AnthropicToolResultContent::Text(content_text)),
                    is_error: None,
                    cache_control: None,
                });
            }
            Role::User => {
                let mut blocks = std::mem::take(&mut pending_tool_results);
                let user_content = canonical_message_to_claude_content(msg);
                match user_content {
                    AnthropicContent::Text(text) if blocks.is_empty() => {
                        out.push(AnthropicMessage {
                            role: "user".to_string(),
                            content: AnthropicContent::Text(text),
                        });
                    }
                    AnthropicContent::Text(text) => {
                        if !text.is_empty() {
                            blocks.push(AnthropicContentBlock::Text {
                                text,
                                cache_control: None,
                            });
                        }
                        out.push(AnthropicMessage {
                            role: "user".to_string(),
                            content: AnthropicContent::Blocks(blocks),
                        });
                    }
                    AnthropicContent::Blocks(user_blocks) => {
                        blocks.extend(user_blocks);
                        out.push(AnthropicMessage {
                            role: "user".to_string(),
                            content: AnthropicContent::Blocks(blocks),
                        });
                    }
                }
            }
            Role::Assistant => {
                // 有 pending tool_results 而下一个不是 user → 强行补一条 user 把它们发掉
                if !pending_tool_results.is_empty() {
                    let blocks = std::mem::take(&mut pending_tool_results);
                    out.push(AnthropicMessage {
                        role: "user".to_string(),
                        content: AnthropicContent::Blocks(blocks),
                    });
                }
                // assistant：text + tool_calls → blocks
                let mut blocks: Vec<AnthropicContentBlock> = Vec::new();
                if let Some(text) = message_text(msg) {
                    if !text.is_empty() {
                        blocks.push(AnthropicContentBlock::Text {
                            text,
                            cache_control: None,
                        });
                    }
                }
                if let Some(tool_calls) = &msg.tool_calls {
                    for tc in tool_calls {
                        let input =
                            serde_json::from_str::<serde_json::Value>(&tc.function.arguments)
                                .unwrap_or_else(|_| {
                                    serde_json::Value::String(tc.function.arguments.clone())
                                });
                        blocks.push(AnthropicContentBlock::ToolUse {
                            id: tc.id.clone(),
                            name: tc.function.name.clone(),
                            input,
                            cache_control: None,
                        });
                    }
                }
                let content = if blocks.len() == 1 {
                    if let AnthropicContentBlock::Text { text, .. } = &blocks[0] {
                        AnthropicContent::Text(text.clone())
                    } else {
                        AnthropicContent::Blocks(blocks)
                    }
                } else if blocks.is_empty() {
                    AnthropicContent::Text(String::new())
                } else {
                    AnthropicContent::Blocks(blocks)
                };
                out.push(AnthropicMessage {
                    role: "assistant".to_string(),
                    content,
                });
            }
            Role::System | Role::Developer => {
                // System 理论上已在前一步过滤；保险起见再 skip
            }
        }
    }

    // 结尾还有未 flush 的 tool_results → 作为独立 user 消息
    if !pending_tool_results.is_empty() {
        out.push(AnthropicMessage {
            role: "user".to_string(),
            content: AnthropicContent::Blocks(pending_tool_results),
        });
    }

    Ok(out)
}

fn canonical_message_to_claude_content(msg: &ChatMessage) -> AnthropicContent {
    let Some(content) = msg.content.as_ref() else {
        return AnthropicContent::Text(String::new());
    };
    match content {
        MessageContent::Text(s) => AnthropicContent::Text(s.clone()),
        MessageContent::Parts(parts) => {
            let mut blocks: Vec<AnthropicContentBlock> = Vec::new();
            for part in parts {
                match part {
                    ContentPart::Text { text } => blocks.push(AnthropicContentBlock::Text {
                        text: text.clone(),
                        cache_control: None,
                    }),
                    ContentPart::ImageUrl { image_url } => {
                        let source = parse_image_url(&image_url.url);
                        blocks.push(AnthropicContentBlock::Image {
                            source,
                            cache_control: None,
                        });
                    }
                    ContentPart::InputAudio { .. } => {
                        // Anthropic 当前不接受音频；丢弃
                    }
                }
            }
            AnthropicContent::Blocks(blocks)
        }
    }
}

fn parse_image_url(url: &str) -> AnthropicImageSource {
    // data:image/png;base64,XYZ
    if let Some(stripped) = url.strip_prefix("data:") {
        if let Some((meta, data)) = stripped.split_once(",") {
            let media_type = meta.split(';').next().unwrap_or("image/png").to_string();
            return AnthropicImageSource::Base64 {
                media_type,
                data: data.to_string(),
            };
        }
    }
    AnthropicImageSource::Url {
        url: url.to_string(),
    }
}

fn canonical_tool_choice_to_claude(tc: &crate::types::ToolChoice) -> Option<AnthropicToolChoice> {
    match tc {
        crate::types::ToolChoice::Mode(s) => match s.as_str() {
            "auto" => Some(AnthropicToolChoice::Auto {
                disable_parallel_tool_use: None,
            }),
            "none" => Some(AnthropicToolChoice::None),
            "required" => Some(AnthropicToolChoice::Any {
                disable_parallel_tool_use: None,
            }),
            _ => None,
        },
        crate::types::ToolChoice::Named(v) => {
            let name = v
                .get("function")
                .and_then(|f| f.get("name"))
                .and_then(|n| n.as_str())?;
            Some(AnthropicToolChoice::Tool {
                name: name.to_string(),
                disable_parallel_tool_use: None,
            })
        }
    }
}

fn message_text(msg: &ChatMessage) -> Option<String> {
    let content = msg.content.as_ref()?;
    match content {
        MessageContent::Text(s) => Some(s.clone()),
        MessageContent::Parts(parts) => {
            let mut buf = String::new();
            for part in parts {
                if let ContentPart::Text { text } = part {
                    if !buf.is_empty() {
                        buf.push('\n');
                    }
                    buf.push_str(text);
                }
            }
            if buf.is_empty() { None } else { Some(buf) }
        }
    }
}

// ---------------------------------------------------------------------------
// Anthropic wire → canonical (response)
// ---------------------------------------------------------------------------

fn claude_response_to_canonical(resp: AnthropicResponse, _target: &ServiceTarget) -> ChatResponse {
    let AnthropicResponse {
        id,
        content,
        model,
        stop_reason,
        usage,
        ..
    } = resp;

    let mut text_buf = String::new();
    let mut tool_calls: Vec<ToolCall> = Vec::new();

    for block in content {
        match block {
            AnthropicContentBlock::Text { text, .. } => {
                if !text_buf.is_empty() {
                    text_buf.push('\n');
                }
                text_buf.push_str(&text);
            }
            AnthropicContentBlock::ToolUse {
                id, name, input, ..
            } => {
                let arguments = serde_json::to_string(&input).unwrap_or_else(|_| "{}".to_string());
                tool_calls.push(ToolCall {
                    id,
                    kind: "function".to_string(),
                    function: ToolCallFunction { name, arguments },
                });
            }
            // 其他 block（thinking / redacted_thinking / ...）作为文本拼接或忽略
            AnthropicContentBlock::Thinking { .. }
            | AnthropicContentBlock::RedactedThinking { .. }
            | AnthropicContentBlock::Image { .. }
            | AnthropicContentBlock::ToolResult { .. }
            | AnthropicContentBlock::Document { .. } => {}
        }
    }

    let message = ChatMessage {
        role: Role::Assistant,
        content: if text_buf.is_empty() {
            None
        } else {
            Some(MessageContent::Text(text_buf))
        },
        refusal: None,
        name: None,
        tool_calls: if tool_calls.is_empty() {
            None
        } else {
            Some(tool_calls)
        },
        tool_call_id: None,
        audio: None,
    };

    let finish_reason = stop_reason.map(stop_reason_to_finish_reason);
    let canonical_usage = claude_usage_to_canonical(usage);

    ChatResponse {
        id,
        object: "chat.completion".to_string(),
        created: 0,
        model,
        choices: vec![ChatChoice {
            index: 0,
            message,
            logprobs: None,
            finish_reason,
        }],
        usage: canonical_usage,
        system_fingerprint: None,
        service_tier: None,
    }
}

fn stop_reason_to_finish_reason(reason: AnthropicStopReason) -> FinishReason {
    match reason {
        AnthropicStopReason::EndTurn
        | AnthropicStopReason::Refusal
        | AnthropicStopReason::PauseTurn => FinishReason::Stop,
        AnthropicStopReason::MaxTokens => FinishReason::Length,
        AnthropicStopReason::StopSequence => FinishReason::Stop,
        AnthropicStopReason::ToolUse => FinishReason::ToolCalls,
    }
}

fn claude_usage_to_canonical(usage: AnthropicUsage) -> Usage {
    let total = (usage.input_tokens as i64) + (usage.output_tokens as i64);
    Usage {
        prompt_tokens: usage.input_tokens as i64,
        completion_tokens: usage.output_tokens as i64,
        total_tokens: total,
        prompt_tokens_details: usage.cache_read_input_tokens.map(|v| PromptTokensDetails {
            cached_tokens: Some(v as i64),
            audio_tokens: None,
        }),
        completion_tokens_details: None,
    }
}

// ---------------------------------------------------------------------------
// Anthropic SSE event → canonical stream event
// ---------------------------------------------------------------------------

fn claude_stream_event_to_canonical(event: AnthropicStreamEvent) -> Option<ChatStreamEvent> {
    match event {
        AnthropicStreamEvent::MessageStart { message } => {
            let AnthropicStreamMessageStart { model, .. } = message;
            Some(ChatStreamEvent::Start {
                adapter: "anthropic".to_string(),
                model,
            })
        }
        AnthropicStreamEvent::ContentBlockStart {
            index,
            content_block,
        } => match content_block {
            AnthropicStreamContentBlock::ToolUse { id, name, .. } => {
                Some(ChatStreamEvent::ToolCallDelta(ToolCallDelta {
                    index: index as i32,
                    id: Some(id),
                    name: Some(name),
                    arguments_delta: None,
                }))
            }
            // text / thinking 的 content_block_start 里 text 通常空 → 忽略，等 delta
            _ => None,
        },
        AnthropicStreamEvent::ContentBlockDelta { index, delta } => match delta {
            AnthropicStreamDelta::TextDelta { text } => Some(ChatStreamEvent::TextDelta { text }),
            AnthropicStreamDelta::ThinkingDelta { thinking } => {
                Some(ChatStreamEvent::ReasoningDelta { text: thinking })
            }
            AnthropicStreamDelta::InputJsonDelta { partial_json } => {
                Some(ChatStreamEvent::ToolCallDelta(ToolCallDelta {
                    index: index as i32,
                    id: None,
                    name: None,
                    arguments_delta: Some(partial_json),
                }))
            }
            AnthropicStreamDelta::SignatureDelta { .. } => None,
        },
        AnthropicStreamEvent::ContentBlockStop { .. } => None,
        AnthropicStreamEvent::MessageDelta { delta, usage } => {
            let finish_reason = delta.stop_reason.map(stop_reason_to_finish_reason);
            let canonical_usage = usage.map(claude_usage_to_canonical);
            Some(ChatStreamEvent::End(StreamEnd {
                finish_reason,
                usage: canonical_usage,
            }))
        }
        AnthropicStreamEvent::MessageStop => None, // 已由 message_delta 产出 End
        AnthropicStreamEvent::Ping => None,
        AnthropicStreamEvent::Error { error } => {
            // 把 error 作为一个 End 事件透传，携带 stop_reason=None
            tracing::warn!(error.kind = %error.kind, error.message = %error.message,
                "anthropic stream error event");
            Some(ChatStreamEvent::End(StreamEnd {
                finish_reason: Some(FinishReason::Stop),
                usage: None,
            }))
        }
    }
}

// ---------------------------------------------------------------------------
// URL / Headers
// ---------------------------------------------------------------------------

fn build_messages_url(base: &str) -> String {
    let base = base.trim_end_matches('/');
    if base.ends_with("/messages") {
        return base.to_string();
    }
    if base.ends_with("/v1") || base.contains("/v1/") {
        format!("{base}/messages")
    } else {
        format!("{base}/v1/messages")
    }
}

fn build_headers(target: &ServiceTarget) -> AdapterResult<HeaderMap> {
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(
        HeaderName::from_static("anthropic-version"),
        HeaderValue::from_static(AnthropicAdapter::API_VERSION),
    );

    if let Some(key) = target.auth.resolve()? {
        let v =
            HeaderValue::from_str(&key).map_err(|e| AdapterError::InvalidHeader(e.to_string()))?;
        headers.insert(HeaderName::from_static("x-api-key"), v);
    }

    for (name, value) in &target.extra_headers {
        let name = HeaderName::try_from(name.as_str())
            .map_err(|e| AdapterError::InvalidHeader(e.to_string()))?;
        let value = HeaderValue::from_str(value.as_str())
            .map_err(|e| AdapterError::InvalidHeader(e.to_string()))?;
        headers.insert(name, value);
    }
    Ok(headers)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ChatMessage;

    fn target() -> ServiceTarget {
        ServiceTarget::bearer(
            "https://api.anthropic.com",
            "sk-ant-test",
            "claude-sonnet-4-5",
        )
    }

    #[test]
    fn url_appends_v1_messages() {
        let t = target();
        let req = ChatRequest::new("alias", vec![ChatMessage::user("hi")]);
        let data = AnthropicAdapter::build_chat_request(&t, ServiceType::Chat, &req).unwrap();
        assert_eq!(data.url, "https://api.anthropic.com/v1/messages");
    }

    #[test]
    fn system_messages_become_system_field() {
        let t = target();
        let mut req = ChatRequest::new(
            "x",
            vec![
                ChatMessage::system("you are helpful"),
                ChatMessage::user("hi"),
            ],
        );
        req.max_tokens = Some(128);
        let data = AnthropicAdapter::build_chat_request(&t, ServiceType::Chat, &req).unwrap();
        assert_eq!(data.payload["system"], "you are helpful");
        let msgs = data.payload["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["role"], "user");
    }

    #[test]
    fn max_tokens_required_default() {
        let t = target();
        let req = ChatRequest::new("x", vec![ChatMessage::user("hi")]);
        let data = AnthropicAdapter::build_chat_request(&t, ServiceType::Chat, &req).unwrap();
        assert_eq!(data.payload["max_tokens"], 4096);
    }

    #[test]
    fn tool_message_merges_into_next_user() {
        let t = target();
        let mut req = ChatRequest::new(
            "x",
            vec![
                ChatMessage::user("what weather"),
                ChatMessage {
                    role: Role::Assistant,
                    content: None,
                    refusal: None,
                    name: None,
                    tool_calls: Some(vec![ToolCall {
                        id: "tu_1".to_string(),
                        kind: "function".to_string(),
                        function: ToolCallFunction {
                            name: "weather".to_string(),
                            arguments: r#"{"city":"NYC"}"#.to_string(),
                        },
                    }]),
                    tool_call_id: None,
                    audio: None,
                },
                ChatMessage::tool_response("tu_1", "72F"),
                ChatMessage::user("thanks"),
            ],
        );
        req.max_tokens = Some(128);
        let data = AnthropicAdapter::build_chat_request(&t, ServiceType::Chat, &req).unwrap();
        let msgs = data.payload["messages"].as_array().unwrap();
        // user / assistant(tool_use) / user(tool_result+text)
        assert_eq!(msgs.len(), 3);
        let last_user = &msgs[2];
        assert_eq!(last_user["role"], "user");
        let blocks = last_user["content"].as_array().unwrap();
        assert_eq!(blocks[0]["type"], "tool_result");
        assert_eq!(blocks[0]["tool_use_id"], "tu_1");
        assert_eq!(blocks[1]["type"], "text");
    }

    #[test]
    fn assistant_tool_calls_become_tool_use_blocks() {
        let t = target();
        let mut req = ChatRequest::new(
            "x",
            vec![ChatMessage {
                role: Role::Assistant,
                content: Some(MessageContent::Text("let me check".to_string())),
                refusal: None,
                name: None,
                tool_calls: Some(vec![ToolCall {
                    id: "tu_1".to_string(),
                    kind: "function".to_string(),
                    function: ToolCallFunction {
                        name: "weather".to_string(),
                        arguments: r#"{"city":"NYC"}"#.to_string(),
                    },
                }]),
                tool_call_id: None,
                audio: None,
            }],
        );
        req.max_tokens = Some(128);
        let data = AnthropicAdapter::build_chat_request(&t, ServiceType::Chat, &req).unwrap();
        let blocks = data.payload["messages"][0]["content"].as_array().unwrap();
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0]["type"], "text");
        assert_eq!(blocks[1]["type"], "tool_use");
        assert_eq!(blocks[1]["id"], "tu_1");
        assert_eq!(blocks[1]["name"], "weather");
    }

    #[test]
    fn headers_contain_api_key_and_version() {
        let t = target();
        let req = ChatRequest::new("x", vec![ChatMessage::user("hi")]);
        let data = AnthropicAdapter::build_chat_request(&t, ServiceType::Chat, &req).unwrap();
        assert_eq!(
            data.headers.get("x-api-key").unwrap().to_str().unwrap(),
            "sk-ant-test"
        );
        assert_eq!(
            data.headers
                .get("anthropic-version")
                .unwrap()
                .to_str()
                .unwrap(),
            "2023-06-01"
        );
    }

    #[test]
    fn parse_response_basic() {
        let t = target();
        let body = br#"{
            "id":"msg_1","type":"message","role":"assistant",
            "content":[{"type":"text","text":"hello"}],
            "model":"claude-sonnet-4-5",
            "stop_reason":"end_turn",
            "usage":{"input_tokens":5,"output_tokens":2,"cache_read_input_tokens":3}
        }"#;
        let resp = AnthropicAdapter::parse_chat_response(&t, Bytes::from_static(body)).unwrap();
        assert_eq!(resp.id, "msg_1");
        assert_eq!(resp.first_text(), Some("hello"));
        assert_eq!(resp.usage.prompt_tokens, 5);
        assert_eq!(resp.usage.completion_tokens, 2);
        assert_eq!(
            resp.usage
                .prompt_tokens_details
                .as_ref()
                .unwrap()
                .cached_tokens,
            Some(3)
        );
        assert_eq!(resp.choices[0].finish_reason, Some(FinishReason::Stop));
    }

    #[test]
    fn stream_event_message_start_becomes_canonical_start() {
        let t = target();
        let raw = r#"{
            "type":"message_start",
            "message":{"id":"msg_1","type":"message","role":"assistant","content":[],
                "model":"claude-sonnet-4-5","usage":{"input_tokens":5,"output_tokens":0}}
        }"#;
        let e = AnthropicAdapter::parse_chat_stream_event(&t, raw)
            .unwrap()
            .unwrap();
        match e {
            ChatStreamEvent::Start { adapter, model } => {
                assert_eq!(adapter, "anthropic");
                assert_eq!(model, "claude-sonnet-4-5");
            }
            _ => panic!(),
        }
    }

    #[test]
    fn stream_text_delta() {
        let t = target();
        let raw =
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"hi"}}"#;
        let e = AnthropicAdapter::parse_chat_stream_event(&t, raw)
            .unwrap()
            .unwrap();
        match e {
            ChatStreamEvent::TextDelta { text } => assert_eq!(text, "hi"),
            _ => panic!(),
        }
    }

    #[test]
    fn stream_tool_use_start_and_input_json_delta() {
        let t = target();
        let start = r#"{
            "type":"content_block_start","index":1,
            "content_block":{"type":"tool_use","id":"tu_1","name":"weather","input":{}}
        }"#;
        let e = AnthropicAdapter::parse_chat_stream_event(&t, start)
            .unwrap()
            .unwrap();
        match e {
            ChatStreamEvent::ToolCallDelta(d) => {
                assert_eq!(d.index, 1);
                assert_eq!(d.id.as_deref(), Some("tu_1"));
                assert_eq!(d.name.as_deref(), Some("weather"));
            }
            _ => panic!(),
        }

        let delta = r#"{
            "type":"content_block_delta","index":1,
            "delta":{"type":"input_json_delta","partial_json":"{\"city\""}
        }"#;
        let e = AnthropicAdapter::parse_chat_stream_event(&t, delta)
            .unwrap()
            .unwrap();
        match e {
            ChatStreamEvent::ToolCallDelta(d) => {
                assert!(d.arguments_delta.as_deref().unwrap().starts_with("{\"city"));
            }
            _ => panic!(),
        }
    }

    #[test]
    fn stream_message_delta_becomes_end() {
        let t = target();
        let raw = r#"{
            "type":"message_delta",
            "delta":{"stop_reason":"end_turn"},
            "usage":{"input_tokens":0,"output_tokens":10}
        }"#;
        let e = AnthropicAdapter::parse_chat_stream_event(&t, raw)
            .unwrap()
            .unwrap();
        match e {
            ChatStreamEvent::End(end) => {
                assert_eq!(end.finish_reason, Some(FinishReason::Stop));
                assert_eq!(end.usage.as_ref().unwrap().completion_tokens, 10);
            }
            _ => panic!(),
        }
    }

    #[test]
    fn stream_ping_and_stop_ignored() {
        let t = target();
        assert!(
            AnthropicAdapter::parse_chat_stream_event(&t, r#"{"type":"ping"}"#)
                .unwrap()
                .is_none()
        );
        assert!(
            AnthropicAdapter::parse_chat_stream_event(&t, r#"{"type":"message_stop"}"#)
                .unwrap()
                .is_none()
        );
    }
}
