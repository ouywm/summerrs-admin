use anyhow::Result;
use bytes::Bytes;
use futures::stream::BoxStream;
use summer_web::axum::http::{HeaderMap, StatusCode};

use crate::types::chat::{ChatCompletionChunk, ChatCompletionRequest, ChatCompletionResponse};

mod anthropic;
mod gemini;
mod openai;

pub use anthropic::AnthropicAdapter;
pub use gemini::GeminiAdapter;
pub use openai::OpenAiAdapter;

const CHAT_ONLY_PROVIDER_SCOPES: &[&str] = &["chat"];

/// Provider 适配器 trait
///
/// 所有方法均为同步；异步由流本身承载。
/// 结构体统一定义在 `crate::types`，此处不定义业务结构体。
pub trait ProviderAdapter: Send + Sync {
    /// 构建上游 HTTP 请求
    fn build_request(
        &self,
        client: &reqwest::Client,
        base_url: &str,
        api_key: &str,
        req: &ChatCompletionRequest,
        actual_model: &str,
    ) -> Result<reqwest::RequestBuilder>;

    /// 解析非流式响应
    fn parse_response(&self, body: Bytes, model: &str) -> Result<ChatCompletionResponse>;

    /// 解析流式响应，返回 chunk 流
    fn parse_stream(
        &self,
        response: reqwest::Response,
        model: &str,
    ) -> Result<BoxStream<'static, Result<ChatCompletionChunk>>>;

    /// Build an upstream /v1/responses request.
    fn build_responses_request(
        &self,
        _client: &reqwest::Client,
        _base_url: &str,
        _api_key: &str,
        _req: &serde_json::Value,
        _actual_model: &str,
    ) -> Result<reqwest::RequestBuilder> {
        Err(anyhow::anyhow!("responses endpoint is not supported"))
    }

    /// Build an upstream /v1/embeddings request.
    fn build_embeddings_request(
        &self,
        _client: &reqwest::Client,
        _base_url: &str,
        _api_key: &str,
        _req: &serde_json::Value,
        _actual_model: &str,
    ) -> Result<reqwest::RequestBuilder> {
        Err(anyhow::anyhow!("embeddings endpoint is not supported"))
    }

    /// Parse a provider-specific error payload into normalized routing semantics.
    fn parse_error(
        &self,
        status: StatusCode,
        _headers: &HeaderMap,
        body: &[u8],
    ) -> ProviderErrorInfo {
        parse_openai_compatible_error(status, body)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderErrorKind {
    InvalidRequest,
    Authentication,
    RateLimit,
    Server,
    Api,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderErrorInfo {
    pub kind: ProviderErrorKind,
    pub message: String,
    pub code: String,
}

impl ProviderErrorInfo {
    pub fn new(kind: ProviderErrorKind, message: impl Into<String>, code: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            code: code.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderStreamError {
    pub info: ProviderErrorInfo,
}

impl ProviderStreamError {
    pub fn new(info: ProviderErrorInfo) -> Self {
        Self { info }
    }
}

impl std::fmt::Display for ProviderStreamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.info.message)
    }
}

impl std::error::Error for ProviderStreamError {}

fn parse_openai_compatible_error(status: StatusCode, body: &[u8]) -> ProviderErrorInfo {
    let payload: serde_json::Value =
        serde_json::from_slice(body).unwrap_or_else(|_| serde_json::json!({}));
    let error_obj = payload.get("error").unwrap_or(&payload);
    let message = error_obj
        .get("message")
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| String::from_utf8_lossy(body).trim().to_string());
    let code = error_obj
        .get("code")
        .and_then(|value| value.as_str())
        .or_else(|| error_obj.get("type").and_then(|value| value.as_str()))
        .unwrap_or_else(|| default_error_code(status))
        .to_string();

    ProviderErrorInfo::new(status_to_provider_error_kind(status), message, code)
}

pub fn status_to_provider_error_kind(status: StatusCode) -> ProviderErrorKind {
    match status.as_u16() {
        400 | 404 | 413 | 422 => ProviderErrorKind::InvalidRequest,
        401 | 403 => ProviderErrorKind::Authentication,
        429 => ProviderErrorKind::RateLimit,
        500..=599 => ProviderErrorKind::Server,
        _ => ProviderErrorKind::Api,
    }
}

fn default_error_code(status: StatusCode) -> &'static str {
    match status_to_provider_error_kind(status) {
        ProviderErrorKind::InvalidRequest => "invalid_request_error",
        ProviderErrorKind::Authentication => "authentication_error",
        ProviderErrorKind::RateLimit => "rate_limit_error",
        ProviderErrorKind::Server => "server_error",
        ProviderErrorKind::Api => "api_error",
    }
}

/// 根据渠道类型获取对应适配器（零状态，全局静态实例）
pub fn get_adapter(channel_type: i16) -> &'static dyn ProviderAdapter {
    static ANTHROPIC: AnthropicAdapter = AnthropicAdapter;
    static GEMINI: GeminiAdapter = GeminiAdapter;
    static OPENAI: OpenAiAdapter = OpenAiAdapter;

    match channel_type {
        1 => &OPENAI,    // OpenAI / OpenAI 兼容
        3 => &ANTHROPIC, // Anthropic
        24 => &GEMINI,   // Gemini
        _ => &OPENAI,    // 默认 OpenAI 兼容
    }
}

