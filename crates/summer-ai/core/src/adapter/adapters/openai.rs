//! OpenAI 协议 adapter。
//!
//! 两个 ZST：
//!
//! - [`OpenAIAdapter`]：官方 `api.openai.com`，`default_endpoint` 写死。
//! - [`OpenAICompatAdapter`]：所有 OpenAI 兼容第三方（OpenRouter / 硅基流动 /
//!   月之暗面 / 智谱 / 阿里云百炼 / vllm / ollama ...）。无默认 endpoint，
//!   必须由 `ServiceTarget` 提供。
//!
//! 两者共用 wire format、响应解析和流式事件解析，仅 URL 拼接规则不同。

use bytes::Bytes;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue};
use serde_json::Value;
use std::future::Future;

use crate::adapter::{
    Adapter, AdapterKind, AuthStrategy, Capabilities, CostProfile, ServiceType, WebRequestData,
};
use crate::error::{AdapterError, AdapterResult};
use crate::resolver::{Endpoint, ServiceTarget};
use crate::types::{
    ChatRequest, ChatResponse, ChatStreamEvent, FinishReason, MessageContent, ModelList, StreamEnd,
    StreamError, ToolCallDelta, Usage,
};

// ---------------------------------------------------------------------------
// OpenAIAdapter — 官方
// ---------------------------------------------------------------------------

/// OpenAI 官方协议（`api.openai.com`）。
pub struct OpenAIAdapter;

impl OpenAIAdapter {
    pub const API_KEY_DEFAULT_ENV_NAME: &'static str = "OPENAI_API_KEY";
    const BASE_URL: &'static str = "https://api.openai.com/v1/";
}

impl Adapter for OpenAIAdapter {
    const KIND: AdapterKind = AdapterKind::OpenAI;
    const DEFAULT_API_KEY_ENV_NAME: Option<&'static str> = Some(Self::API_KEY_DEFAULT_ENV_NAME);

    fn default_endpoint() -> Option<Endpoint> {
        Some(Endpoint::from_static(Self::BASE_URL))
    }

    fn capabilities() -> Capabilities {
        Capabilities::openai_like()
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
        shared::build_chat_request(target, service, req)
    }

    fn parse_chat_response(_target: &ServiceTarget, body: Bytes) -> AdapterResult<ChatResponse> {
        shared::parse_chat_response(body)
    }

    fn parse_chat_stream_event(
        target: &ServiceTarget,
        raw: &str,
    ) -> AdapterResult<Vec<ChatStreamEvent>> {
        shared::parse_chat_stream_event(Self::KIND.as_lower_str(), target, raw)
    }

    fn fetch_model_names(
        target: &ServiceTarget,
        http: &reqwest::Client,
    ) -> impl Future<Output = AdapterResult<Vec<String>>> + Send {
        shared::fetch_model_names(target, http)
    }
}

// ---------------------------------------------------------------------------
// OpenAICompatAdapter — 兼容家族
// ---------------------------------------------------------------------------

/// 所有 OpenAI 兼容的第三方协议（DeepSeek / 硅基流动 / OpenRouter / ...）。
///
/// 无默认 endpoint，必须由 [`ServiceTarget::endpoint`] 提供。
pub struct OpenAICompatAdapter;

impl Adapter for OpenAICompatAdapter {
    const KIND: AdapterKind = AdapterKind::OpenAICompat;
    const DEFAULT_API_KEY_ENV_NAME: Option<&'static str> = None;

    fn default_endpoint() -> Option<Endpoint> {
        None
    }

    fn capabilities() -> Capabilities {
        Capabilities::openai_like()
    }

    fn auth_strategy() -> AuthStrategy {
        AuthStrategy::Bearer
    }

    fn cost_profile() -> CostProfile {
        // Compat 默认不 assume 有 prompt cache——由具体厂商 adapter 覆盖
        CostProfile::default()
    }

    fn build_chat_request(
        target: &ServiceTarget,
        service: ServiceType,
        req: &ChatRequest,
    ) -> AdapterResult<WebRequestData> {
        Self::validate_chat_request(req)?;
        shared::build_chat_request(target, service, req)
    }

    fn parse_chat_response(_target: &ServiceTarget, body: Bytes) -> AdapterResult<ChatResponse> {
        shared::parse_chat_response(body)
    }

    fn parse_chat_stream_event(
        target: &ServiceTarget,
        raw: &str,
    ) -> AdapterResult<Vec<ChatStreamEvent>> {
        shared::parse_chat_stream_event(Self::KIND.as_lower_str(), target, raw)
    }

    fn fetch_model_names(
        target: &ServiceTarget,
        http: &reqwest::Client,
    ) -> impl Future<Output = AdapterResult<Vec<String>>> + Send {
        shared::fetch_model_names(target, http)
    }
}

// ---------------------------------------------------------------------------
// 共享逻辑
// ---------------------------------------------------------------------------

mod shared {
    use super::*;

