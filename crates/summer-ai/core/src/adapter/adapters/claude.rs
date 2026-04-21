//! Claude Messages API adapter。
//!
//! canonical ↔ Claude wire 双向转换，挂 `AdapterDispatcher` 用。
//!
//! # 鉴权
//!
//! - header `x-api-key: {key}`
//! - header `anthropic-version: 2023-06-01`
//!
//! # prompt cache 计费
//!
//! Claude 独有 `cache_creation_input_tokens`（写入 1.25x）和
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
use crate::types::ingress_wire::claude::{
    ClaudeContent, ClaudeContentBlock, ClaudeImageSource, ClaudeMessage, ClaudeMessagesRequest,
    ClaudeResponse, ClaudeStopReason, ClaudeStreamContentBlock, ClaudeStreamDelta,
    ClaudeStreamEvent, ClaudeStreamMessageStart, ClaudeSystem, ClaudeSystemBlock, ClaudeTool,
    ClaudeToolChoice, ClaudeToolResultContent, ClaudeUsage,
};
use crate::types::{
    ChatChoice, ChatMessage, ChatRequest, ChatResponse, ChatStreamEvent, ContentPart, FinishReason,
    MessageContent, PromptTokensDetails, Role, StreamEnd, StreamError, ToolCall, ToolCallDelta,
    ToolCallFunction, Usage,
};

/// Claude Messages API 协议（`api.anthropic.com/v1/messages`）。
pub struct ClaudeAdapter;

impl ClaudeAdapter {
    pub const API_KEY_DEFAULT_ENV_NAME: &'static str = "ANTHROPIC_API_KEY";
    const BASE_URL: &'static str = "https://api.anthropic.com/v1/";
    const API_VERSION: &'static str = "2023-06-01";
}

impl Adapter for ClaudeAdapter {
    const KIND: AdapterKind = AdapterKind::Claude;
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
        let resp: ClaudeResponse =
            serde_json::from_slice(&body).map_err(AdapterError::DeserializeResponse)?;
        Ok(claude_response_to_canonical(resp, target))
    }

    fn parse_chat_stream_event(
        _target: &ServiceTarget,
        raw: &str,
    ) -> AdapterResult<Vec<ChatStreamEvent>> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Ok(Vec::new());
        }
        let event: ClaudeStreamEvent =
            serde_json::from_str(trimmed).map_err(AdapterError::DeserializeResponse)?;
        Ok(claude_stream_event_to_canonical(event))
    }

    fn fetch_model_names(
        _target: &ServiceTarget,
        _http: &reqwest::Client,
    ) -> impl Future<Output = AdapterResult<Vec<String>>> + Send {
        async {
            // Claude `/v1/models` 最近才出现且权限要求较高；后续按需启用。
            Err(AdapterError::Unsupported {
                adapter: Self::KIND.as_str(),
                feature: "fetch_model_names",
            })
        }
    }
}

// ---------------------------------------------------------------------------
// canonical → Claude wire (request)
// ---------------------------------------------------------------------------

