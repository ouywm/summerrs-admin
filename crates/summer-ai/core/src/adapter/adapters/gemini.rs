//! Google Gemini GenerateContent adapter。
//!
//! canonical ↔ Gemini wire 双向转换，挂 `AdapterDispatcher` 用。
//!
//! # URL 形态
//!
//! `{base}/v1beta/models/{actual_model}:generateContent?key={api_key}`
//! 流式则 `:streamGenerateContent?alt=sse&key={api_key}`（用 SSE 分隔）。
//!
//! # 鉴权
//!
//! 默认 API key 作 URL query param（`?key=`）。也支持 `x-goog-api-key` header。

use bytes::Bytes;
use reqwest::header::{CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue};

use crate::adapter::{
    Adapter, AdapterKind, AuthStrategy, CostProfile, ServiceType, WebRequestData,
};
use crate::error::{AdapterError, AdapterResult};
use crate::resolver::{Endpoint, ServiceTarget};
use crate::types::ingress_wire::gemini::{
    GeminiChatResponse, GeminiContent, GeminiFileData, GeminiFunctionCall, GeminiFunctionResponse,
    GeminiGenerateContentRequest, GeminiGenerationConfig, GeminiInlineData, GeminiPart,
    GeminiUsageMetadata,
};
use crate::types::{
    ChatChoice, ChatMessage, ChatRequest, ChatResponse, ChatStreamEvent, CompletionTokensDetails,
    ContentPart, FinishReason, ImageUrl, MessageContent, PromptTokensDetails, Role, StreamEnd,
    ToolCall, ToolCallDelta, ToolCallFunction, ToolFunction, Usage,
};

/// Google Gemini GenerateContent 协议（`generativelanguage.googleapis.com`）。
pub struct GeminiAdapter;

impl GeminiAdapter {
    const BASE_URL: &'static str = "https://generativelanguage.googleapis.com/v1beta/";
}

impl Adapter for GeminiAdapter {
    const KIND: AdapterKind = AdapterKind::Gemini;

    fn default_endpoint() -> Option<Endpoint> {
        Some(Endpoint::from_static(Self::BASE_URL))
    }

    fn auth_strategy() -> AuthStrategy {
        AuthStrategy::QueryParam("key")
    }

    fn cost_profile() -> CostProfile {
        CostProfile::default()
    }

    fn build_chat_request(
        target: &ServiceTarget,
        _service: ServiceType,
        req: &ChatRequest,
    ) -> AdapterResult<WebRequestData> {
        let method = if req.stream {
            "streamGenerateContent"
        } else {
            "generateContent"
        };

        let api_key = target.auth.resolve()?.unwrap_or_default();
        let url = build_gemini_url(
            target.endpoint.trimmed(),
            target.actual_model(),
            method,
            req.stream,
            &api_key,
        );

        let wire = canonical_to_gemini_request(req)?;
        let payload = serde_json::to_value(&wire).map_err(AdapterError::SerializeRequest)?;
        let headers = build_headers(target)?;

        Ok(WebRequestData {
            url,
            headers,
            payload,
        })
    }

    fn parse_chat_response(_target: &ServiceTarget, body: Bytes) -> AdapterResult<ChatResponse> {
        let resp: GeminiChatResponse =
            serde_json::from_slice(&body).map_err(AdapterError::DeserializeResponse)?;
        Ok(gemini_response_to_canonical(resp))
    }

    fn parse_chat_stream_event(
        _target: &ServiceTarget,
        raw: &str,
    ) -> AdapterResult<Vec<ChatStreamEvent>> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Ok(Vec::new());
        }
        let chunk: GeminiChatResponse =
            serde_json::from_str(trimmed).map_err(AdapterError::DeserializeResponse)?;
        Ok(gemini_chunk_to_canonical(chunk))
    }
}

// ---------------------------------------------------------------------------
// canonical → Gemini wire (request)
// ---------------------------------------------------------------------------