    /// 构造 POST `/v1/chat/completions` 的 HTTP 请求数据。
    pub(super) fn build_chat_request(
        target: &ServiceTarget,
        _service: ServiceType,
        request: &ChatRequest,
    ) -> AdapterResult<WebRequestData> {
        let url = build_url(target.endpoint.trimmed());

        // payload：原样序列化 ChatRequest，只把 model 覆盖成 actual_model
        let mut payload = serde_json::to_value(request).map_err(AdapterError::SerializeRequest)?;
        if let Some(obj) = payload.as_object_mut() {
            obj.insert(
                "model".to_string(),
                Value::String(target.actual_model().to_string()),
            );
        }

        let headers = build_headers(target)?;

        Ok(WebRequestData {
            url,
            headers,
            payload,
        })
    }

    /// 解析 OpenAI 非流式响应。
    pub(super) fn parse_chat_response(body: Bytes) -> AdapterResult<ChatResponse> {
        let mut resp: ChatResponse =
            serde_json::from_slice(&body).map_err(AdapterError::DeserializeResponse)?;
        adjust_xai_reasoning_tokens(&mut resp.usage, &resp.model);
        // DeepSeek R1 本地部署（Ollama / vLLM / sglang）会把思考链塞在 content 里的
        // `<think>...</think>` 标签中，而非走 `reasoning_content` 字段。正式 API
        // 本身不用这种表达，所以只在 reasoning_content 为空、content 含标签时
        // 才做提取。
        for choice in &mut resp.choices {
            extract_think_tag_into_reasoning(&mut choice.message);
        }
        Ok(resp)
    }

    /// 从 `ChatMessage.content` 里抽出 `<think>...</think>` 包裹的思考链，
    /// 塞进 `reasoning_content`。content 清理为去掉标签的正文。
    ///
    /// 只在 `reasoning_content` 原本为 None 时执行，避免覆盖上游已经正确分字段
    /// 的情况（DeepSeek 官方 API、Kimi、OpenRouter 等）。
    pub(super) fn extract_think_tag_into_reasoning(msg: &mut crate::types::ChatMessage) {
        if msg.reasoning_content.is_some() {
            return;
        }
        let Some(MessageContent::Text(text)) = &msg.content else {
            return;
        };
        let Some((cleaned, thinking)) = split_think_tag(text) else {
            return;
        };
        msg.reasoning_content = Some(thinking);
        msg.content = Some(MessageContent::Text(cleaned));
    }

    /// 在字符串里定位 `<think>...</think>`，返回 `(去标签后的正文, 思考内容)`。
    ///
    /// 规则（对齐 rust-genai `openai/adapter_impl.rs::extract_think`）：
    /// - `<think>` 不必在最开头（部分上游会在前面塞几个空行 / 一段 prelude）。
    /// - 只处理第一次出现的一对标签；嵌套 `<think>` 的罕见情形按第一个 `</think>` 关闭。
    /// - 思考内容 `trim` 掉首尾空白；after_think 去掉 leading 空白，避免 cleaned
    ///   开头留空行。
    /// - 找不到成对标签返回 `None`，调用方保持 content 不变。
    fn split_think_tag(s: &str) -> Option<(String, String)> {
        const START: &str = "<think>";
        const END: &str = "</think>";
        let start_pos = s.find(START)?;
        let after_start = &s[start_pos + START.len()..];
        let end_offset = after_start.find(END)?;
        let think = after_start[..end_offset].trim().to_string();
        let before = &s[..start_pos];
        let after = after_start[end_offset + END.len()..].trim_start();
        Some((format!("{before}{after}"), think))
    }

    /// 修补 xAI grok-3 家族上游协议 bug：`completion_tokens` 不含 `reasoning_tokens`。
    ///
    /// OpenAI / o1 的约定是 `completion_tokens` 已经包含 `reasoning_tokens`；但 xAI
    /// 的 grok-3 / grok-3-mini 把思考 tokens 单独列在 `completion_tokens_details`
    /// 里而不加进 `completion_tokens`，导致 billing 按 completion 计费时漏算。
    ///
    /// 检测策略：按 rust-genai 的做法，看 response.model 是否以 `grok-3` 开头；
    /// 命中则把 `reasoning_tokens` 累加到 `completion_tokens` 和 `total_tokens` 上，
    /// 使这两个字段与 OpenAI 惯例对齐。
    pub(super) fn adjust_xai_reasoning_tokens(usage: &mut Usage, model: &str) {
        if !model.starts_with("grok-3") {
            return;
        }
        let Some(details) = usage.completion_tokens_details.as_ref() else {
            return;
        };
        let Some(reasoning) = details.reasoning_tokens else {
            return;
        };
        if reasoning <= 0 {
            return;
        }
        usage.completion_tokens += reasoning;
        usage.total_tokens += reasoning;
    }

