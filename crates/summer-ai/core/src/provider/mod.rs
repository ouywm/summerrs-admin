use anyhow::Result;
use bytes::Bytes;
use futures::stream::BoxStream;
use reqwest::header::HeaderMap;

use crate::types::chat::{ChatCompletionChunk, ChatCompletionRequest, ChatCompletionResponse};
use crate::types::common::{Message, Tool};
use crate::types::embedding::EmbeddingResponse;
use crate::types::responses::ResponsesRequest;

mod anthropic;
mod azure;
mod gemini;
mod openai;

pub use anthropic::AnthropicAdapter;
pub use azure::AzureOpenAiAdapter;
pub use gemini::GeminiAdapter;
pub use openai::OpenAiAdapter;

const CHAT_ONLY_PROVIDER_SCOPES: &[&str] = &["chat", "responses"];
const GEMINI_PROVIDER_SCOPES: &[&str] = &["chat", "responses", "embeddings"];
const OLLAMA_PROVIDER_SCOPES: &[&str] = &["chat", "embeddings"];

/// Static metadata for each known provider.
#[derive(Debug, Clone)]
pub struct ProviderMeta {
    /// Human-readable provider name.
    pub name: &'static str,
    /// Default base URL (user can override via channel config).
    pub default_base_url: &'static str,
    /// Supported endpoint scopes (empty = unrestricted / all OpenAI endpoints).
    pub supported_scopes: &'static [&'static str],
    /// Whether this provider uses the OpenAI-compatible API format.
    pub openai_compatible: bool,
}