pub fn provider_scope_allowlist(channel_type: i16) -> Option<&'static [&'static str]> {
    match channel_type {
        3 | 24 => Some(CHAT_ONLY_PROVIDER_SCOPES),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use summer_web::axum::http::StatusCode;

    #[test]
    fn get_adapter_openai() {
        let adapter = get_adapter(1);
        // 验证返回的是合法的 trait object
        let _ = format!("{:p}", adapter);
    }

    #[test]
    fn get_adapter_unknown_defaults_to_openai() {
        let a = get_adapter(1);
        let b = get_adapter(999);
        // 未知类型回退到 OpenAI，指向同一个静态实例
        assert!(std::ptr::eq(a, b));
    }

    #[test]
    fn get_adapter_anthropic() {
        let anthropic = get_adapter(3);
        let openai = get_adapter(1);
        assert!(!std::ptr::eq(anthropic, openai));
    }

    #[test]
    fn get_adapter_gemini() {
        let gemini = get_adapter(24);
        let openai = get_adapter(1);
        assert!(!std::ptr::eq(gemini, openai));
    }

    #[test]
    fn provider_scope_allowlist_restricts_anthropic_and_gemini_to_chat() {
        assert_eq!(provider_scope_allowlist(3), Some(&["chat"][..]));
        assert_eq!(provider_scope_allowlist(24), Some(&["chat"][..]));
    }

    #[test]
    fn provider_scope_allowlist_keeps_openai_unrestricted() {
        assert_eq!(provider_scope_allowlist(1), None);
        assert_eq!(provider_scope_allowlist(999), None);
    }

    #[test]
    fn anthropic_and_gemini_default_responses_request_to_unsupported() {
        let client = reqwest::Client::new();
        let payload = serde_json::json!({"model": "demo"});

        let anthropic_error = get_adapter(3)
            .build_responses_request(&client, "https://example.com", "sk-demo", &payload, "demo")
            .unwrap_err();
        assert!(anthropic_error
            .to_string()
            .contains("responses endpoint is not supported"));

        let gemini_error = get_adapter(24)
            .build_responses_request(&client, "https://example.com", "sk-demo", &payload, "demo")
            .unwrap_err();
        assert!(gemini_error
            .to_string()
            .contains("responses endpoint is not supported"));
    }

    #[test]
    fn anthropic_and_gemini_default_embeddings_request_to_unsupported() {
        let client = reqwest::Client::new();
        let payload = serde_json::json!({"model": "demo"});

        let anthropic_error = get_adapter(3)
            .build_embeddings_request(&client, "https://example.com", "sk-demo", &payload, "demo")
            .unwrap_err();
        assert!(anthropic_error
            .to_string()
            .contains("embeddings endpoint is not supported"));

        let gemini_error = get_adapter(24)
            .build_embeddings_request(&client, "https://example.com", "sk-demo", &payload, "demo")
            .unwrap_err();
        assert!(gemini_error
            .to_string()
            .contains("embeddings endpoint is not supported"));
    }

    #[test]
    fn anthropic_parse_error_maps_rate_limit_error() {
        let info = get_adapter(3).parse_error(
            StatusCode::TOO_MANY_REQUESTS,
            &HeaderMap::new(),
            br#"{"type":"error","error":{"type":"rate_limit_error","message":"too many requests"}}"#,
        );

        assert_eq!(info.kind, ProviderErrorKind::RateLimit);
        assert_eq!(info.code, "rate_limit_error");
        assert_eq!(info.message, "too many requests");
    }

    #[test]
    fn anthropic_parse_error_preserves_new_api_error_payload() {
        let info = get_adapter(3).parse_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            &HeaderMap::new(),
            br#"{"error":{"type":"new_api_error","message":"invalid claude code request"},"type":"error"}"#,
        );

        assert_eq!(info.kind, ProviderErrorKind::Server);
        assert_eq!(info.code, "new_api_error");
        assert_eq!(info.message, "invalid claude code request");
    }

    #[test]
    fn gemini_parse_error_maps_invalid_argument() {
        let info = get_adapter(24).parse_error(
            StatusCode::BAD_REQUEST,
            &HeaderMap::new(),
            br#"{"error":{"status":"INVALID_ARGUMENT","message":"bad request"}}"#,
        );

        assert_eq!(info.kind, ProviderErrorKind::InvalidRequest);
        assert_eq!(info.code, "INVALID_ARGUMENT");
        assert_eq!(info.message, "bad request");
    }

    #[test]
    fn openai_parse_error_uses_openai_compatible_shape() {
        let info = get_adapter(1).parse_error(
            StatusCode::UNAUTHORIZED,
            &HeaderMap::new(),
            br#"{"error":{"message":"bad key","type":"invalid_request_error","code":"invalid_api_key"}}"#,
        );

        assert_eq!(info.kind, ProviderErrorKind::Authentication);
        assert_eq!(info.code, "invalid_api_key");
        assert_eq!(info.message, "bad key");
    }
}
