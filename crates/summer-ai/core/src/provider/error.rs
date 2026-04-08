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

pub fn status_to_provider_error_kind(status: u16) -> ProviderErrorKind {
    match status {
        400 | 404 | 413 | 422 => ProviderErrorKind::InvalidRequest,
        401 | 403 => ProviderErrorKind::Authentication,
        429 => ProviderErrorKind::RateLimit,
        500..=599 => ProviderErrorKind::Server,
        _ => ProviderErrorKind::Api,
    }
}

pub(crate) fn parse_openai_compatible_error(status: u16, body: &[u8]) -> ProviderErrorInfo {
    let payload: serde_json::Value = serde_json::from_slice(body).unwrap_or_else(|error| {
        tracing::warn!(
            error = %error,
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

fn default_error_code(status: u16) -> &'static str {
    match status_to_provider_error_kind(status) {
        ProviderErrorKind::InvalidRequest => "invalid_request_error",
        ProviderErrorKind::Authentication => "authentication_error",
        ProviderErrorKind::RateLimit => "rate_limit_error",
        ProviderErrorKind::Server => "server_error",
        ProviderErrorKind::Api => "api_error",
    }
}