    /// 解析 OpenAI SSE 单个事件（已去 `data: ` 前缀）。
    ///
    /// 映射规则：
    ///
    /// | 上游 chunk 形态 | canonical 事件 |
    /// |---|---|
    /// | `[DONE]` | 空 Vec（忽略） |
    /// | `delta.role` 非空 **且** `delta.content` 空/缺失 | `[Start { adapter, model }]` |
    /// | `delta.content` 非空（role 是否存在不影响） | 含 `TextDelta { text }` |
    /// | `delta.reasoning_content` 或 `delta.reasoning` 非空 | 含 `ReasoningDelta { text }` |
    /// | `delta.tool_calls[*]` | 每个元素一个 `ToolCallDelta(...)`（并行工具调用） |
    /// | `choice.finish_reason` 非空 | 最后追加 `End { finish_reason, usage }` |
    /// | `choices` 空 + `usage` 非空（OpenAI 末尾 usage-only chunk） | `[End { usage }]` |
    /// | 其他 | 空 Vec（忽略 keep-alive 等） |
    ///
    /// **多事件组合**：上游常把 `content + finish_reason`（Mistral）或
    /// `tool_calls + finish_reason`（Ollama）打包在同一 chunk，按 rust-genai
    /// 的做法一次 emit 多个事件——只发首个会丢失 content / finish / 并行 tool_call。
    ///
    /// **关于 role + content 并存**：标准 OpenAI 首块 `{role:"assistant",content:""}`
    /// 是 `Start`；某些聚合网关（one-hub 风格）每块都带 role + 非空 content，
    /// 按 content 走 TextDelta 避免整条流被吞。
    pub(super) fn parse_chat_stream_event(
        adapter_lower: &'static str,
        target: &ServiceTarget,
        raw: &str,
    ) -> AdapterResult<Vec<ChatStreamEvent>> {
        // 1. 终止标记
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed == "[DONE]" {
            return Ok(Vec::new());
        }

        // 2. 解析 JSON
        let v: Value = serde_json::from_str(trimmed).map_err(AdapterError::DeserializeResponse)?;

        // 2.1 error chunk：OpenAI 在 stream 中途遇错时会发 `{"error":{...}}`。对齐
        //     rust-genai openai/streamer.rs:13 `take_stream_error`，单独识别并转成
        //     canonical Error 事件，stream_driver 会终止流 + Failure outcome。
        if let Some(err) = v.get("error").filter(|x| !x.is_null()) {
            let message = err
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("upstream stream error")
                .to_string();
            let kind = err.get("type").and_then(Value::as_str).map(str::to_string);
            return Ok(vec![ChatStreamEvent::Error(StreamError { message, kind })]);
        }

        // 3. model（Start 事件用；未给则用 actual_model 兜底）
        let model = v
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or(target.actual_model())
            .to_string();

        // 4. usage（可能与 choices 同块，也可能是独立末尾 chunk）
        //
        // Groq 的 openai-compatible 流式 response 把 usage 挂在 `x_groq.usage` 而非顶层，
        // 顶层 `usage` 可能完全缺失。优先读 `x_groq.usage`，没有再 fallback 顶层，
        // 兼容 Groq 与所有其他 provider。
        let usage: Option<Usage> = v
            .get("x_groq")
            .and_then(|g| g.get("usage"))
            .filter(|x| !x.is_null())
            .and_then(|u| serde_json::from_value::<Usage>(u.clone()).ok())
            .or_else(|| {
                v.get("usage")
                    .filter(|x| !x.is_null())
                    .and_then(|u| serde_json::from_value::<Usage>(u.clone()).ok())
            })
            .map(|mut u| {
                adjust_xai_reasoning_tokens(&mut u, &model);
                u
            });

        // 5. choices[0]
        let choice = v
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|a| a.first());

        let Some(choice) = choice else {
            // usage-only 末尾 chunk
            if usage.is_some() {
                return Ok(vec![ChatStreamEvent::End(StreamEnd {
                    finish_reason: None,
                    usage,
                })]);
            }
            return Ok(Vec::new());
        };

        // 6. finish_reason（可能存在也可能 null）
        let finish_reason: Option<FinishReason> = choice
            .get("finish_reason")
            .filter(|x| !x.is_null())
            .and_then(|x| serde_json::from_value(x.clone()).ok());

        // 7. delta（首/中间块核心字段）
        let empty_map = Value::Null;
        let delta = choice.get("delta").unwrap_or(&empty_map);
        let role_str = delta.get("role").and_then(Value::as_str);
        let content_str = delta.get("content").and_then(Value::as_str);

        let mut events: Vec<ChatStreamEvent> = Vec::new();

        // 7.1 Start：role 存在且 content 空/缺失。content 非空时不发 Start（避免吞掉文字），
        //     ingress 侧的 `role_emitted` 兜底保证客户端 wire 格式仍带 role-only chunk。
        if role_str.is_some() && content_str.map(str::is_empty).unwrap_or(true) {
            events.push(ChatStreamEvent::Start {
                adapter: adapter_lower.to_string(),
                model,
            });
        }

        // 7.2 TextDelta：content 非空（Mistral 等会把 content 和 finish_reason 一并塞进末块）
        if let Some(text) = content_str
            && !text.is_empty()
        {
            events.push(ChatStreamEvent::TextDelta {
                text: text.to_string(),
            });
        }

        // 7.3 ReasoningDelta：reasoning_content 优先，fallback reasoning
        let reasoning_text = delta
            .get("reasoning_content")
            .and_then(Value::as_str)
            .or_else(|| delta.get("reasoning").and_then(Value::as_str));
        if let Some(text) = reasoning_text
            && !text.is_empty()
        {
            events.push(ChatStreamEvent::ReasoningDelta {
                text: text.to_string(),
            });
        }