/// Look up static metadata for a channel type.
///
/// Returns `None` for truly unknown types (0 or unrecognized values).
pub fn provider_meta(channel_type: i16) -> Option<&'static ProviderMeta> {
    static PROVIDERS: &[ProviderMeta] = &[
        // 1 - OpenAI
        ProviderMeta {
            name: "OpenAI",
            default_base_url: "https://api.openai.com",
            supported_scopes: &[],
            openai_compatible: true,
        },
        // 3 - Anthropic
        ProviderMeta {
            name: "Anthropic",
            default_base_url: "https://api.anthropic.com",
            supported_scopes: &["chat", "responses"],
            openai_compatible: false,
        },
        // 14 - Azure OpenAI
        ProviderMeta {
            name: "Azure OpenAI",
            default_base_url: "",
            supported_scopes: &[],
            openai_compatible: false,
        },
        // 15 - Baidu
        ProviderMeta {
            name: "百度文心",
            default_base_url: "https://aip.baidubce.com",
            supported_scopes: &["chat"],
            openai_compatible: true,
        },
        // 17 - Ali
        ProviderMeta {
            name: "阿里通义",
            default_base_url: "https://dashscope.aliyuncs.com/compatible-mode",
            supported_scopes: &[],
            openai_compatible: true,
        },
        // 24 - Gemini
        ProviderMeta {
            name: "Google Gemini",
            default_base_url: "https://generativelanguage.googleapis.com",
            supported_scopes: &["chat", "responses", "embeddings"],
            openai_compatible: false,
        },
        // 28 - Ollama
        ProviderMeta {
            name: "Ollama",
            default_base_url: "http://localhost:11434",
            supported_scopes: &["chat", "embeddings"],
            openai_compatible: true,
        },
        // 30 - DeepSeek
        ProviderMeta {
            name: "DeepSeek",
            default_base_url: "https://api.deepseek.com",
            supported_scopes: &[],
            openai_compatible: true,
        },
        // 31 - Groq
        ProviderMeta {
            name: "Groq",
            default_base_url: "https://api.groq.com/openai",
            supported_scopes: &["chat"],
            openai_compatible: true,
        },
        // 32 - Mistral
        ProviderMeta {
            name: "Mistral",
            default_base_url: "https://api.mistral.ai",
            supported_scopes: &[],
            openai_compatible: true,
        },
        // 33 - SiliconFlow
        ProviderMeta {
            name: "SiliconFlow",
            default_base_url: "https://api.siliconflow.cn",
            supported_scopes: &[],
            openai_compatible: true,
        },
        // 34 - vLLM
        ProviderMeta {
            name: "vLLM",
            default_base_url: "http://localhost:8000",
            supported_scopes: &["chat", "embeddings"],
            openai_compatible: true,
        },
        // 35 - Fireworks
        ProviderMeta {
            name: "Fireworks AI",
            default_base_url: "https://api.fireworks.ai/inference",
            supported_scopes: &["chat", "embeddings"],
            openai_compatible: true,
        },
        // 36 - Together
        ProviderMeta {
            name: "Together AI",
            default_base_url: "https://api.together.xyz",
            supported_scopes: &["chat", "embeddings"],
            openai_compatible: true,
        },
        // 37 - OpenRouter
        ProviderMeta {
            name: "OpenRouter",
            default_base_url: "https://openrouter.ai/api",
            supported_scopes: &[],
            openai_compatible: true,
        },
        // 38 - Moonshot
        ProviderMeta {
            name: "Moonshot",
            default_base_url: "https://api.moonshot.cn",
            supported_scopes: &["chat"],
            openai_compatible: true,
        },
        // 39 - Lingyi
        ProviderMeta {
            name: "零一万物",
            default_base_url: "https://api.lingyiwanwu.com",
            supported_scopes: &["chat"],
            openai_compatible: true,
        },
        // 40 - Cohere
        ProviderMeta {
            name: "Cohere",
            default_base_url: "https://api.cohere.com/compatibility",
            supported_scopes: &["chat", "responses", "rerank", "embeddings"],
            openai_compatible: true,
        },
    ];

    // Map channel_type → index into PROVIDERS.
    let index = match channel_type {
        1 => 0,
        3 => 1,
        14 => 2,
        15 => 3,
        17 => 4,
        24 => 5,
        28 => 6,
        30 => 7,
        31 => 8,
        32 => 9,
        33 => 10,
        34 => 11,
        35 => 12,
        36 => 13,
        37 => 14,
        38 => 15,
        39 => 16,
        40 => 17,
        _ => return None,
    };
    PROVIDERS.get(index)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponsesRuntimeMode {
    Native,
    ChatBridge,
}

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

    fn responses_runtime_mode(&self) -> ResponsesRuntimeMode {
        ResponsesRuntimeMode::Native
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

    /// Parse an upstream /v1/embeddings-like response into the OpenAI-compatible shape.
    fn parse_embeddings_response(
        &self,
        body: Bytes,
        _model: &str,
        _estimated_prompt_tokens: i32,
    ) -> Result<EmbeddingResponse> {
        serde_json::from_slice(&body).map_err(Into::into)
    }

    /// Parse a provider-specific error payload into normalized routing semantics.
    fn parse_error(&self, status: u16, _headers: &HeaderMap, body: &[u8]) -> ProviderErrorInfo {
        parse_openai_compatible_error(status, body)
    }
}

pub(crate) fn responses_request_to_chat_request(req: &ResponsesRequest) -> ChatCompletionRequest {
    let mut messages = Vec::new();
    if let Some(instructions) = req.instructions.as_ref()
        && !instructions.is_empty()
    {
        messages.push(Message {
            role: "system".into(),
            content: serde_json::Value::String(instructions.clone()),
            name: None,
            tool_calls: None,
            tool_call_id: None,
        });
    }
    messages.extend(responses_input_to_messages(&req.input));

    let mut extra = req.extra.clone();
    if let Some(reasoning) = req.reasoning.as_ref() {
        extra.insert("reasoning".into(), reasoning.clone());
    }
    if let Some(metadata) = req.metadata.as_ref() {
        extra.insert("metadata".into(), metadata.clone());
    }

    ChatCompletionRequest {
        model: req.model.clone(),
        messages,
        stream: req.stream,
        temperature: req.temperature,
        max_tokens: req.max_output_tokens,
        top_p: req.top_p,
        frequency_penalty: None,
        presence_penalty: None,
        stop: None,
        tools: req
            .tools
            .as_ref()
            .and_then(|tools| {
                serde_json::from_value::<Vec<Tool>>(tools.clone())
                    .map_err(|e| {
                        tracing::warn!(error = %e, "failed to parse tools from request, ignoring tools");
                        e
                    })
                    .ok()
            }),
        tool_choice: req.tool_choice.clone(),
        response_format: req
            .text
            .as_ref()
            .and_then(|text| text.get("format"))
            .cloned(),
        stream_options: None,
        extra,
    }
}