fn canonical_to_gemini_request(req: &ChatRequest) -> AdapterResult<GeminiGenerateContentRequest> {
    let mut contents: Vec<GeminiContent> = Vec::new();
    let mut system_texts: Vec<String> = Vec::new();

    // 收集 tool_call_id → name 映射，functionResponse 需要 name
    let mut tool_name_map: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for msg in &req.messages {
        if let Some(tcs) = &msg.tool_calls {
            for tc in tcs {
                tool_name_map.insert(tc.id.clone(), tc.function.name.clone());
            }
        }
    }

    for msg in &req.messages {
        match msg.role {
            Role::System | Role::Developer => {
                if let Some(text) = message_text(msg)
                    && !text.is_empty()
                {
                    system_texts.push(text);
                }
            }
            Role::User => {
                let parts = canonical_content_to_gemini_parts(msg);
                contents.push(GeminiContent {
                    role: Some("user".to_string()),
                    parts,
                });
            }
            Role::Assistant => {
                let mut parts = canonical_content_to_gemini_parts(msg);
                if let Some(tool_calls) = &msg.tool_calls {
                    for tc in tool_calls {
                        let args =
                            serde_json::from_str::<serde_json::Value>(&tc.function.arguments)
                                .unwrap_or_else(|_| serde_json::Value::Object(Default::default()));
                        parts.push(GeminiPart::FunctionCall {
                            function_call: GeminiFunctionCall {
                                name: tc.function.name.clone(),
                                args,
                            },
                        });
                    }
                }
                contents.push(GeminiContent {
                    role: Some("model".to_string()),
                    parts,
                });
            }
            Role::Tool => {
                // tool_call_id → name 反查
                let name = msg
                    .tool_call_id
                    .as_ref()
                    .and_then(|id| tool_name_map.get(id).cloned())
                    .unwrap_or_else(|| "unknown".to_string());
                let text = message_text(msg).unwrap_or_default();
                let response: serde_json::Value =
                    serde_json::from_str(&text).unwrap_or(serde_json::Value::String(text));
                contents.push(GeminiContent {
                    role: Some("user".to_string()),
                    parts: vec![GeminiPart::FunctionResponse {
                        function_response: GeminiFunctionResponse { name, response },
                    }],
                });
            }
        }
    }

    let system_instruction = if system_texts.is_empty() {
        None
    } else {
        let combined = system_texts.join("\n");
        Some(
            crate::types::ingress_wire::gemini::GeminiSystemInstruction {
                parts: vec![GeminiPart::plain_text(combined)],
            },
        )
    };

    // tools：Gemini 的 tools[] 是 key-based 平面结构——function tool 合并进单个
    // GeminiTool.functionDeclarations；built-in（web_search / url_context /
    // code_execution）按 kind 分派到对应字段；mcp / 未知 kind 无对应字段 warn 丢弃
    // （Gemini 对未知 tool 字段严格 400）。
    let tools = if let Some(canonical_tools) = &req.tools {
        build_gemini_tools(canonical_tools)
    } else {
        Vec::new()
    };

    // generationConfig
    let stop_sequences = match &req.stop {
        Some(serde_json::Value::String(s)) => vec![s.clone()],
        Some(serde_json::Value::Array(arr)) => arr
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    };
    let generation_config = Some(GeminiGenerationConfig {
        temperature: req.temperature,
        top_p: req.top_p,
        top_k: req.extra.get("top_k").and_then(|v| v.as_i64()),
        candidate_count: req.n,
        max_output_tokens: req.max_tokens.or(req.max_completion_tokens),
        stop_sequences,
        response_mime_type: None,
        response_schema: None,
        thinking_config: None,
        extra: serde_json::Map::new(),
    });

    Ok(GeminiGenerateContentRequest {
        contents,
        system_instruction,
        tools,
        tool_config: None,
        generation_config,
        safety_settings: Vec::new(),
        extra: serde_json::Map::new(),
    })
}

fn gemini_function_declaration(
    f: &ToolFunction,
) -> crate::types::ingress_wire::gemini::GeminiFunctionDeclaration {
    crate::types::ingress_wire::gemini::GeminiFunctionDeclaration {
        name: f.name.clone(),
        description: f.description.clone(),
        parameters: f.parameters.clone(),
    }
}

/// canonical `tools: Vec<Tool>` → Gemini `tools: Vec<GeminiTool>`。
///
/// Gemini 的 tool 对象是 key-based 平面结构，多个 built-in 可以共存在一个对象里。
/// 策略：**最小 key 映射 + 其余透传**。
///
/// - 所有 function tool 合并到 **单个** `GeminiTool.function_declarations`
/// - `web_search*` / `google_search*` / `googleSearch` → `googleSearch: {...}`
///   （跨 provider 方言翻译：OpenAI `web_search_preview` / Claude
///   `web_search_20250305` 客户端路由到 Gemini 时能自动命中）
/// - `url_context*` / `urlContext` → `extra["urlContext"] = {...}`
/// - `code_execution` / `code_interpreter` / `codeExecution` → `codeExecution: {...}`
/// - **其他任意 kind（包括 `mcp*`、未来新 built-in、客户端自定义字符串）**：
///   把 `kind` 原样作为 wire key 写进 extra，`t.extra` 作为 value。Gemini 不认
///   就 400 返给客户端——这是客户端传未知 kind 的代价，relay 不替它决定。
///
/// 如果全部都是 function tool，输出 **单个** GeminiTool；如果混合了 built-in，
/// function_declarations 和 built-in 字段共存在同一个对象里（Gemini 接受）。
fn build_gemini_tools(
    canonical_tools: &[crate::types::Tool],
) -> Vec<crate::types::ingress_wire::gemini::GeminiTool> {
    use crate::types::ingress_wire::gemini::{GeminiTool, kind_prefix, wire_key};

    let mut tool = GeminiTool {
        function_declarations: Vec::new(),
        google_search: None,
        code_execution: None,
        extra: serde_json::Map::new(),
    };

    for t in canonical_tools {
        if t.is_function() {
            if let Some(f) = t.function.as_ref() {
                tool.function_declarations
                    .push(gemini_function_declaration(f));
            }
            continue;
        }
        let k = t.kind.as_str();
        let value = serde_json::Value::Object(t.extra.clone());
        if k.starts_with(kind_prefix::WEB_SEARCH)
            || k.starts_with(kind_prefix::GOOGLE_SEARCH)
            || k == kind_prefix::GOOGLE_SEARCH_CAMEL
        {
            tool.google_search = Some(value);
        } else if k.starts_with(kind_prefix::URL_CONTEXT) || k == kind_prefix::URL_CONTEXT_CAMEL {
            tool.extra.insert(wire_key::URL_CONTEXT.to_string(), value);
        } else if k == kind_prefix::CODE_EXECUTION
            || k == kind_prefix::CODE_INTERPRETER
            || k == kind_prefix::CODE_EXECUTION_CAMEL
        {
            tool.code_execution = Some(value);
        } else {
            // 未知 kind：原样作为 wire key 写入 extra，让 Gemini 自己判断。
            tool.extra.insert(k.to_string(), value);
        }
    }

    if tool.function_declarations.is_empty()
        && tool.google_search.is_none()
        && tool.code_execution.is_none()
        && tool.extra.is_empty()
    {
        Vec::new()
    } else {
        vec![tool]
    }
}