fn canonical_to_claude_request(
    target: &ServiceTarget,
    req: &ChatRequest,
) -> AdapterResult<ClaudeMessagesRequest> {
    // max_tokens 是 Claude 必填字段
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
        Some(ClaudeSystem::Text(
            system_text_parts.into_iter().next().unwrap(),
        ))
    } else {
        Some(ClaudeSystem::Blocks(
            system_text_parts
                .into_iter()
                .map(|text| ClaudeSystemBlock {
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
                .map(|t| ClaudeTool {
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

    // thinking：从 canonical extra 里取（ClaudeIngress 可能已透传）
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

    Ok(ClaudeMessagesRequest {
        model: target.actual_model().to_string(),
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
        metadata: req
            .user
            .clone()
            .map(|uid| crate::types::ingress_wire::claude::ClaudeMetadata { user_id: Some(uid) }),
        extra: serde_json::Map::new(),
    })
}

fn merge_tool_messages(msgs: &[&ChatMessage]) -> AdapterResult<Vec<ClaudeMessage>> {
    let mut out: Vec<ClaudeMessage> = Vec::with_capacity(msgs.len());
    let mut pending_tool_results: Vec<ClaudeContentBlock> = Vec::new();

    for msg in msgs {
        match msg.role {
            Role::Tool => {
                let tool_use_id = msg.tool_call_id.clone().unwrap_or_default();
                let content_text = message_text(msg).unwrap_or_default();
                pending_tool_results.push(ClaudeContentBlock::ToolResult {
                    tool_use_id,
                    content: Some(ClaudeToolResultContent::Text(content_text)),
                    is_error: None,
                    cache_control: None,
                });
            }
            Role::User => {
                let mut blocks = std::mem::take(&mut pending_tool_results);
                let user_content = canonical_message_to_claude_content(msg);
                match user_content {
                    ClaudeContent::Text(text) if blocks.is_empty() => {
                        out.push(ClaudeMessage {
                            role: "user".to_string(),
                            content: ClaudeContent::Text(text),
                        });
                    }
                    ClaudeContent::Text(text) => {
                        if !text.is_empty() {
                            blocks.push(ClaudeContentBlock::Text {
                                text,
                                cache_control: None,
                            });
                        }
                        out.push(ClaudeMessage {
                            role: "user".to_string(),
                            content: ClaudeContent::Blocks(blocks),
                        });
                    }
                    ClaudeContent::Blocks(user_blocks) => {
                        blocks.extend(user_blocks);
                        out.push(ClaudeMessage {
                            role: "user".to_string(),
                            content: ClaudeContent::Blocks(blocks),
                        });
                    }
                }
            }
            Role::Assistant => {
                // 有 pending tool_results 而下一个不是 user → 强行补一条 user 把它们发掉
                if !pending_tool_results.is_empty() {
                    let blocks = std::mem::take(&mut pending_tool_results);
                    out.push(ClaudeMessage {
                        role: "user".to_string(),
                        content: ClaudeContent::Blocks(blocks),
                    });
                }
                // assistant：text + tool_calls → blocks
                let mut blocks: Vec<ClaudeContentBlock> = Vec::new();
                if let Some(text) = message_text(msg) {
                    if !text.is_empty() {
                        blocks.push(ClaudeContentBlock::Text {
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
                        blocks.push(ClaudeContentBlock::ToolUse {
                            id: tc.id.clone(),
                            name: tc.function.name.clone(),
                            input,
                            cache_control: None,
                        });
                    }
                }
                let content = if blocks.len() == 1 {
                    if let ClaudeContentBlock::Text { text, .. } = &blocks[0] {
                        ClaudeContent::Text(text.clone())
                    } else {
                        ClaudeContent::Blocks(blocks)
                    }
                } else if blocks.is_empty() {
                    ClaudeContent::Text(String::new())
                } else {
                    ClaudeContent::Blocks(blocks)
                };
                out.push(ClaudeMessage {
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
        out.push(ClaudeMessage {
            role: "user".to_string(),
            content: ClaudeContent::Blocks(pending_tool_results),
        });
    }

    Ok(out)
}

fn canonical_message_to_claude_content(msg: &ChatMessage) -> ClaudeContent {
    let Some(content) = msg.content.as_ref() else {
        return ClaudeContent::Text(String::new());
    };
    match content {
        MessageContent::Text(s) => ClaudeContent::Text(s.clone()),
        MessageContent::Parts(parts) => {
            let mut blocks: Vec<ClaudeContentBlock> = Vec::new();
            for part in parts {
                match part {
                    ContentPart::Text { text } => blocks.push(ClaudeContentBlock::Text {
                        text: text.clone(),
                        cache_control: None,
                    }),
                    ContentPart::ImageUrl { image_url } => {
                        let source = parse_image_url(&image_url.url);
                        blocks.push(ClaudeContentBlock::Image {
                            source,
                            cache_control: None,
                        });
                    }
                    ContentPart::InputAudio { .. } => {
                        // Claude 当前不接受音频；丢弃
                    }
                }
            }
            ClaudeContent::Blocks(blocks)
        }
    }
}

fn parse_image_url(url: &str) -> ClaudeImageSource {
    // data:image/png;base64,XYZ
    if let Some(stripped) = url.strip_prefix("data:") {
        if let Some((meta, data)) = stripped.split_once(",") {
            let media_type = meta.split(';').next().unwrap_or("image/png").to_string();
            return ClaudeImageSource::Base64 {
                media_type,
                data: data.to_string(),
            };
        }
    }
    ClaudeImageSource::Url {
        url: url.to_string(),
    }
}

fn canonical_tool_choice_to_claude(tc: &crate::types::ToolChoice) -> Option<ClaudeToolChoice> {
    match tc {
        crate::types::ToolChoice::Mode(s) => match s.as_str() {
            "auto" => Some(ClaudeToolChoice::Auto {
                disable_parallel_tool_use: None,
            }),
            "none" => Some(ClaudeToolChoice::None),
            "required" => Some(ClaudeToolChoice::Any {
                disable_parallel_tool_use: None,
            }),
            _ => None,
        },
        crate::types::ToolChoice::Named(v) => {
            let name = v
                .get("function")
                .and_then(|f| f.get("name"))
                .and_then(|n| n.as_str())?;
            Some(ClaudeToolChoice::Tool {
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
// Claude wire → canonical (response)
// ---------------------------------------------------------------------------

fn claude_response_to_canonical(resp: ClaudeResponse, _target: &ServiceTarget) -> ChatResponse {
    let ClaudeResponse {
        id,
        content,
        model,
        stop_reason,
        usage,
        ..
    } = resp;

    let mut text_buf = String::new();
    let mut reasoning_buf = String::new();
    let mut tool_calls: Vec<ToolCall> = Vec::new();

    for block in content {
        match block {
            ClaudeContentBlock::Text { text, .. } => {
                if !text_buf.is_empty() {
                    text_buf.push('\n');
                }
                text_buf.push_str(&text);
            }
            ClaudeContentBlock::ToolUse {
                id, name, input, ..
            } => {
                // input=null 时 serde_json::to_string 会产出 "null" —— 客户端
                // `JSON.parse("null")` 合法但 `arguments.x` 全部 undefined。
                // 按 Anthropic 协议 tool_use.input 必是 object,null 视作空对象。
                let arguments = if input.is_null() {
                    "{}".to_string()
                } else {
                    serde_json::to_string(&input).unwrap_or_else(|_| "{}".to_string())
                };
                tool_calls.push(ToolCall {
                    id,
                    kind: "function".to_string(),
                    function: ToolCallFunction { name, arguments },
                });
            }
            // extended thinking 的文本进 reasoning_content，让客户端能看到思考链；
            // redacted_thinking 只有加密 data 没有明文，忽略（multi-turn 续接在流式
            // 路径里靠 ThoughtSignature 而非 content block）。
            ClaudeContentBlock::Thinking { thinking, .. } => {
                if !reasoning_buf.is_empty() {
                    reasoning_buf.push('\n');
                }
                reasoning_buf.push_str(&thinking);
            }
            ClaudeContentBlock::RedactedThinking { .. }
            | ClaudeContentBlock::Image { .. }
            | ClaudeContentBlock::ToolResult { .. }
            | ClaudeContentBlock::Document { .. }
            | ClaudeContentBlock::Unknown => {}
        }
    }

    let message = ChatMessage {
        role: Role::Assistant,
        content: if text_buf.is_empty() {
            None
        } else {
            Some(MessageContent::Text(text_buf))
        },
        reasoning_content: if reasoning_buf.is_empty() {
            None
        } else {
            Some(reasoning_buf)
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

fn stop_reason_to_finish_reason(reason: ClaudeStopReason) -> FinishReason {
    match reason {
        ClaudeStopReason::EndTurn | ClaudeStopReason::Refusal | ClaudeStopReason::PauseTurn => {
            FinishReason::Stop
        }
        ClaudeStopReason::MaxTokens => FinishReason::Length,
        ClaudeStopReason::StopSequence => FinishReason::Stop,
        ClaudeStopReason::ToolUse => FinishReason::ToolCalls,
    }
}

fn claude_usage_to_canonical(usage: ClaudeUsage) -> Usage {
    // Anthropic 的 `input_tokens` 不包含 cache_creation / cache_read —— 三者相加
    // 才是"真正的 prompt 侧 token 消耗"。直接用 input_tokens 会让 billing 少算
    // cache 部分（0.1x 的 read、1.25x 的 write 全丢），也和 OpenAI 风格里
    // prompt_tokens 含 cached_tokens 的惯例不一致。
    let input = usage.input_tokens as i64;
    let cache_creation = usage.cache_creation_input_tokens.map(|v| v as i64);
    let cache_read = usage.cache_read_input_tokens.map(|v| v as i64);

    let prompt_tokens = input + cache_creation.unwrap_or(0) + cache_read.unwrap_or(0);
    let completion_tokens = usage.output_tokens as i64;
    let total_tokens = prompt_tokens + completion_tokens;

    let prompt_tokens_details = if cache_creation.is_some() || cache_read.is_some() {
        Some(PromptTokensDetails {
            cached_tokens: cache_read,
            cache_creation_tokens: cache_creation,
            audio_tokens: None,
        })
    } else {
        None
    };

    Usage {
        prompt_tokens,
        completion_tokens,
        total_tokens,
        prompt_tokens_details,
        completion_tokens_details: None,
    }
}

// ---------------------------------------------------------------------------
// Claude SSE event → canonical stream event
// ---------------------------------------------------------------------------

fn claude_stream_event_to_canonical(event: ClaudeStreamEvent) -> Vec<ChatStreamEvent> {
    match event {
        ClaudeStreamEvent::MessageStart { message } => {
            // message_start 是 Claude stream 里唯一携带完整 prompt 侧 usage 的事件：
            // `input_tokens + cache_creation_input_tokens + cache_read_input_tokens` 都
            // 在这儿。后续 `message_delta.usage` 只会累积 output_tokens，prompt 侧是 0。
            // 因此这里除了 Start 之外还要 emit 一个 UsageDelta 把 prompt/cache 带出去，
            // 不然 stream_driver 的 final_usage 在流结尾拿到的 prompt_tokens 就是 0。
            let ClaudeStreamMessageStart { model, usage, .. } = message;
            let mut events = Vec::with_capacity(2);
            events.push(ChatStreamEvent::Start {
                adapter: "anthropic".to_string(),
                model,
            });
            let canonical_usage = claude_usage_to_canonical(usage);
            if canonical_usage.prompt_tokens > 0
                || canonical_usage.completion_tokens > 0
                || canonical_usage.prompt_tokens_details.is_some()
            {
                events.push(ChatStreamEvent::UsageDelta(canonical_usage));
            }
            events
        }
        ClaudeStreamEvent::ContentBlockStart {
            index,
            content_block,
        } => match content_block {
            ClaudeStreamContentBlock::ToolUse { id, name, .. } => {
                vec![ChatStreamEvent::ToolCallDelta(ToolCallDelta {
                    index: index as i32,
                    id: Some(id),
                    name: Some(name),
                    arguments_delta: None,
                })]
            }
            // text / thinking 的 content_block_start 里 text 通常空 → 忽略，等 delta
            _ => Vec::new(),
        },
        ClaudeStreamEvent::ContentBlockDelta { index, delta } => match delta {
            ClaudeStreamDelta::TextDelta { text } => vec![ChatStreamEvent::TextDelta { text }],
            ClaudeStreamDelta::ThinkingDelta { thinking } => {
                vec![ChatStreamEvent::ReasoningDelta { text: thinking }]
            }
            ClaudeStreamDelta::InputJsonDelta { partial_json } => {
                vec![ChatStreamEvent::ToolCallDelta(ToolCallDelta {
                    index: index as i32,
                    id: None,
                    name: None,
                    arguments_delta: Some(partial_json),
                })]
            }
            // signature_delta 是 Claude extended thinking 的 multi-turn 续接凭证——
            // 下一轮客户端要把完整 signature 回传上游以继承思考状态。丢弃会导致
            // thinking + multi-turn 的第二轮 400。透传给 ingress 让客户端拿到。
            ClaudeStreamDelta::SignatureDelta { signature } => {
                vec![ChatStreamEvent::ThoughtSignature { signature }]
            }
        },
        ClaudeStreamEvent::ContentBlockStop { .. } => Vec::new(),
        ClaudeStreamEvent::MessageDelta { delta, usage } => {
            let finish_reason = delta.stop_reason.map(stop_reason_to_finish_reason);
            let canonical_usage = usage.map(claude_usage_to_canonical);
            vec![ChatStreamEvent::End(StreamEnd {
                finish_reason,
                usage: canonical_usage,
            })]
        }
        ClaudeStreamEvent::MessageStop => Vec::new(), // 已由 message_delta 产出 End
        ClaudeStreamEvent::Ping => Vec::new(),
        ClaudeStreamEvent::Error { error } => {
            // 上游 SSE 中途报错：emit canonical Error，stream_driver 会终止流并置
            // outcome 为 Failure（触发 billing refund / tracking failure）。
            // 之前假装成 End{Stop} 会让计费 settle 成功、客户端以为响应完整。
            vec![ChatStreamEvent::Error(StreamError {
                message: error.message,
                kind: Some(error.kind),
            })]
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
        HeaderValue::from_static(ClaudeAdapter::API_VERSION),
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
            AdapterKind::Claude,
            "https://api.anthropic.com",
            "sk-ant-test",
            "claude-sonnet-4-5",
        )
    }

    #[test]
    fn url_appends_v1_messages() {
        let t = target();
        let req = ChatRequest::new("alias", vec![ChatMessage::user("hi")]);
        let data = ClaudeAdapter::build_chat_request(&t, ServiceType::Chat, &req).unwrap();
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
        let data = ClaudeAdapter::build_chat_request(&t, ServiceType::Chat, &req).unwrap();
        assert_eq!(data.payload["system"], "you are helpful");
        let msgs = data.payload["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["role"], "user");
    }

    #[test]
    fn max_tokens_required_default() {
        let t = target();
        let req = ChatRequest::new("x", vec![ChatMessage::user("hi")]);
        let data = ClaudeAdapter::build_chat_request(&t, ServiceType::Chat, &req).unwrap();
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
                    reasoning_content: None,
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
        let data = ClaudeAdapter::build_chat_request(&t, ServiceType::Chat, &req).unwrap();
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
                reasoning_content: None,
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
        let data = ClaudeAdapter::build_chat_request(&t, ServiceType::Chat, &req).unwrap();
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
        let data = ClaudeAdapter::build_chat_request(&t, ServiceType::Chat, &req).unwrap();
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
        // Claude 的 input_tokens 不含 cache_read / cache_creation —— canonical 层
        // 必须把三者相加作为 prompt_tokens（对齐 OpenAI 语义里 prompt 含 cached），
        // 不然 billing 会少算 cache 部分的 token（0.1x 的 read、1.25x 的 write）。
        let t = target();
        let body = br#"{
            "id":"msg_1","type":"message","role":"assistant",
            "content":[{"type":"text","text":"hello"}],
            "model":"claude-sonnet-4-5",
            "stop_reason":"end_turn",
            "usage":{"input_tokens":5,"output_tokens":2,"cache_read_input_tokens":3}
        }"#;
        let resp = ClaudeAdapter::parse_chat_response(&t, Bytes::from_static(body)).unwrap();
        assert_eq!(resp.id, "msg_1");
        assert_eq!(resp.first_text(), Some("hello"));
        // 5 (input) + 3 (cache_read) = 8
        assert_eq!(resp.usage.prompt_tokens, 8);
        assert_eq!(resp.usage.completion_tokens, 2);
        assert_eq!(resp.usage.total_tokens, 10);
        let details = resp.usage.prompt_tokens_details.as_ref().unwrap();
        assert_eq!(details.cached_tokens, Some(3));
        assert_eq!(details.cache_creation_tokens, None);
        assert_eq!(resp.choices[0].finish_reason, Some(FinishReason::Stop));
    }

    #[test]
    fn parse_response_with_cache_creation_sums_into_prompt_tokens() {
        // cache_creation 是 1.25x 计费的 "写入 prompt cache"，必须进 prompt_tokens
        // 且在 prompt_tokens_details.cache_creation_tokens 独立暴露。
        let t = target();
        let body = br#"{
            "id":"msg_x","type":"message","role":"assistant",
            "content":[{"type":"text","text":"hi"}],
            "model":"claude-sonnet-4-5",
            "stop_reason":"end_turn",
            "usage":{
                "input_tokens":10,
                "output_tokens":4,
                "cache_creation_input_tokens":200,
                "cache_read_input_tokens":80
            }
        }"#;
        let resp = ClaudeAdapter::parse_chat_response(&t, Bytes::from_static(body)).unwrap();
        // 10 + 200 + 80 = 290
        assert_eq!(resp.usage.prompt_tokens, 290);
        assert_eq!(resp.usage.completion_tokens, 4);
        assert_eq!(resp.usage.total_tokens, 294);
        let details = resp.usage.prompt_tokens_details.as_ref().unwrap();
        assert_eq!(details.cached_tokens, Some(80));
        assert_eq!(details.cache_creation_tokens, Some(200));
    }

    #[test]
    fn parse_response_extended_thinking_goes_into_reasoning_content() {
        // Claude extended thinking 的 content block 是 `{"type":"thinking","thinking":"..."}`，
        // 非流式响应下之前直接丢弃 —— 客户端完全看不到思考链。现在入 canonical
        // ChatMessage.reasoning_content，egress 再决定是否透传。
        let t = target();
        let body = br#"{
            "id":"msg_t","type":"message","role":"assistant",
            "content":[
                {"type":"thinking","thinking":"let me reason"},
                {"type":"text","text":"final answer"}
            ],
            "model":"claude-sonnet-4-5",
            "stop_reason":"end_turn",
            "usage":{"input_tokens":5,"output_tokens":2}
        }"#;
        let resp = ClaudeAdapter::parse_chat_response(&t, Bytes::from_static(body)).unwrap();
        let msg = &resp.choices[0].message;
        assert_eq!(
            msg.reasoning_content.as_deref(),
            Some("let me reason"),
            "thinking block should populate reasoning_content"
        );
        // content 仍只含正文
        match msg.content.as_ref().unwrap() {
            MessageContent::Text(t) => assert_eq!(t, "final answer"),
            other => panic!("expected Text, got {other:?}"),
        }
    }

    #[test]
    fn parse_response_tool_use_input_null_becomes_empty_object() {
        // 上游极少会把 `tool_use.input` 置为 null（协议上要求是 object），但若发生
        // 过，之前会被 `serde_json::to_string` 序列成 `"null"`，客户端 `JSON.parse`
        // 后得到 `null` 而非 `{}`，`arguments.x` 全部 undefined 直接炸参数。
        // 这里锁死 null → `"{}"`。
        let t = target();
        let body = br#"{
            "id":"msg_n","type":"message","role":"assistant",
            "content":[{"type":"tool_use","id":"tu_x","name":"noop","input":null}],
            "model":"claude-sonnet-4-5",
            "stop_reason":"tool_use",
            "usage":{"input_tokens":1,"output_tokens":1}
        }"#;
        let resp = ClaudeAdapter::parse_chat_response(&t, Bytes::from_static(body)).unwrap();
        let tcs = resp.choices[0].message.tool_calls.as_ref().unwrap();
        assert_eq!(tcs[0].function.arguments, "{}");
    }

    #[test]
    fn stream_event_message_start_becomes_canonical_start() {
        let t = target();
        let raw = r#"{
            "type":"message_start",
            "message":{"id":"msg_1","type":"message","role":"assistant","content":[],
                "model":"claude-sonnet-4-5","usage":{"input_tokens":5,"output_tokens":0}}
        }"#;
        let e = ClaudeAdapter::parse_chat_stream_event(&t, raw)
            .unwrap()
            .into_iter()
            .next()
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
    fn stream_message_start_emits_start_then_usage_delta() {
        // message_start 是 Claude stream 里唯一送 prompt 侧 usage 的事件：
        // input_tokens + cache_creation + cache_read 全部在这儿。必须 emit 出
        // Start + UsageDelta 两个事件，否则 stream_driver 的 final_usage
        // 在流尾只会拿到 message_delta 的 output_tokens，prompt 整条归 0，
        // billing 无法为 prompt 花费计费。
        let t = target();
        let raw = r#"{
            "type":"message_start",
            "message":{"id":"msg_1","type":"message","role":"assistant","content":[],
                "model":"claude-sonnet-4-5",
                "usage":{
                    "input_tokens":12,
                    "output_tokens":0,
                    "cache_creation_input_tokens":200,
                    "cache_read_input_tokens":80
                }
            }
        }"#;
        let events = ClaudeAdapter::parse_chat_stream_event(&t, raw).unwrap();
        assert_eq!(events.len(), 2, "expected Start + UsageDelta");
        assert!(matches!(events[0], ChatStreamEvent::Start { .. }));
        match &events[1] {
            ChatStreamEvent::UsageDelta(u) => {
                // 12 + 200 + 80 = 292
                assert_eq!(u.prompt_tokens, 292);
                assert_eq!(u.completion_tokens, 0);
                let details = u.prompt_tokens_details.as_ref().unwrap();
                assert_eq!(details.cached_tokens, Some(80));
                assert_eq!(details.cache_creation_tokens, Some(200));
            }
            other => panic!("expected UsageDelta, got {other:?}"),
        }
    }

    #[test]
    fn stream_message_start_without_prompt_usage_skips_usage_delta() {
        // 上游偶尔发 message_start 但 input_tokens=0 + 无 cache 字段（e.g. 预热
        // / replay）；此时 emit 一个空 UsageDelta 只会让 stream_driver 无意义
        // merge，干脆只发 Start。
        let t = target();
        let raw = r#"{
            "type":"message_start",
            "message":{"id":"msg_1","type":"message","role":"assistant","content":[],
                "model":"claude-sonnet-4-5","usage":{"input_tokens":0,"output_tokens":0}}
        }"#;
        let events = ClaudeAdapter::parse_chat_stream_event(&t, raw).unwrap();
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], ChatStreamEvent::Start { .. }));
    }

    #[test]
    fn stream_text_delta() {
        let t = target();
        let raw =
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"hi"}}"#;
        let e = ClaudeAdapter::parse_chat_stream_event(&t, raw)
            .unwrap()
            .into_iter()
            .next()
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
        let e = ClaudeAdapter::parse_chat_stream_event(&t, start)
            .unwrap()
            .into_iter()
            .next()
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
        let e = ClaudeAdapter::parse_chat_stream_event(&t, delta)
            .unwrap()
            .into_iter()
            .next()
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
        let e = ClaudeAdapter::parse_chat_stream_event(&t, raw)
            .unwrap()
            .into_iter()
            .next()
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
            ClaudeAdapter::parse_chat_stream_event(&t, r#"{"type":"ping"}"#)
                .unwrap()
                .is_empty()
        );
        assert!(
            ClaudeAdapter::parse_chat_stream_event(&t, r#"{"type":"message_stop"}"#)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn stream_error_event_maps_to_canonical_error() {
        // Claude `event: error` 必须映射到 canonical `Error`，stream_driver 据此
        // 走 Failure 路径（refund billing / 记失败）；之前假装成 End+Stop 会
        // 让计费成功落库、客户端以为响应完整。
        let t = target();
        let raw = r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#;
        let e = ClaudeAdapter::parse_chat_stream_event(&t, raw)
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        match e {
            ChatStreamEvent::Error(err) => {
                assert_eq!(err.message, "Overloaded");
                assert_eq!(err.kind.as_deref(), Some("overloaded_error"));
            }
            other => panic!("expected Error, got {other:?}"),
        }
    }

    #[test]
    fn stream_signature_delta_maps_to_thought_signature() {
        // Claude extended thinking 的 `signature_delta` 用于 multi-turn 续接：
        // 下一轮客户端要回传完整 signature 上游才能接着思考。之前直接丢弃
        // 会让 thinking + multi-turn 的下一轮 400（signature 缺失）。
        let t = target();
        let raw = r#"{
            "type":"content_block_delta",
            "index":0,
            "delta":{"type":"signature_delta","signature":"EqMC..."}
        }"#;
        let e = ClaudeAdapter::parse_chat_stream_event(&t, raw)
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        match e {
            ChatStreamEvent::ThoughtSignature { signature } => assert_eq!(signature, "EqMC..."),
            other => panic!("expected ThoughtSignature, got {other:?}"),
        }
    }
}
