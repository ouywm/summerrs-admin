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
    ChatRequest, ChatResponse, ChatStreamEvent, ModelList, StreamEnd, ToolCallDelta, Usage,
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
    ) -> AdapterResult<Option<ChatStreamEvent>> {
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
    ) -> AdapterResult<Option<ChatStreamEvent>> {
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
                Value::String(target.actual_model.clone()),
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
        serde_json::from_slice(&body).map_err(AdapterError::DeserializeResponse)
    }

    /// 解析 OpenAI SSE 单个事件（已去 `data: ` 前缀）。
    ///
    /// 映射规则：
    ///
    /// | 上游 chunk 形态 | canonical 事件 |
    /// |---|---|
    /// | `[DONE]` | `Ok(None)` 忽略 |
    /// | `delta.role` 非空 **且** `delta.content` 空/缺失 | `Start { adapter, model }` |
    /// | `delta.content` 非空（role 是否存在不影响） | `TextDelta { text }` |
    /// | `delta.reasoning_content` 或 `delta.reasoning` 非空 | `ReasoningDelta { text }` |
    /// | `delta.tool_calls[*]` | `ToolCallDelta(...)`（当前取首个） |
    /// | `choice.finish_reason` 非空 | `End { finish_reason, usage }` |
    /// | `choices` 空 + `usage` 非空（OpenAI 末尾 usage-only chunk） | `End { usage }` |
    /// | 其他 | `Ok(None)`（忽略 keep-alive 等） |
    ///
    /// **关于 role + content 并存的 chunk**：标准 OpenAI 首块 `{role:"assistant", content:""}`
    /// → `Start`；一些聚合网关（如 one-hub 风格）会**每一个** chunk 都重复 role +
    /// 非空 content，这种时候 content 优先走 TextDelta，否则整条流的文本都会被吞掉。
    pub(super) fn parse_chat_stream_event(
        adapter_lower: &'static str,
        target: &ServiceTarget,
        raw: &str,
    ) -> AdapterResult<Option<ChatStreamEvent>> {
        // 1. 终止标记
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed == "[DONE]" {
            return Ok(None);
        }

        // 2. 解析 JSON
        let v: Value = serde_json::from_str(trimmed).map_err(AdapterError::DeserializeResponse)?;

        // 3. model（Start 事件用；未给则用 actual_model 兜底）
        let model = v
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or(&target.actual_model)
            .to_string();

        // 4. usage（可能与 choices 同块，也可能是独立末尾 chunk）
        let usage: Option<Usage> = v
            .get("usage")
            .filter(|x| !x.is_null())
            .and_then(|u| serde_json::from_value::<Usage>(u.clone()).ok());

        // 5. choices[0]
        let choice = v
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|a| a.first());

        let Some(choice) = choice else {
            // usage-only 末尾 chunk
            if usage.is_some() {
                return Ok(Some(ChatStreamEvent::End(StreamEnd {
                    finish_reason: None,
                    usage,
                })));
            }
            return Ok(None);
        };

        // 6. finish_reason（可能存在也可能 null）
        let finish_reason = choice
            .get("finish_reason")
            .filter(|x| !x.is_null())
            .and_then(|x| serde_json::from_value(x.clone()).ok());

        // 7. delta（首/中间块核心字段）
        let empty_map = Value::Null;
        let delta = choice.get("delta").unwrap_or(&empty_map);

        let role_str = delta.get("role").and_then(Value::as_str);
        let content_str = delta.get("content").and_then(Value::as_str);

        // 7.1 Start：首 chunk，带 role 但 content 为空 / 缺失
        //
        // 标准 OpenAI 首 chunk 长这样：`{"role":"assistant","content":""}`——开始事件。
        // 但部分兼容实现（如 one-hub 风格聚合网关）**每个 chunk 都带 role + 非空 content**，
        // 那种情况下应该按 content 走 TextDelta，不能因为带 role 就吞掉 content。
        if role_str.is_some() && content_str.map(str::is_empty).unwrap_or(true) {
            return Ok(Some(ChatStreamEvent::Start {
                adapter: adapter_lower.to_string(),
                model,
            }));
        }

        // 7.2 TextDelta：只要 content 非空即可（带不带 role 都一样）
        if let Some(text) = content_str
            && !text.is_empty()
        {
            return Ok(Some(ChatStreamEvent::TextDelta {
                text: text.to_string(),
            }));
        }

        // 7.3 ReasoningDelta（不同上游字段名不统一）
        //
        // - DeepSeek / o1 等用 `reasoning_content`
        // - OpenRouter / Ollama 等用 `reasoning`
        //
        // 对齐 rust-genai (openai/streamer.rs:249-253)：`reasoning_content` 优先，
        // 缺失时回退 `reasoning`，保证两类上游的思考过程都能透传。
        let reasoning_text = delta
            .get("reasoning_content")
            .and_then(Value::as_str)
            .or_else(|| delta.get("reasoning").and_then(Value::as_str));
        if let Some(text) = reasoning_text
            && !text.is_empty()
        {
            return Ok(Some(ChatStreamEvent::ReasoningDelta {
                text: text.to_string(),
            }));
        }

        // 7.4 ToolCallDelta（当前只处理首个；后续 chunk 会继续带）
        if let Some(tool_calls) = delta.get("tool_calls").and_then(Value::as_array) {
            if let Some(tc) = tool_calls.first() {
                let index = tc.get("index").and_then(Value::as_i64).unwrap_or(0) as i32;
                let id = tc.get("id").and_then(Value::as_str).map(str::to_string);
                let function = tc.get("function");
                let name = function
                    .and_then(|f| f.get("name"))
                    .and_then(Value::as_str)
                    .map(str::to_string);
                let arguments_delta = function
                    .and_then(|f| f.get("arguments"))
                    .and_then(Value::as_str)
                    .map(str::to_string);
                return Ok(Some(ChatStreamEvent::ToolCallDelta(ToolCallDelta {
                    index,
                    id,
                    name,
                    arguments_delta,
                })));
            }
        }

        // 8. End（finish_reason 或 usage 非空）
        if finish_reason.is_some() || usage.is_some() {
            return Ok(Some(ChatStreamEvent::End(StreamEnd {
                finish_reason,
                usage,
            })));
        }

        Ok(None)
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
        ServiceTarget::bearer(base, "sk-test", "gpt-4o-mini")
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
        let target = ServiceTarget::bearer("https://openrouter.ai/api/v1", "sk", "gpt-4o")
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
        assert!(e.is_none());
    }

    #[test]
    fn stream_empty_line_returns_none() {
        let t = bearer_target("https://api.openai.com");
        assert!(
            OpenAIAdapter::parse_chat_stream_event(&t, "")
                .unwrap()
                .is_none()
        );
        assert!(
            OpenAIAdapter::parse_chat_stream_event(&t, "   ")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn stream_first_chunk_is_start_event() {
        let t = bearer_target("https://api.openai.com");
        let raw = r#"{"id":"chatcmpl-x","object":"chat.completion.chunk","created":1,"model":"gpt-4o-mini","choices":[{"index":0,"delta":{"role":"assistant","content":""},"finish_reason":null}]}"#;
        let e = OpenAIAdapter::parse_chat_stream_event(&t, raw)
            .unwrap()
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
}