fn responses_input_to_messages(input: &serde_json::Value) -> Vec<Message> {
    match input {
        serde_json::Value::Null => Vec::new(),
        serde_json::Value::String(text) => {
            vec![user_message(serde_json::Value::String(text.clone()))]
        }
        serde_json::Value::Array(items) => {
            let parsed: Option<Vec<Message>> =
                items.iter().map(response_input_item_to_message).collect();
            parsed.unwrap_or_else(|| vec![user_message(input.clone())])
        }
        _ => response_input_item_to_message(input)
            .map(|message| vec![message])
            .unwrap_or_else(|| vec![user_message(input.clone())]),
    }
}

fn response_input_item_to_message(value: &serde_json::Value) -> Option<Message> {
    if value.get("role").is_some() && value.get("content").is_some() {
        return serde_json::from_value::<Message>(value.clone()).ok();
    }

    let role = value.get("role").and_then(serde_json::Value::as_str)?;
    let content = value.get("content")?.clone();
    Some(Message {
        role: role.to_string(),
        content,
        name: None,
        tool_calls: None,
        tool_call_id: None,
    })
}

fn user_message(content: serde_json::Value) -> Message {
    Message {
        role: "user".into(),
        content,
        name: None,
        tool_calls: None,
        tool_call_id: None,
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
    pub fn new(
        kind: ProviderErrorKind,
        message: impl Into<String>,
        code: impl Into<String>,
    ) -> Self {
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

fn parse_openai_compatible_error(status: u16, body: &[u8]) -> ProviderErrorInfo {
    let payload: serde_json::Value = serde_json::from_slice(body).unwrap_or_else(|e| {
        tracing::warn!(
            error = %e,
            body_preview = %String::from_utf8_lossy(&body[..body.len().min(200)]),
            "failed to parse upstream error response as JSON"
        );
        serde_json::json!({})
    });
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

pub fn status_to_provider_error_kind(status: u16) -> ProviderErrorKind {
    match status {
        400 | 404 | 413 | 422 => ProviderErrorKind::InvalidRequest,
        401 | 403 => ProviderErrorKind::Authentication,
        429 => ProviderErrorKind::RateLimit,
        500..=599 => ProviderErrorKind::Server,
        _ => ProviderErrorKind::Api,
    }
}

fn default_error_code(status: u16) -> &'static str {
    match status_to_provider_error_kind(status) {
        ProviderErrorKind::InvalidRequest => "invalid_request_error",
        ProviderErrorKind::Authentication => "authentication_error",
        ProviderErrorKind::RateLimit => "rate_limit_error",
        ProviderErrorKind::Server => "server_error",
        ProviderErrorKind::Api => "api_error",
    }
}

fn merge_extra_body_fields(
    body: &mut serde_json::Value,
    extra: &serde_json::Map<String, serde_json::Value>,
) {
    let Some(body_obj) = body.as_object_mut() else {
        return;
    };

    for (key, value) in extra {
        body_obj.entry(key.clone()).or_insert_with(|| value.clone());
    }
}

/// 根据渠道类型获取对应适配器（零状态，全局静态实例）
///
/// OpenAI 兼容厂商（DeepSeek, Groq, Mistral 等）共享 OpenAI adapter，
/// 仅 base_url 和 api_key 不同，由渠道配置决定。
pub fn get_adapter(channel_type: i16) -> &'static dyn ProviderAdapter {
    static ANTHROPIC: AnthropicAdapter = AnthropicAdapter;
    static AZURE: AzureOpenAiAdapter = AzureOpenAiAdapter;
    static GEMINI: GeminiAdapter = GeminiAdapter;
    static OPENAI: OpenAiAdapter = OpenAiAdapter;

    match channel_type {
        3 => &ANTHROPIC, // Anthropic
        14 => &AZURE,    // Azure OpenAI
        24 => &GEMINI,   // Gemini
        // All others → OpenAI compatible (OpenAI, DeepSeek, Groq, Mistral,
        // Ollama, SiliconFlow, vLLM, Fireworks, Together, OpenRouter,
        // Ali, Baidu, Moonshot, Lingyi, Cohere, Unknown, etc.)
        _ => &OPENAI,
    }
}

/// Restrict which endpoint scopes a provider supports.
///
/// Returns `None` for OpenAI-compatible providers (unrestricted).
/// Returns `Some(allowlist)` for providers that only support specific endpoints.
pub fn provider_scope_allowlist(channel_type: i16) -> Option<&'static [&'static str]> {
    match channel_type {
        3 => Some(CHAT_ONLY_PROVIDER_SCOPES), // Anthropic: chat + responses only
        24 => Some(GEMINI_PROVIDER_SCOPES),   // Gemini: chat + responses + embeddings
        28 => Some(OLLAMA_PROVIDER_SCOPES),   // Ollama: chat + embeddings
        40 => Some(CHAT_ONLY_PROVIDER_SCOPES), // Cohere: chat + responses only
        _ => None,                            // OpenAI-compatible: unrestricted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn provider_scope_allowlist_keeps_bridged_responses_for_anthropic_and_gemini() {
        assert_eq!(
            provider_scope_allowlist(3),
            Some(&["chat", "responses"][..])
        );
        assert_eq!(
            provider_scope_allowlist(24),
            Some(&["chat", "responses", "embeddings"][..])
        );
        assert_eq!(
            provider_scope_allowlist(28),
            Some(&["chat", "embeddings"][..])
        );
    }

    #[test]
    fn provider_scope_allowlist_keeps_openai_compatible_unrestricted() {
        // OpenAI
        assert_eq!(provider_scope_allowlist(1), None);
        // DeepSeek
        assert_eq!(provider_scope_allowlist(30), None);
        // Groq
        assert_eq!(provider_scope_allowlist(31), None);
        // Mistral
        assert_eq!(provider_scope_allowlist(32), None);
        // Unknown
        assert_eq!(provider_scope_allowlist(999), None);
    }

    #[test]
    fn openai_compatible_providers_share_openai_adapter() {
        let openai = get_adapter(1);
        let deepseek = get_adapter(30);
        let groq = get_adapter(31);
        let mistral = get_adapter(32);
        let ollama = get_adapter(28);
        let siliconflow = get_adapter(33);

        // All OpenAI-compatible providers point to the same static instance.
        assert!(std::ptr::eq(openai, deepseek));
        assert!(std::ptr::eq(openai, groq));
        assert!(std::ptr::eq(openai, mistral));
        assert!(std::ptr::eq(openai, ollama));
        assert!(std::ptr::eq(openai, siliconflow));
    }

    #[test]
    fn provider_meta_returns_known_providers() {
        let openai = provider_meta(1).unwrap();
        assert_eq!(openai.name, "OpenAI");
        assert!(openai.openai_compatible);

        let anthropic = provider_meta(3).unwrap();
        assert_eq!(anthropic.name, "Anthropic");
        assert!(!anthropic.openai_compatible);

        let deepseek = provider_meta(30).unwrap();
        assert_eq!(deepseek.name, "DeepSeek");
        assert_eq!(deepseek.default_base_url, "https://api.deepseek.com");
        assert!(deepseek.openai_compatible);

        let groq = provider_meta(31).unwrap();
        assert_eq!(groq.name, "Groq");
        assert!(groq.openai_compatible);

        assert!(provider_meta(0).is_none());
        assert!(provider_meta(999).is_none());
    }

    #[test]
    fn azure_legacy_chat_request_uses_api_key_header_and_deployment_path() {
        let client = reqwest::Client::new();
        let req: ChatCompletionRequest = serde_json::from_value(serde_json::json!({
            "model": "gpt-4o",
            "messages": [{"role": "user", "content": "Hello"}]
        }))
        .unwrap();

        let built = get_adapter(14)
            .build_request(
                &client,
                "https://example-resource.openai.azure.com",
                "azure-key",
                &req,
                "gpt-4o-deployment",
            )
            .unwrap()
            .build()
            .unwrap();

        assert_eq!(
            built.url().as_str(),
            "https://example-resource.openai.azure.com/openai/deployments/gpt-4o-deployment/chat/completions?api-version=2024-10-21"
        );
        assert_eq!(
            built
                .headers()
                .get("api-key")
                .and_then(|value| value.to_str().ok()),
            Some("azure-key")
        );
        assert!(built.headers().get("authorization").is_none());
    }

    #[test]
    fn azure_v1_responses_request_uses_openai_v1_base_url() {
        let client = reqwest::Client::new();
        let payload = serde_json::json!({
            "model": "gpt-4.1",
            "input": "hello"
        });

        let built = get_adapter(14)
            .build_responses_request(
                &client,
                "https://example-resource.openai.azure.com/openai/v1/",
                "azure-key",
                &payload,
                "gpt-4.1-deployment",
            )
            .unwrap()
            .build()
            .unwrap();

        assert_eq!(
            built.url().as_str(),
            "https://example-resource.openai.azure.com/openai/v1/responses"
        );
        assert_eq!(
            built
                .headers()
                .get("api-key")
                .and_then(|value| value.to_str().ok()),
            Some("azure-key")
        );
        assert!(built.headers().get("authorization").is_none());

        let body_bytes = built.body().unwrap().as_bytes().unwrap();
        let body: serde_json::Value = serde_json::from_slice(body_bytes).unwrap();
        assert_eq!(body["model"], "gpt-4.1-deployment");
    }

    #[test]
    fn azure_legacy_embeddings_request_uses_api_key_header_and_deployment_path() {
        let client = reqwest::Client::new();
        let payload = serde_json::json!({
            "model": "text-embedding-3-large",
            "input": "hello"
        });

        let built = get_adapter(14)
            .build_embeddings_request(
                &client,
                "https://example-resource.openai.azure.com",
                "azure-key",
                &payload,
                "text-embedding-3-large-deployment",
            )
            .unwrap()
            .build()
            .unwrap();

        assert_eq!(
            built.url().as_str(),
            "https://example-resource.openai.azure.com/openai/deployments/text-embedding-3-large-deployment/embeddings?api-version=2024-10-21"
        );
        assert_eq!(
            built
                .headers()
                .get("api-key")
                .and_then(|value| value.to_str().ok()),
            Some("azure-key")
        );
        assert!(built.headers().get("authorization").is_none());
    }

    #[test]
    fn anthropic_and_gemini_responses_requests_bridge_to_chat_endpoints() {
        let client = reqwest::Client::new();
        let payload = serde_json::json!({
            "model": "demo",
            "input": "hello"
        });

        let anthropic = get_adapter(3)
            .build_responses_request(
                &client,
                "https://api.anthropic.com",
                "sk-demo",
                &payload,
                "claude-sonnet-4",
            )
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(
            anthropic.url().as_str(),
            "https://api.anthropic.com/v1/messages"
        );

        let gemini = get_adapter(24)
            .build_responses_request(
                &client,
                "https://generativelanguage.googleapis.com",
                "sk-demo",
                &payload,
                "gemini-2.5-pro",
            )
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(
            gemini.url().as_str(),
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-pro:generateContent"
        );
    }

    #[test]
    fn anthropic_default_embeddings_request_to_unsupported() {
        let client = reqwest::Client::new();
        let payload = serde_json::json!({"model": "demo"});

        let anthropic_error = get_adapter(3)
            .build_embeddings_request(&client, "https://example.com", "sk-demo", &payload, "demo")
            .unwrap_err();
        assert!(
            anthropic_error
                .to_string()
                .contains("embeddings endpoint is not supported")
        );
    }

    #[test]
    fn anthropic_parse_error_maps_rate_limit_error() {
        let info = get_adapter(3).parse_error(
            429,
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
            500,
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
            400,
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
            401,
            &HeaderMap::new(),
            br#"{"error":{"message":"bad key","type":"invalid_request_error","code":"invalid_api_key"}}"#,
        );

        assert_eq!(info.kind, ProviderErrorKind::Authentication);
        assert_eq!(info.code, "invalid_api_key");
        assert_eq!(info.message, "bad key");
    }
}