        // 7.4 ToolCallDelta：并行 tool_call 同 chunk 多条，全部 emit
        if let Some(tool_calls) = delta.get("tool_calls").and_then(Value::as_array) {
            for tc in tool_calls {
                let index = tc.get("index").and_then(Value::as_i64).unwrap_or(0) as i32;
                let id = tc
                    .get("id")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string);
                let function = tc.get("function");
                let name = function
                    .and_then(|f| f.get("name"))
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string);
                let arguments_delta = function
                    .and_then(|f| f.get("arguments"))
                    .and_then(Value::as_str)
                    .map(str::to_string);
                // 完全空 delta（没 id 没 name 没 args）跳过，避免产生噪声事件
                if id.is_none() && name.is_none() && arguments_delta.is_none() {
                    continue;
                }
                events.push(ChatStreamEvent::ToolCallDelta(ToolCallDelta {
                    index,
                    id,
                    name,
                    arguments_delta,
                }));
            }
        }

        // 8. End：finish_reason 非空或 usage 非空时 emit；要在 content / tool_calls 之后
        //    push，保证同一 chunk 里内容先发、End 收尾。
        if finish_reason.is_some() || usage.is_some() {
            events.push(ChatStreamEvent::End(StreamEnd {
                finish_reason,
                usage,
            }));
        }

        Ok(events)
    }

    /// GET `{endpoint}/models` 拉取可用模型 id 列表。
    ///
    /// 用于 `/v1/models` 端点 + admin 连通性测试。Bearer auth 来自 `target.auth`。
    pub(super) async fn fetch_model_names(
        target: &ServiceTarget,
        http: &reqwest::Client,
    ) -> AdapterResult<Vec<String>> {
        let url = build_models_url(target.endpoint.trimmed());
        let headers = build_headers(target)?;

        let response = http
            .get(&url)
            .headers(headers)
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

// ---------------------------------------------------------------------------
// URL / Headers 工具
// ---------------------------------------------------------------------------

/// URL 拼接规则：
///
/// - 已含 `/chat/completions` 后缀：原样返
/// - 已含 `/v1` 或 `/v1/`：只拼 `/chat/completions`
/// - 其它：拼 `/v1/chat/completions`（compat 厂商 90% 也是 v1 路径）
fn build_url(base: &str) -> String {
    if base.ends_with("/chat/completions") {
        return base.to_string();
    }
    if base.ends_with("/v1") || base.contains("/v1/") {
        let base = base.trim_end_matches('/');
        format!("{base}/chat/completions")
    } else {
        let base = base.trim_end_matches('/');
        format!("{base}/v1/chat/completions")
    }
}

/// 构造 headers：Bearer auth + extra_headers + Content-Type。
fn build_headers(target: &ServiceTarget) -> AdapterResult<HeaderMap> {
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

    // OpenAI 协议家族统一用 Bearer
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

/// URL 拼接规则（`/v1/models` 端点）：逻辑同 `build_url`，只是后缀换成 `/models`。
fn build_models_url(base: &str) -> String {
    if base.ends_with("/models") {
        return base.to_string();
    }
    if base.ends_with("/v1") || base.contains("/v1/") {
        let base = base.trim_end_matches('/');
        format!("{base}/models")
    } else {
        let base = base.trim_end_matches('/');
        format!("{base}/v1/models")
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ChatMessage;

    fn bearer_target(base: &str) -> ServiceTarget {
        ServiceTarget::bearer(AdapterKind::OpenAI, base, "sk-test", "gpt-4o-mini")
    }

    // ────── build_chat_request ──────

    #[test]
    fn openai_endpoint_forces_v1() {
        let target = bearer_target("https://api.openai.com");
        let data = OpenAIAdapter::build_chat_request(
            &target,
            ServiceType::Chat,
            &ChatRequest::new("logical", vec![ChatMessage::user("hi")]),
        )
        .unwrap();
        assert_eq!(data.url, "https://api.openai.com/v1/chat/completions");
    }

    #[test]
    fn compat_respects_existing_v1_in_base_url() {
        let target = bearer_target("https://openrouter.ai/api/v1");
        let data = OpenAICompatAdapter::build_chat_request(
            &target,
            ServiceType::Chat,
            &ChatRequest::new("logical", vec![ChatMessage::user("hi")]),
        )
        .unwrap();
        assert_eq!(data.url, "https://openrouter.ai/api/v1/chat/completions");
    }

    #[test]
    fn build_request_rewrites_model_to_actual() {
        let target = bearer_target("https://api.openai.com");
        let req = ChatRequest::new("alias-name", vec![ChatMessage::user("hi")]);
        let data = OpenAIAdapter::build_chat_request(&target, ServiceType::Chat, &req).unwrap();
        assert_eq!(data.payload["model"], "gpt-4o-mini");
    }

    #[test]
    fn bearer_header_set_from_auth() {
        let target = bearer_target("https://api.openai.com");
        let req = ChatRequest::new("x", vec![ChatMessage::user("hi")]);
        let data = OpenAIAdapter::build_chat_request(&target, ServiceType::Chat, &req).unwrap();
        assert_eq!(
            data.headers.get("authorization").unwrap().to_str().unwrap(),
            "Bearer sk-test"
        );
    }

    #[test]
    fn extra_headers_applied() {
        let target = ServiceTarget::bearer(
            AdapterKind::OpenAICompat,
            "https://openrouter.ai/api/v1",
            "sk",
            "gpt-4o",
        )
        .with_header("HTTP-Referer", "https://my.app")
        .with_header("X-Title", "MyApp");
        let req = ChatRequest::new("x", vec![ChatMessage::user("hi")]);
        let data =
            OpenAICompatAdapter::build_chat_request(&target, ServiceType::Chat, &req).unwrap();
        assert_eq!(
            data.headers.get("HTTP-Referer").unwrap().to_str().unwrap(),
            "https://my.app"
        );
    }

    // ────── parse_chat_response ──────

    #[test]
    fn parse_response_reads_minimal_payload() {
        let target = bearer_target("https://api.openai.com");
        let payload = br#"{
            "id":"chatcmpl-1",
            "object":"chat.completion",
            "created":1700000000,
            "model":"gpt-4o-mini",
            "choices":[{"index":0,"message":{"role":"assistant","content":"hi"},"finish_reason":"stop"}],
            "usage":{"prompt_tokens":3,"completion_tokens":1,"total_tokens":4}
        }"#;
        let parsed =
            OpenAIAdapter::parse_chat_response(&target, Bytes::from_static(payload)).unwrap();
        assert_eq!(parsed.id, "chatcmpl-1");
        assert_eq!(parsed.first_text(), Some("hi"));
        assert_eq!(parsed.usage.total_tokens, 4);
    }

    // ────── parse_chat_stream_event ──────

    #[test]
    fn stream_done_returns_none() {
        let t = bearer_target("https://api.openai.com");
        let e = OpenAIAdapter::parse_chat_stream_event(&t, "[DONE]").unwrap();
        assert!(e.is_empty());
    }

    #[test]
    fn stream_empty_line_returns_none() {
        let t = bearer_target("https://api.openai.com");
        assert!(
            OpenAIAdapter::parse_chat_stream_event(&t, "")
                .unwrap()
                .is_empty()
        );
        assert!(
            OpenAIAdapter::parse_chat_stream_event(&t, "   ")
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn stream_first_chunk_is_start_event() {
        let t = bearer_target("https://api.openai.com");
        let raw = r#"{"id":"chatcmpl-x","object":"chat.completion.chunk","created":1,"model":"gpt-4o-mini","choices":[{"index":0,"delta":{"role":"assistant","content":""},"finish_reason":null}]}"#;
        let e = OpenAIAdapter::parse_chat_stream_event(&t, raw)
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        match e {
            ChatStreamEvent::Start { adapter, model } => {
                assert_eq!(adapter, "openai");
                assert_eq!(model, "gpt-4o-mini");
            }
            other => panic!("expected Start, got {other:?}"),
        }
    }

    #[test]
    fn stream_text_delta_chunk() {
        let t = bearer_target("https://api.openai.com");
        let raw = r#"{"id":"x","choices":[{"index":0,"delta":{"content":"hello"},"finish_reason":null}]}"#;
        let e = OpenAIAdapter::parse_chat_stream_event(&t, raw)
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        match e {
            ChatStreamEvent::TextDelta { text } => assert_eq!(text, "hello"),
            other => panic!("expected TextDelta, got {other:?}"),
        }
    }

    #[test]
    fn stream_chunk_with_role_and_nonempty_content_is_text_delta() {
        // 回归：部分聚合网关（hybgzs / one-hub 风格）每个 chunk 都带 role + 非空 content，
        // 以前的实现看到 role 就回 Start，把所有 content 吞掉，整条流只剩空 role delta。
        let t = bearer_target("https://ai.hybgzs.com");
        let raw = r#"{"id":"x","choices":[{"index":0,"delta":{"role":"assistant","content":"Use"},"finish_reason":null}]}"#;
        let e = OpenAIAdapter::parse_chat_stream_event(&t, raw)
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        match e {
            ChatStreamEvent::TextDelta { text } => assert_eq!(text, "Use"),
            other => panic!("expected TextDelta, got {other:?}"),
        }
    }

    #[test]
    fn stream_reasoning_delta_chunk() {
        let t = bearer_target("https://api.deepseek.com");
        let raw = r#"{"choices":[{"index":0,"delta":{"reasoning_content":"let me think"},"finish_reason":null}]}"#;
        let e = OpenAIAdapter::parse_chat_stream_event(&t, raw)
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        match e {
            ChatStreamEvent::ReasoningDelta { text } => assert_eq!(text, "let me think"),
            other => panic!("expected ReasoningDelta, got {other:?}"),
        }
    }

    #[test]
    fn stream_reasoning_delta_falls_back_to_reasoning_field() {
        // OpenRouter / Ollama 用 `reasoning` 而非 `reasoning_content`——要能透传。
        let t = bearer_target("https://openrouter.ai");
        let raw = r#"{"choices":[{"index":0,"delta":{"reasoning":"considering options"},"finish_reason":null}]}"#;
        let e = OpenAIAdapter::parse_chat_stream_event(&t, raw)
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        match e {
            ChatStreamEvent::ReasoningDelta { text } => assert_eq!(text, "considering options"),
            other => panic!("expected ReasoningDelta, got {other:?}"),
        }
    }

    #[test]
    fn stream_reasoning_delta_prefers_reasoning_content_over_reasoning() {
        // 两个字段都带时，`reasoning_content` 优先（对齐 rust-genai）。
        let t = bearer_target("https://api.deepseek.com");
        let raw = r#"{"choices":[{"index":0,"delta":{"reasoning_content":"primary","reasoning":"secondary"},"finish_reason":null}]}"#;
        let e = OpenAIAdapter::parse_chat_stream_event(&t, raw)
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        match e {
            ChatStreamEvent::ReasoningDelta { text } => assert_eq!(text, "primary"),
            other => panic!("expected ReasoningDelta, got {other:?}"),
        }
    }

    #[test]
    fn stream_tool_call_delta_chunk() {
        let t = bearer_target("https://api.openai.com");
        let raw = r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"tc_1","type":"function","function":{"name":"weather","arguments":"{\"city\""}}]},"finish_reason":null}]}"#;
        let e = OpenAIAdapter::parse_chat_stream_event(&t, raw)
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        match e {
            ChatStreamEvent::ToolCallDelta(d) => {
                assert_eq!(d.index, 0);
                assert_eq!(d.id.as_deref(), Some("tc_1"));
                assert_eq!(d.name.as_deref(), Some("weather"));
                assert!(
                    d.arguments_delta
                        .as_deref()
                        .unwrap()
                        .starts_with("{\"city\"")
                );
            }
            other => panic!("expected ToolCallDelta, got {other:?}"),
        }
    }

    #[test]
    fn stream_finish_reason_chunk_emits_end() {
        let t = bearer_target("https://api.openai.com");
        let raw = r#"{"choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}"#;
        let e = OpenAIAdapter::parse_chat_stream_event(&t, raw)
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        match e {
            ChatStreamEvent::End(end) => {
                assert!(end.finish_reason.is_some());
                assert!(end.usage.is_none());
            }
            other => panic!("expected End, got {other:?}"),
        }
    }

    #[test]
    fn stream_usage_only_chunk_emits_end() {
        let t = bearer_target("https://api.openai.com");
        let raw =
            r#"{"choices":[],"usage":{"prompt_tokens":5,"completion_tokens":7,"total_tokens":12}}"#;
        let e = OpenAIAdapter::parse_chat_stream_event(&t, raw)
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        match e {
            ChatStreamEvent::End(end) => {
                assert!(end.finish_reason.is_none());
                let usage = end.usage.unwrap();
                assert_eq!(usage.prompt_tokens, 5);
                assert_eq!(usage.completion_tokens, 7);
                assert_eq!(usage.total_tokens, 12);
            }
            other => panic!("expected End, got {other:?}"),
        }
    }

    #[test]
    fn stream_content_and_finish_in_same_chunk_emits_both_events() {
        // Mistral / 某些聚合网关会把最后一段 content 和 finish_reason 打包在同一 chunk：
        // `{delta:{content:"!"}, finish_reason:"stop"}`。必须同时 emit TextDelta + End，
        // 否则 content 要么丢、要么 End 永不发送（客户端靠 [DONE] 兜底）。
        let t = bearer_target("https://api.openai.com");
        let raw = r#"{"choices":[{"index":0,"delta":{"content":"bye"},"finish_reason":"stop"}]}"#;
        let events = OpenAIAdapter::parse_chat_stream_event(&t, raw).unwrap();
        assert_eq!(events.len(), 2, "expected TextDelta + End");
        match &events[0] {
            ChatStreamEvent::TextDelta { text } => assert_eq!(text, "bye"),
            other => panic!("expected TextDelta first, got {other:?}"),
        }
        match &events[1] {
            ChatStreamEvent::End(end) => assert!(end.finish_reason.is_some()),
            other => panic!("expected End last, got {other:?}"),
        }
    }

    #[test]
    fn stream_parallel_tool_calls_emit_multiple_deltas_in_one_chunk() {
        // OpenAI parallel_tool_calls=true 时，单个 chunk 的 delta.tool_calls 可以同时
        // 出现多个 index 不同的 tool_call——必须全部 emit，否则并行调用只会看到第一个。
        let t = bearer_target("https://api.openai.com");
        let raw = r#"{"choices":[{"index":0,"delta":{"tool_calls":[
            {"index":0,"id":"a","type":"function","function":{"name":"fa","arguments":""}},
            {"index":1,"id":"b","type":"function","function":{"name":"fb","arguments":""}}
        ]},"finish_reason":null}]}"#;
        let events = OpenAIAdapter::parse_chat_stream_event(&t, raw).unwrap();
        assert_eq!(events.len(), 2);
        for (expected_idx, expected_name, ev) in [(0, "fa", &events[0]), (1, "fb", &events[1])] {
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
    fn stream_tool_calls_and_finish_in_same_chunk_emit_both() {
        // Ollama 等会把 tool_calls + finish_reason="tool_calls" 一起发——End 必须同时 emit。
        let t = bearer_target("https://api.openai.com");
        let raw = r#"{"choices":[{"index":0,"delta":{"tool_calls":[
            {"index":0,"id":"c","type":"function","function":{"name":"fx","arguments":"{}"}}
        ]},"finish_reason":"tool_calls"}]}"#;
        let events = OpenAIAdapter::parse_chat_stream_event(&t, raw).unwrap();
        assert_eq!(events.len(), 2);
        assert!(matches!(events[0], ChatStreamEvent::ToolCallDelta(_)));
        assert!(matches!(events[1], ChatStreamEvent::End(_)));
    }

    #[test]
    fn stream_error_chunk_maps_to_canonical_error() {
        // OpenAI 在 stream 中途遇错时会下发 `{"error":{"message":..,"type":..}}` chunk，
        // 之前会因为没 choices + 没 usage 被当成 keep-alive 丢掉，客户端完全感知不到失败。
        // 必须识别出来 emit Error，stream_driver 会终止流并置 Failure outcome。
        let t = bearer_target("https://api.openai.com");
        let raw = r#"{"error":{"message":"quota exceeded","type":"insufficient_quota"}}"#;
        let events = OpenAIAdapter::parse_chat_stream_event(&t, raw).unwrap();
        assert_eq!(events.len(), 1);
        match &events[0] {
            ChatStreamEvent::Error(err) => {
                assert_eq!(err.message, "quota exceeded");
                assert_eq!(err.kind.as_deref(), Some("insufficient_quota"));
            }
            other => panic!("expected Error, got {other:?}"),
        }
    }

    #[test]
    fn stream_groq_x_groq_usage_takes_precedence() {
        // Groq 的 openai-compatible 流式 response 把 usage 放在 `x_groq.usage` 而非顶层。
        // 此时必须读 x_groq.usage，否则 billing 看到的 tokens 为 0。
        let t = bearer_target("https://api.groq.com/openai");
        let raw = r#"{
            "choices":[],
            "x_groq":{"usage":{"prompt_tokens":20,"completion_tokens":30,"total_tokens":50}}
        }"#;
        let events = OpenAIAdapter::parse_chat_stream_event(&t, raw).unwrap();
        assert_eq!(events.len(), 1);
        match &events[0] {
            ChatStreamEvent::End(end) => {
                let u = end.usage.as_ref().expect("usage must come from x_groq");
                assert_eq!(u.prompt_tokens, 20);
                assert_eq!(u.completion_tokens, 30);
                assert_eq!(u.total_tokens, 50);
            }
            other => panic!("expected End, got {other:?}"),
        }
    }

    #[test]
    fn stream_top_level_usage_still_works_for_non_groq() {
        // 非 Groq 供应商 x_groq 字段不存在，必须 fallback 顶层 usage —— 这是 OpenAI
        // 官方行为，不能因为加了 x_groq 优先就把顶层 usage 丢了。
        let t = bearer_target("https://api.openai.com");
        let raw = r#"{
            "choices":[],
            "usage":{"prompt_tokens":3,"completion_tokens":4,"total_tokens":7}
        }"#;
        let events = OpenAIAdapter::parse_chat_stream_event(&t, raw).unwrap();
        assert_eq!(events.len(), 1);
        match &events[0] {
            ChatStreamEvent::End(end) => {
                let u = end.usage.as_ref().unwrap();
                assert_eq!(u.prompt_tokens, 3);
                assert_eq!(u.completion_tokens, 4);
            }
            other => panic!("expected End, got {other:?}"),
        }
    }

    #[test]
    fn non_stream_xai_grok3_reasoning_tokens_added_into_completion() {
        // xAI grok-3 协议 bug：`completion_tokens` 不含 `reasoning_tokens`。
        // canonical 层必须补加，让 billing 按 completion 计费时不漏 thinking tokens
        // —— 这是和 OpenAI o1 的 "completion 已含 reasoning" 惯例对齐。
        let t = bearer_target("https://api.x.ai");
        let body = br#"{
            "id":"resp_x","object":"chat.completion","created":0,
            "model":"grok-3-mini",
            "choices":[{"index":0,"message":{"role":"assistant","content":"hi"},"finish_reason":"stop"}],
            "usage":{
                "prompt_tokens":10,
                "completion_tokens":5,
                "total_tokens":15,
                "completion_tokens_details":{"reasoning_tokens":30}
            }
        }"#;
        let resp = OpenAIAdapter::parse_chat_response(&t, Bytes::from_static(body)).unwrap();
        // 5 + 30 = 35，total 15 + 30 = 45
        assert_eq!(resp.usage.completion_tokens, 35);
        assert_eq!(resp.usage.total_tokens, 45);
        assert_eq!(
            resp.usage
                .completion_tokens_details
                .as_ref()
                .unwrap()
                .reasoning_tokens,
            Some(30)
        );
    }

    #[test]
    fn non_stream_non_xai_reasoning_tokens_not_touched() {
        // 非 xAI 的 OpenAI 家（o1）完全遵守 "completion_tokens 已含 reasoning_tokens"，
        // 不能被再次累加。确保 adjustment 只针对 grok-3 前缀。
        let t = bearer_target("https://api.openai.com");
        let body = br#"{
            "id":"resp_o1","object":"chat.completion","created":0,
            "model":"o1-mini",
            "choices":[{"index":0,"message":{"role":"assistant","content":"hi"},"finish_reason":"stop"}],
            "usage":{
                "prompt_tokens":10,
                "completion_tokens":42,
                "total_tokens":52,
                "completion_tokens_details":{"reasoning_tokens":30}
            }
        }"#;
        let resp = OpenAIAdapter::parse_chat_response(&t, Bytes::from_static(body)).unwrap();
        assert_eq!(resp.usage.completion_tokens, 42);
        assert_eq!(resp.usage.total_tokens, 52);
    }

    #[test]
    fn stream_xai_grok3_reasoning_tokens_merged_on_usage_chunk() {
        // 流式路径下 xAI 末尾 usage-only chunk 也要走同一修补；否则 stream_driver
        // 累加 final_usage 时仍然漏计 reasoning。
        let t = bearer_target("https://api.x.ai");
        let raw = r#"{
            "model":"grok-3",
            "choices":[],
            "usage":{
                "prompt_tokens":4,
                "completion_tokens":2,
                "total_tokens":6,
                "completion_tokens_details":{"reasoning_tokens":10}
            }
        }"#;
        let events = OpenAIAdapter::parse_chat_stream_event(&t, raw).unwrap();
        match &events[0] {
            ChatStreamEvent::End(end) => {
                let u = end.usage.as_ref().unwrap();
                // 2 + 10 = 12
                assert_eq!(u.completion_tokens, 12);
                assert_eq!(u.total_tokens, 16);
            }
            other => panic!("expected End, got {other:?}"),
        }
    }

    #[test]
    fn non_stream_deepseek_r1_think_tag_moves_to_reasoning_content() {
        // DeepSeek R1 本地部署（Ollama / vLLM）把思考链塞 content 的 `<think>...</think>`
        // 标签中。canonical 层必须识别并抽到 reasoning_content —— 否则客户端
        // content 里带着原始标签，UI 直接看到 "<think>..." 字符串。
        let t = bearer_target("http://localhost:11434");
        let body = br#"{
            "id":"r","object":"chat.completion","created":0,
            "model":"deepseek-r1:7b",
            "choices":[{"index":0,"message":{
                "role":"assistant",
                "content":"<think>let me think step by step</think>\n\nfinal answer"
            },"finish_reason":"stop"}],
            "usage":{"prompt_tokens":3,"completion_tokens":10,"total_tokens":13}
        }"#;
        let resp = OpenAIAdapter::parse_chat_response(&t, Bytes::from_static(body)).unwrap();
        let msg = &resp.choices[0].message;
        assert_eq!(
            msg.reasoning_content.as_deref(),
            Some("let me think step by step")
        );
        match msg.content.as_ref().unwrap() {
            MessageContent::Text(t) => assert_eq!(t, "final answer"),
            other => panic!("expected Text, got {other:?}"),
        }
    }

    #[test]
    fn non_stream_think_tag_not_touched_when_reasoning_content_already_set() {
        // DeepSeek 官方 API 会正确分 reasoning_content 字段。此时 content 里
        // 恰好含 `<think>` 字面量（比如用户示例代码）不能被误抽 —— 上游已明确
        // 分了字段，canonical 不应再修改。
        let t = bearer_target("https://api.deepseek.com");
        let body = br#"{
            "id":"r","object":"chat.completion","created":0,
            "model":"deepseek-reasoner",
            "choices":[{"index":0,"message":{
                "role":"assistant",
                "content":"Here is an example: <think>not a real think tag</think>",
                "reasoning_content":"real reasoning from API"
            },"finish_reason":"stop"}],
            "usage":{"prompt_tokens":3,"completion_tokens":10,"total_tokens":13}
        }"#;
        let resp = OpenAIAdapter::parse_chat_response(&t, Bytes::from_static(body)).unwrap();
        let msg = &resp.choices[0].message;
        assert_eq!(
            msg.reasoning_content.as_deref(),
            Some("real reasoning from API")
        );
        // content 原样保留
        match msg.content.as_ref().unwrap() {
            MessageContent::Text(t) => {
                assert!(t.contains("<think>"));
                assert!(t.contains("</think>"));
            }
            _ => panic!("expected Text"),
        }
    }

    #[test]
    fn non_stream_think_tag_missing_close_is_preserved() {
        // content 里只有 `<think>` 没有 `</think>`（极端边缘）：不做提取，
        // 保持 content 原样，避免把未完成的思考当作正文全吞了。
        let t = bearer_target("http://localhost:11434");
        let body = br#"{
            "id":"r","object":"chat.completion","created":0,
            "model":"deepseek-r1:7b",
            "choices":[{"index":0,"message":{
                "role":"assistant",
                "content":"<think>incomplete"
            },"finish_reason":"stop"}],
            "usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}
        }"#;
        let resp = OpenAIAdapter::parse_chat_response(&t, Bytes::from_static(body)).unwrap();
        let msg = &resp.choices[0].message;
        assert!(msg.reasoning_content.is_none());
        match msg.content.as_ref().unwrap() {
            MessageContent::Text(t) => assert_eq!(t, "<think>incomplete"),
            _ => panic!("expected Text"),
        }
    }
}
