use crate::provider::ProviderErrorKind;

pub(super) fn anthropic_error_kind(error_type: &str) -> Option<ProviderErrorKind> {
    match error_type {
        "invalid_request_error" | "not_found_error" => Some(ProviderErrorKind::InvalidRequest),
        "authentication_error" | "permission_error" => Some(ProviderErrorKind::Authentication),
        "rate_limit_error" => Some(ProviderErrorKind::RateLimit),
        "overloaded_error" | "api_error" => Some(ProviderErrorKind::Server),
        _ => None,
    }
}

#[derive(Debug, serde::Serialize)]
pub(super) struct AnthropicRequest {
    pub model: String,
    pub messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    pub max_tokens: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequences: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<AnthropicTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub stream: bool,
}

#[derive(Debug, serde::Serialize)]
pub(super) struct AnthropicMessage {
    pub role: String,
    pub content: Vec<serde_json::Value>,
}

#[derive(Debug, serde::Serialize)]
pub(super) struct AnthropicTool {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub input_schema: serde_json::Value,
}

#[derive(Debug, serde::Deserialize)]
pub(super) struct AnthropicResponse {
    pub id: String,
    pub model: String,
    #[serde(default)]
    pub content: Vec<AnthropicContentBlock>,
    #[serde(default)]
    pub stop_reason: Option<String>,
    #[serde(default)]
    pub usage: AnthropicUsage,
}

#[derive(Debug, serde::Deserialize)]
pub(super) struct AnthropicContentBlock {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub _thinking: String,
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub input: serde_json::Value,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub(super) struct AnthropicUsage {
    #[serde(default)]
    pub input_tokens: i32,
    #[serde(default)]
    pub output_tokens: i32,
    #[serde(default)]
    pub cache_read_input_tokens: i32,
    #[serde(default)]
    pub cache_creation_input_tokens: i32,
}

#[derive(Debug, serde::Deserialize)]
pub(super) struct AnthropicStreamEnvelope {
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub index: Option<u64>,
    #[serde(default)]
    pub message: Option<AnthropicStreamMessage>,
    #[serde(default)]
    pub content_block: Option<AnthropicStreamContentBlock>,
    #[serde(default)]
    pub delta: Option<AnthropicStreamDelta>,
    #[serde(default)]
    pub usage: Option<AnthropicUsage>,
    #[serde(default)]
    pub error: Option<AnthropicStreamError>,
}

#[derive(Debug, serde::Deserialize)]
pub(super) struct AnthropicStreamMessage {
    pub id: String,
    pub model: String,
    #[serde(default)]
    pub usage: AnthropicUsage,
}

#[derive(Debug, serde::Deserialize)]
pub(super) struct AnthropicStreamContentBlock {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, serde::Deserialize)]
pub(super) struct AnthropicStreamDelta {
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub thinking: String,
    #[serde(default, rename = "partial_json")]
    pub partial_json: String,
    #[serde(default)]
    pub stop_reason: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub(super) struct AnthropicStreamError {
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub message: String,
}