fn canonical_content_to_gemini_parts(msg: &ChatMessage) -> Vec<GeminiPart> {
    let Some(content) = msg.content.as_ref() else {
        return Vec::new();
    };
    match content {
        MessageContent::Text(s) if s.is_empty() => Vec::new(),
        MessageContent::Text(s) => vec![GeminiPart::plain_text(s.clone())],
        MessageContent::Parts(parts) => parts
            .iter()
            .filter_map(|p| match p {
                ContentPart::Text { text } => Some(GeminiPart::plain_text(text.clone())),
                ContentPart::ImageUrl { image_url } => Some(canonical_image_to_gemini(image_url)),
                ContentPart::InputAudio { .. } => None, // Gemini 没有 inline audio（应走 fileData）
            })
            .collect(),
    }
}

fn canonical_image_to_gemini(image_url: &ImageUrl) -> GeminiPart {
    if let Some(stripped) = image_url.url.strip_prefix("data:")
        && let Some((meta, data)) = stripped.split_once(",")
    {
        let mime_type = meta.split(';').next().unwrap_or("image/png").to_string();
        return GeminiPart::InlineData {
            inline_data: GeminiInlineData {
                mime_type,
                data: data.to_string(),
            },
        };
    }
    GeminiPart::FileData {
        file_data: GeminiFileData {
            mime_type: "application/octet-stream".to_string(),
            file_uri: image_url.url.clone(),
        },
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
// Gemini wire → canonical (response)
// ---------------------------------------------------------------------------

fn gemini_response_to_canonical(resp: GeminiChatResponse) -> ChatResponse {
    let GeminiChatResponse {
        candidates,
        usage_metadata,
        model_version,
        ..
    } = resp;

    let choices = candidates
        .into_iter()
        .map(gemini_candidate_to_choice)
        .collect();

    ChatResponse {
        id: format!("chatcmpl-gemini-{}", timestamp_id()),
        object: "chat.completion".to_string(),
        created: 0,
        model: model_version.unwrap_or_else(|| "gemini".to_string()),
        choices,
        usage: usage_metadata
            .map(gemini_usage_to_canonical)
            .unwrap_or_default(),
        system_fingerprint: None,
        service_tier: None,
    }
}

fn gemini_candidate_to_choice(
    c: crate::types::ingress_wire::gemini::GeminiCandidate,
) -> ChatChoice {
    let mut text_buf = String::new();
    let mut reasoning_buf = String::new();
    let mut tool_calls: Vec<ToolCall> = Vec::new();
    let mut tc_counter: u32 = 0;
    // Gemini 3+ multi-turn tool-use 续接约定：上游在 parts 上挂 `thoughtSignature`,
    // 下一轮客户端必须把这些 signature 连同 tool_calls 一起回传，否则思考状态丢失。
    // 收集所有 part 上出现的 signature（正文 Text、thought=true 的 reasoning Text、
    // 独立 ThoughtSignature part），non-stream response 里 canonical ChatMessage
    // 没有 per-message signature 字段，统一挂到首个 tool_call.thought_signatures。
    let mut thought_signatures: Vec<String> = Vec::new();

    if let Some(content) = c.content {
        for part in content.parts {
            match part {
                // Gemini 2.5 legacy：`thought=true` 表示这段 text 是思考链；
                // 之前按普通 text 拼进 content，reasoning 污染了客户端输出。
                GeminiPart::Text {
                    text,
                    thought: true,
                    thought_signature,
                } => {
                    if !reasoning_buf.is_empty() {
                        reasoning_buf.push('\n');
                    }
                    reasoning_buf.push_str(&text);
                    if let Some(sig) = thought_signature {
                        thought_signatures.push(sig);
                    }
                }
                GeminiPart::Text {
                    text,
                    thought_signature,
                    ..
                } => {
                    if !text_buf.is_empty() {
                        text_buf.push('\n');
                    }
                    text_buf.push_str(&text);
                    if let Some(sig) = thought_signature {
                        thought_signatures.push(sig);
                    }
                }
                GeminiPart::ThoughtSignature { thought_signature } => {
                    thought_signatures.push(thought_signature);
                }
                GeminiPart::FunctionCall { function_call } => {
                    tc_counter += 1;
                    let GeminiFunctionCall { name, args } = function_call;
                    let arguments =
                        serde_json::to_string(&args).unwrap_or_else(|_| "{}".to_string());
                    tool_calls.push(ToolCall {
                        id: format!("call_{tc_counter}"),
                        kind: "function".to_string(),
                        function: ToolCallFunction { name, arguments },
                        thought_signatures: None,
                    });
                }
                _ => {}
            }
        }
    }

    // Gemini 3+ signature 挂到首个 tool_call（Claude adapter 同样做法）。
    // 只有在存在 tool_calls 时才挂 —— 没有 tool_calls 的 assistant 消息续接场景
    // 很罕见，canonical 层暂不新增独立字段。
    if !thought_signatures.is_empty()
        && let Some(first) = tool_calls.first_mut()
    {
        first.thought_signatures = Some(thought_signatures);
    }

    ChatChoice {
        index: c.index,
        message: ChatMessage {
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
            native_content_blocks: None,
            options: None,
        },
        logprobs: None,
        finish_reason: c
            .finish_reason
            .as_deref()
            .and_then(gemini_finish_reason_to_canonical),
    }
}

fn gemini_finish_reason_to_canonical(reason: &str) -> Option<FinishReason> {
    match reason {
        "STOP" => Some(FinishReason::Stop),
        "MAX_TOKENS" => Some(FinishReason::Length),
        "SAFETY" | "RECITATION" | "BLOCKLIST" | "PROHIBITED_CONTENT" | "SPII" => {
            Some(FinishReason::ContentFilter)
        }
        "MALFORMED_FUNCTION_CALL" | "UNEXPECTED_TOOL_CALL" => Some(FinishReason::ToolCalls),
        _ => Some(FinishReason::Stop),
    }
}

fn gemini_usage_to_canonical(m: GeminiUsageMetadata) -> Usage {
    // Gemini 2.5 用 `thoughtsTokenCount` 暴露思考消耗，必须映射到 canonical
    // `completion_tokens_details.reasoning_tokens`（与 OpenAI o1 对齐），billing
    // 才能按 reasoning 分桶计费。另外 Gemini 的 `candidatesTokenCount` 已经包含
    // thoughtsTokenCount（文档要求），所以不要再加一次到 completion_tokens。
    let completion_tokens_details = m.thoughts_token_count.map(|t| CompletionTokensDetails {
        reasoning_tokens: Some(t),
        audio_tokens: None,
        accepted_prediction_tokens: None,
        rejected_prediction_tokens: None,
    });

    Usage {
        prompt_tokens: m.prompt_token_count,
        completion_tokens: m.candidates_token_count,
        total_tokens: m.total_token_count,
        prompt_tokens_details: m.cached_content_token_count.map(|c| PromptTokensDetails {
            cached_tokens: Some(c),
            cache_creation_tokens: None,
            audio_tokens: None,
        }),
        completion_tokens_details,
    }
}

// ---------------------------------------------------------------------------
// Gemini stream chunk → canonical event
// ---------------------------------------------------------------------------

fn gemini_chunk_to_canonical(chunk: GeminiChatResponse) -> Vec<ChatStreamEvent> {
    // Gemini 允许单个 chunk 包含：多个 text part、多个并行 functionCall part、
    // 以及 finishReason / usageMetadata——对齐 rust-genai gemini/streamer.rs 用
    // pending_events 队列的做法：一次 chunk 产出一个事件列表，按 text → tool_calls →
    // End 的顺序 emit，不因为 finish_reason 在场就吞掉同块里的内容。
    let candidate = chunk.candidates.into_iter().next();
    let usage_meta = chunk.usage_metadata;

    let Some(candidate) = candidate else {
        // 只有 usage 无 candidate → End（usage_metadata-only chunk）
        if let Some(u) = usage_meta {
            return vec![ChatStreamEvent::End(StreamEnd {
                finish_reason: None,
                usage: Some(gemini_usage_to_canonical(u)),
            })];
        }
        return Vec::new();
    };

    let finish_reason_raw = candidate.finish_reason.clone();

    // 先把 parts 里的 text / reasoning / signature / functionCall 全部分桶。
    // Gemini 一个 chunk 内可能同时含多种 part（思考中 + 部分正文 + signature），
    // 必须全部 emit，不能丢任何一种。
    let parts = candidate.content.map(|c| c.parts).unwrap_or_default();
    let mut text_buf = String::new();
    let mut reasoning_buf = String::new();
    let mut thought_signatures: Vec<String> = Vec::new();
    let mut function_calls: Vec<GeminiFunctionCall> = Vec::new();
    for part in parts {
        match part {
            // Gemini 2.5 legacy：`thought=true` 的 text 是思考链，不是正文。
            GeminiPart::Text {
                text,
                thought: true,
                thought_signature,
            } => {
                if !reasoning_buf.is_empty() {
                    reasoning_buf.push('\n');
                }
                reasoning_buf.push_str(&text);
                if let Some(sig) = thought_signature {
                    thought_signatures.push(sig);
                }
            }
            // 正文 text part，可能同时挂 signature（Gemini 3+）。
            GeminiPart::Text {
                text,
                thought_signature,
                ..
            } => {
                if !text_buf.is_empty() {
                    text_buf.push('\n');
                }
                text_buf.push_str(&text);
                if let Some(sig) = thought_signature {
                    thought_signatures.push(sig);
                }
            }
            GeminiPart::ThoughtSignature { thought_signature } => {
                thought_signatures.push(thought_signature);
            }
            GeminiPart::FunctionCall { function_call } => {
                function_calls.push(function_call);
            }
            _ => {}
        }
    }

    let mut events: Vec<ChatStreamEvent> = Vec::new();

    if !reasoning_buf.is_empty() {
        events.push(ChatStreamEvent::ReasoningDelta {
            text: reasoning_buf,
        });
    }
    if !text_buf.is_empty() {
        events.push(ChatStreamEvent::TextDelta { text: text_buf });
    }
    for signature in thought_signatures {
        events.push(ChatStreamEvent::ThoughtSignature { signature });
    }

    for (idx, fc) in function_calls.into_iter().enumerate() {
        let args = serde_json::to_string(&fc.args).unwrap_or_else(|_| "{}".to_string());
        events.push(ChatStreamEvent::ToolCallDelta(ToolCallDelta {
            index: idx as i32,
            id: Some(format!("call_{}", timestamp_id())),
            name: Some(fc.name),
            arguments_delta: Some(args),
        }));
    }

    // finishReason 非空：即使同块有内容也要 emit End 收尾（Gemini 常把最后 text
    // 和 finishReason 一起发）
    if let Some(reason) = finish_reason_raw {
        events.push(ChatStreamEvent::End(StreamEnd {
            finish_reason: gemini_finish_reason_to_canonical(&reason),
            usage: usage_meta.map(gemini_usage_to_canonical),
        }));
    }

    events
}

// ---------------------------------------------------------------------------
// URL / Headers
// ---------------------------------------------------------------------------

fn build_gemini_url(base: &str, model: &str, method: &str, stream: bool, api_key: &str) -> String {
    let base = base.trim_end_matches('/');
    let mut url = if base.ends_with("/v1beta") || base.contains("/v1beta/") {
        format!("{base}/models/{model}:{method}")
    } else {
        format!("{base}/v1beta/models/{model}:{method}")
    };
    let sep = if url.contains('?') { '&' } else { '?' };
    if stream {
        url.push(sep);
        url.push_str("alt=sse");
        if !api_key.is_empty() {
            url.push_str("&key=");
            url.push_str(api_key);
        }
    } else if !api_key.is_empty() {
        url.push(sep);
        url.push_str("key=");
        url.push_str(api_key);
    }
    url
}

fn build_headers(target: &ServiceTarget) -> AdapterResult<HeaderMap> {
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

    for (name, value) in &target.extra_headers {
        let name = HeaderName::try_from(name.as_str())
            .map_err(|e| AdapterError::InvalidHeader(e.to_string()))?;
        let value = HeaderValue::from_str(value.as_str())
            .map_err(|e| AdapterError::InvalidHeader(e.to_string()))?;
        headers.insert(name, value);
    }
    Ok(headers)
}

fn timestamp_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{nanos:x}")
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
            AdapterKind::Gemini,
            "https://generativelanguage.googleapis.com",
            "AIza-fake-key",
            "gemini-2.5-flash",
        )
    }

    #[test]
    fn url_non_stream_uses_generate_content_with_key() {
        let t = target();
        let req = ChatRequest::new("alias", vec![ChatMessage::user("hi")]);
        let data = GeminiAdapter::build_chat_request(&t, ServiceType::Chat, &req).unwrap();
        assert!(
            data.url
                .contains("/v1beta/models/gemini-2.5-flash:generateContent")
        );
        assert!(data.url.contains("key=AIza-fake-key"));
    }

    #[test]
    fn url_stream_adds_alt_sse() {
        let t = target();
        let mut req = ChatRequest::new("alias", vec![ChatMessage::user("hi")]);
        req.stream = true;
        let data = GeminiAdapter::build_chat_request(&t, ServiceType::Chat, &req).unwrap();
        assert!(data.url.contains(":streamGenerateContent"));
        assert!(data.url.contains("alt=sse"));
    }

    #[test]
    fn system_messages_become_system_instruction() {
        let t = target();
        let req = ChatRequest::new(
            "x",
            vec![
                ChatMessage::system("you are helpful"),
                ChatMessage::user("hi"),
            ],
        );
        let data = GeminiAdapter::build_chat_request(&t, ServiceType::Chat, &req).unwrap();
        let sys = &data.payload["systemInstruction"];
        assert_eq!(sys["parts"][0]["text"], "you are helpful");
        let contents = data.payload["contents"].as_array().unwrap();
        assert_eq!(contents.len(), 1);
        assert_eq!(contents[0]["role"], "user");
    }

    #[test]
    fn role_mapping_user_assistant() {
        let t = target();
        let req = ChatRequest::new(
            "x",
            vec![ChatMessage::user("hi"), ChatMessage::assistant("hello")],
        );
        let data = GeminiAdapter::build_chat_request(&t, ServiceType::Chat, &req).unwrap();
        let contents = data.payload["contents"].as_array().unwrap();
        assert_eq!(contents[0]["role"], "user");
        assert_eq!(contents[1]["role"], "model");
    }

    #[test]
    fn assistant_tool_calls_emit_function_call_parts() {
        let t = target();
        let req = ChatRequest::new(
            "x",
            vec![ChatMessage {
                role: Role::Assistant,
                content: Some(MessageContent::Text("let me check".to_string())),
                reasoning_content: None,
                refusal: None,
                name: None,
                tool_calls: Some(vec![ToolCall {
                    id: "call_1".to_string(),
                    kind: "function".to_string(),
                    function: ToolCallFunction {
                        name: "weather".to_string(),
                        arguments: r#"{"city":"NYC"}"#.to_string(),
                    },
                    thought_signatures: None,
                }]),
                tool_call_id: None,
                audio: None,
                native_content_blocks: None,
                options: None,
            }],
        );
        let data = GeminiAdapter::build_chat_request(&t, ServiceType::Chat, &req).unwrap();
        let parts = data.payload["contents"][0]["parts"].as_array().unwrap();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0]["text"], "let me check");
        assert_eq!(parts[1]["functionCall"]["name"], "weather");
        assert_eq!(parts[1]["functionCall"]["args"]["city"], "NYC");
    }

    #[test]
    fn tool_response_becomes_function_response_part() {
        let t = target();
        let req = ChatRequest::new(
            "x",
            vec![
                ChatMessage {
                    role: Role::Assistant,
                    content: None,
                    reasoning_content: None,
                    refusal: None,
                    name: None,
                    tool_calls: Some(vec![ToolCall {
                        id: "call_1".to_string(),
                        kind: "function".to_string(),
                        function: ToolCallFunction {
                            name: "weather".to_string(),
                            arguments: "{}".to_string(),
                        },
                        thought_signatures: None,
                    }]),
                    tool_call_id: None,
                    audio: None,
                    native_content_blocks: None,
                    options: None,
                },
                ChatMessage::tool_response("call_1", r#"{"temp":"72F"}"#),
            ],
        );
        let data = GeminiAdapter::build_chat_request(&t, ServiceType::Chat, &req).unwrap();
        let contents = data.payload["contents"].as_array().unwrap();
        assert_eq!(contents.len(), 2);
        let tool_part = &contents[1]["parts"][0];
        assert_eq!(tool_part["functionResponse"]["name"], "weather");
        assert_eq!(tool_part["functionResponse"]["response"]["temp"], "72F");
    }

    #[test]
    fn image_data_uri_becomes_inline_data() {
        let t = target();
        let req = ChatRequest::new(
            "x",
            vec![ChatMessage {
                role: Role::User,
                content: Some(MessageContent::Parts(vec![
                    ContentPart::Text {
                        text: "describe".to_string(),
                    },
                    ContentPart::ImageUrl {
                        image_url: ImageUrl {
                            url: "data:image/png;base64,XYZ".to_string(),
                            detail: None,
                        },
                    },
                ])),
                reasoning_content: None,
                refusal: None,
                name: None,
                tool_calls: None,
                tool_call_id: None,
                audio: None,
                native_content_blocks: None,
                options: None,
            }],
        );
        let data = GeminiAdapter::build_chat_request(&t, ServiceType::Chat, &req).unwrap();
        let parts = data.payload["contents"][0]["parts"].as_array().unwrap();
        assert_eq!(parts[1]["inlineData"]["mimeType"], "image/png");
        assert_eq!(parts[1]["inlineData"]["data"], "XYZ");
    }

    #[test]
    fn parse_response_basic() {
        let t = target();
        let body = br#"{
            "candidates":[{
                "index":0,
                "content":{"role":"model","parts":[{"text":"hello"}]},
                "finishReason":"STOP"
            }],
            "usageMetadata":{
                "promptTokenCount":3,"candidatesTokenCount":5,"totalTokenCount":8,
                "cachedContentTokenCount":2
            },
            "modelVersion":"gemini-2.5-flash"
        }"#;
        let resp = GeminiAdapter::parse_chat_response(&t, Bytes::from_static(body)).unwrap();
        assert_eq!(resp.first_text(), Some("hello"));
        assert_eq!(resp.usage.prompt_tokens, 3);
        assert_eq!(resp.usage.total_tokens, 8);
        assert_eq!(
            resp.usage
                .prompt_tokens_details
                .as_ref()
                .unwrap()
                .cached_tokens,
            Some(2)
        );
        assert_eq!(resp.choices[0].finish_reason, Some(FinishReason::Stop));
    }

    #[test]
    fn parse_response_function_call() {
        let t = target();
        let body = br#"{
            "candidates":[{
                "index":0,
                "content":{"role":"model","parts":[
                    {"functionCall":{"name":"weather","args":{"city":"NYC"}}}
                ]},
                "finishReason":"STOP"
            }],
            "modelVersion":"gemini-2.5-flash"
        }"#;
        let resp = GeminiAdapter::parse_chat_response(&t, Bytes::from_static(body)).unwrap();
        let tcs = resp.choices[0].message.tool_calls.as_ref().unwrap();
        assert_eq!(tcs[0].function.name, "weather");
        assert!(tcs[0].function.arguments.contains("NYC"));
    }

    #[test]
    fn stream_chunk_text_delta() {
        let t = target();
        let raw = r#"{
            "candidates":[{"index":0,"content":{"role":"model","parts":[{"text":"hi"}]}}]
        }"#;
        let e = GeminiAdapter::parse_chat_stream_event(&t, raw)
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
    fn stream_chunk_with_finish_reason_emits_end() {
        let t = target();
        let raw = r#"{
            "candidates":[{
                "index":0,
                "content":{"role":"model","parts":[]},
                "finishReason":"STOP"
            }],
            "usageMetadata":{"promptTokenCount":1,"candidatesTokenCount":2,"totalTokenCount":3}
        }"#;
        let e = GeminiAdapter::parse_chat_stream_event(&t, raw)
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        match e {
            ChatStreamEvent::End(end) => {
                assert_eq!(end.finish_reason, Some(FinishReason::Stop));
                assert_eq!(end.usage.as_ref().unwrap().total_tokens, 3);
            }
            _ => panic!(),
        }
    }

    #[test]
    fn finish_reason_safety_maps_to_content_filter() {
        assert_eq!(
            gemini_finish_reason_to_canonical("SAFETY"),
            Some(FinishReason::ContentFilter)
        );
        assert_eq!(
            gemini_finish_reason_to_canonical("MAX_TOKENS"),
            Some(FinishReason::Length)
        );
    }

    #[test]
    fn stream_chunk_text_and_finish_reason_emit_both_events() {
        // 回归：Gemini 经常把最后一段 text 和 finishReason 打包在同一 chunk
        // （等同 Mistral / 规则 6）。之前 finishReason 存在就直接 return End，
        // text 被丢；修复后必须同时 emit TextDelta + End。
        let t = target();
        let raw = r#"{
            "candidates":[{
                "index":0,
                "content":{"role":"model","parts":[{"text":"end."}]},
                "finishReason":"STOP"
            }]
        }"#;
        let events = GeminiAdapter::parse_chat_stream_event(&t, raw).unwrap();
        assert_eq!(events.len(), 2, "expected TextDelta + End, got {events:?}");
        match &events[0] {
            ChatStreamEvent::TextDelta { text } => assert_eq!(text, "end."),
            other => panic!("expected TextDelta first, got {other:?}"),
        }
        match &events[1] {
            ChatStreamEvent::End(end) => assert_eq!(end.finish_reason, Some(FinishReason::Stop)),
            other => panic!("expected End last, got {other:?}"),
        }
    }

    #[test]
    fn stream_chunk_parallel_function_calls_emit_all() {
        // 回归：Gemini parallel function calling 单块可含多个 functionCall part；
        // 之前的实现只取首个，其余并行调用被丢弃。
        let t = target();
        let raw = r#"{
            "candidates":[{
                "index":0,
                "content":{"role":"model","parts":[
                    {"functionCall":{"name":"fa","args":{"x":1}}},
                    {"functionCall":{"name":"fb","args":{"y":2}}}
                ]}
            }]
        }"#;
        let events = GeminiAdapter::parse_chat_stream_event(&t, raw).unwrap();
        assert_eq!(events.len(), 2);
        for (expected_idx, expected_name, ev) in [(0i32, "fa", &events[0]), (1, "fb", &events[1])] {
            match ev {
                ChatStreamEvent::ToolCallDelta(d) => {
                    assert_eq!(d.index, expected_idx);
                    assert_eq!(d.name.as_deref(), Some(expected_name));
                }
                other => panic!("expected ToolCallDelta, got {other:?}"),
            }
        }
    }

    #[test]
    fn stream_thought_true_part_becomes_reasoning_delta() {
        // Gemini 2.5 legacy：`{"text":"...","thought":true}` 这段 text 是思考链，
        // 必须 emit ReasoningDelta 而不是 TextDelta —— 否则客户端正文里混入思考内容。
        let t = target();
        let raw = r#"{
            "candidates":[{
                "index":0,
                "content":{"role":"model","parts":[
                    {"text":"let me think","thought":true}
                ]}
            }]
        }"#;
        let events = GeminiAdapter::parse_chat_stream_event(&t, raw).unwrap();
        assert_eq!(events.len(), 1);
        match &events[0] {
            ChatStreamEvent::ReasoningDelta { text } => assert_eq!(text, "let me think"),
            other => panic!("expected ReasoningDelta, got {other:?}"),
        }
    }

    #[test]
    fn stream_thought_signature_only_part_is_emitted() {
        // Gemini 3+ 的 signature-only part（无 text）必须被识别并透传，
        // 不然 multi-turn thinking 续接时客户端没法回传 signature → 上游 400。
        let t = target();
        let raw = r#"{
            "candidates":[{
                "index":0,
                "content":{"role":"model","parts":[
                    {"thoughtSignature":"sig-xyz"}
                ]}
            }]
        }"#;
        let events = GeminiAdapter::parse_chat_stream_event(&t, raw).unwrap();
        assert_eq!(events.len(), 1);
        match &events[0] {
            ChatStreamEvent::ThoughtSignature { signature } => {
                assert_eq!(signature, "sig-xyz");
            }
            other => panic!("expected ThoughtSignature, got {other:?}"),
        }
    }

    #[test]
    fn stream_text_part_with_attached_signature_emits_both() {
        // Gemini 3+ 可能把 signature 直接挂在正文 text part 上：
        // `{"text":"hello","thoughtSignature":"sig"}`。text 要当正文，signature 单独 emit。
        let t = target();
        let raw = r#"{
            "candidates":[{
                "index":0,
                "content":{"role":"model","parts":[
                    {"text":"hello","thoughtSignature":"sig-attached"}
                ]}
            }]
        }"#;
        let events = GeminiAdapter::parse_chat_stream_event(&t, raw).unwrap();
        assert_eq!(events.len(), 2);
        assert!(matches!(events[0], ChatStreamEvent::TextDelta { ref text } if text == "hello"));
        assert!(matches!(
            events[1],
            ChatStreamEvent::ThoughtSignature { ref signature } if signature == "sig-attached"
        ));
    }

    #[test]
    fn non_stream_thought_true_part_goes_to_reasoning_content() {
        // 非流式 response：`thought=true` 的 text 进 message.reasoning_content，
        // content 保留正文。
        let t = target();
        let body = br#"{
            "candidates":[{
                "index":0,
                "content":{"role":"model","parts":[
                    {"text":"internal thought","thought":true},
                    {"text":"public answer"}
                ]},
                "finishReason":"STOP"
            }],
            "usageMetadata":{"promptTokenCount":5,"candidatesTokenCount":2,"totalTokenCount":7}
        }"#;
        let resp = GeminiAdapter::parse_chat_response(&t, Bytes::from_static(body)).unwrap();
        let msg = &resp.choices[0].message;
        assert_eq!(msg.reasoning_content.as_deref(), Some("internal thought"));
        match msg.content.as_ref().unwrap() {
            MessageContent::Text(t) => assert_eq!(t, "public answer"),
            _ => panic!("expected Text"),
        }
    }

    #[test]
    fn usage_thoughts_token_count_maps_to_reasoning_tokens() {
        // Gemini 2.5 的 usageMetadata.thoughtsTokenCount 是思考消耗，要映射到
        // canonical completion_tokens_details.reasoning_tokens（和 OpenAI o1 对齐）。
        // candidatesTokenCount 按 Gemini 文档已经包含 thoughtsTokenCount，
        // 所以 completion_tokens 保持原值、不再叠加。
        let t = target();
        let body = br#"{
            "candidates":[{
                "index":0,
                "content":{"role":"model","parts":[{"text":"answer"}]},
                "finishReason":"STOP"
            }],
            "usageMetadata":{
                "promptTokenCount":10,
                "candidatesTokenCount":50,
                "thoughtsTokenCount":35,
                "totalTokenCount":60
            }
        }"#;
        let resp = GeminiAdapter::parse_chat_response(&t, Bytes::from_static(body)).unwrap();
        assert_eq!(resp.usage.prompt_tokens, 10);
        assert_eq!(resp.usage.completion_tokens, 50); // 不把 35 加一次
        let details =
            resp.usage.completion_tokens_details.as_ref().expect(
                "completion_tokens_details should be present when thoughts_token_count is set",
            );
        assert_eq!(details.reasoning_tokens, Some(35));
    }

    #[test]
    fn usage_without_thoughts_token_count_keeps_details_none() {
        // 上游不带 thoughtsTokenCount 时，completion_tokens_details 维持 None，
        // 不要发 `{reasoning_tokens: None}` 的空 wrapper —— billing 根据存在性判断
        // 是否有 thinking 消耗。
        let t = target();
        let body = br#"{
            "candidates":[{
                "index":0,
                "content":{"role":"model","parts":[{"text":"hi"}]},
                "finishReason":"STOP"
            }],
            "usageMetadata":{"promptTokenCount":5,"candidatesTokenCount":2,"totalTokenCount":7}
        }"#;
        let resp = GeminiAdapter::parse_chat_response(&t, Bytes::from_static(body)).unwrap();
        assert!(resp.usage.completion_tokens_details.is_none());
    }

    #[test]
    fn non_stream_thought_signatures_collected_into_first_tool_call() {
        // Gemini 3 multi-turn tool-use：response 的 parts 里带 thoughtSignature，
        // 客户端构造下一轮请求时必须把 signature 回传；canonical ChatMessage
        // 没 per-message signature 字段，adapter 统一挂到首个 tool_call。
        // 这里覆盖三种 signature 来源：正文 Text、reasoning Text(thought=true)、
        // 独立的 thoughtSignature-only part。
        let t = target();
        let body = br#"{
            "candidates":[{
                "index":0,
                "content":{"role":"model","parts":[
                    {"text":"I'll check the weather","thoughtSignature":"sig-text"},
                    {"text":"reasoning step","thought":true,"thoughtSignature":"sig-reason"},
                    {"thoughtSignature":"sig-only"},
                    {"functionCall":{"name":"get_weather","args":{"city":"NYC"}}}
                ]},
                "finishReason":"STOP"
            }],
            "usageMetadata":{"promptTokenCount":5,"candidatesTokenCount":3,"totalTokenCount":8}
        }"#;
        let resp = GeminiAdapter::parse_chat_response(&t, Bytes::from_static(body)).unwrap();
        let msg = &resp.choices[0].message;
        let calls = msg.tool_calls.as_ref().expect("tool_calls should be set");
        assert_eq!(calls.len(), 1);
        let sigs = calls[0]
            .thought_signatures
            .as_ref()
            .expect("thought_signatures should be attached to first tool_call");
        assert_eq!(
            sigs,
            &vec![
                "sig-text".to_string(),
                "sig-reason".to_string(),
                "sig-only".to_string(),
            ]
        );
    }

    #[test]
    fn non_stream_thought_signatures_without_tool_calls_are_dropped() {
        // 没有 tool_calls 的 assistant 响应也可能带 signature（比如纯文字回答
        // 后继续思考），但 canonical 层目前无处落盘、客户端也不需要在文字回答上
        // 带 signature 续接。直接丢掉，保持 ToolCall 成为唯一承载点。
        let t = target();
        let body = br#"{
            "candidates":[{
                "index":0,
                "content":{"role":"model","parts":[
                    {"text":"hi there","thoughtSignature":"sig-stale"}
                ]},
                "finishReason":"STOP"
            }],
            "usageMetadata":{"promptTokenCount":3,"candidatesTokenCount":2,"totalTokenCount":5}
        }"#;
        let resp = GeminiAdapter::parse_chat_response(&t, Bytes::from_static(body)).unwrap();
        let msg = &resp.choices[0].message;
        assert!(msg.tool_calls.is_none());
        match msg.content.as_ref().unwrap() {
            MessageContent::Text(t) => assert_eq!(t, "hi there"),
            _ => panic!("expected Text"),
        }
    }

    // ------------------------------------------------------------------
    // Built-in tool dispatch
    // ------------------------------------------------------------------

    #[test]
    fn web_search_kind_maps_to_google_search_field() {
        // 客户端发 OpenAI `web_search_preview` / Claude `web_search_20250305` /
        // Gemini `googleSearch`：adapter 都要统一落成 Gemini wire 的
        // `tools: [{googleSearch: {}}]`。Gemini 对未知 tool 字段严格 400，
        // 所以这个翻译是跨 provider 路由的硬需求。
        let t = target();
        let mut req = ChatRequest::new("x", vec![ChatMessage::user("what's new?")]);
        req.tools = Some(vec![crate::types::Tool::builtin(
            "web_search_preview",
            serde_json::Map::new(),
        )]);
        let wire = GeminiAdapter::build_chat_request(&t, ServiceType::Chat, &req).unwrap();
        let tools = wire.payload["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 1);
        assert!(tools[0]["googleSearch"].is_object());
    }

    #[test]
    fn function_tools_combine_into_single_gemini_tool() {
        // 多个 function tool 必须合并到单个 GeminiTool.functionDeclarations
        // —— Gemini wire 期望的是一个 tool 对象 + 一个 declarations 数组，而不是
        // 每个 function 一个 tool 对象（会被某些 Gemini endpoint 拒收）。
        let t = target();
        let mut req = ChatRequest::new("x", vec![ChatMessage::user("q")]);
        req.tools = Some(vec![
            crate::types::Tool::function(crate::types::ToolFunction {
                name: "a".to_string(),
                description: None,
                parameters: None,
            }),
            crate::types::Tool::function(crate::types::ToolFunction {
                name: "b".to_string(),
                description: None,
                parameters: None,
            }),
        ]);
        let wire = GeminiAdapter::build_chat_request(&t, ServiceType::Chat, &req).unwrap();
        let tools = wire.payload["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 1);
        let decls = tools[0]["functionDeclarations"].as_array().unwrap();
        assert_eq!(decls.len(), 2);
        assert_eq!(decls[0]["name"], "a");
        assert_eq!(decls[1]["name"], "b");
    }

    #[test]
    fn mcp_kind_passes_through_as_wire_key_for_gemini() {
        // 纯透传策略：Gemini wire 不认 `mcp` key，但 adapter 不替客户端决定丢弃。
        // kind 原样作为 wire key 写进 tools[0]，Gemini 收到不认的 key 会 400
        // 返给客户端——这是客户端选择把 MCP tool 路由到 Gemini 的代价。
        let t = target();
        let mut extra = serde_json::Map::new();
        extra.insert(
            "server_url".to_string(),
            serde_json::json!("https://example.com"),
        );
        let mut req = ChatRequest::new("x", vec![ChatMessage::user("q")]);
        req.tools = Some(vec![crate::types::Tool::builtin("mcp", extra)]);
        let wire = GeminiAdapter::build_chat_request(&t, ServiceType::Chat, &req).unwrap();
        let tools = wire.payload["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 1);
        // mcp 作为 wire key 平铺到 tool 对象里，value 是原 extra
        assert_eq!(tools[0]["mcp"]["server_url"], "https://example.com");
    }
}
