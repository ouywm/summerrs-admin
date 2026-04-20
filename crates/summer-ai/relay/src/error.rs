//! summer-ai-relay 层错误。
//!
//! 分四类：
//!
//! - [`RelayError::Adapter`] —— 协议层（`build_chat_request` / `parse_chat_response` 失败）
//! - [`RelayError::Http`] —— 网络层（DNS / connect / timeout / read）
//! - [`RelayError::UpstreamStatus`] —— 上游非 2xx
//! - [`RelayError::MissingConfig`] —— 运行时必要环境变量缺失（P3 walking skeleton 用）
//!
//! 错误返回支持三家官方格式：OpenAI / Claude / Gemini，由 [`ErrorFlavor`] 决定。
//! 统一入口 [`RelayError::into_response_with`]；handler 外层包一层 `unwrap_or_else` 指定
//! flavor，`AiAuthLayer` 则用 [`ErrorFlavor::from_path`] 根据路径推断。
//!
//! `impl IntoResponse for RelayError` 默认走 OpenAI flavor，给没显式指定 flavor 的 handler
//! 兜底。

use bytes::Bytes;
use sea_orm::DbErr;
use serde_json::{Value, json};
use summer_ai_core::AdapterError;
use summer_web::axum::Json;
use summer_web::axum::http::StatusCode;
use summer_web::axum::response::{IntoResponse, Response};

/// 错误 JSON 格式风格——由入口协议决定。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorFlavor {
    /// OpenAI Chat Completions / Responses / Models:
    /// `{"error":{"message","type","code","param"}}`
    OpenAI,
    /// Anthropic Messages:
    /// `{"type":"error","error":{"type","message"}}`
    Claude,
    /// Gemini GenerateContent:
    /// `{"error":{"code","message","status"}}`
    Gemini,
}

impl ErrorFlavor {
    /// 根据请求路径推断风格。
    ///
    /// - `/v1beta/*`        → Gemini
    /// - `/v1/messages*`    → Claude
    /// - 其他（含 `/v1/chat/completions`、`/v1/responses`、`/v1/models`）→ OpenAI
    pub fn from_path(path: &str) -> Self {
        if path.starts_with("/v1beta/") {
            Self::Gemini
        } else if path.starts_with("/v1/messages") {
            Self::Claude
        } else {
            Self::OpenAI
        }
    }
}

/// relay 运行时错误。
#[derive(Debug, thiserror::Error)]
pub enum RelayError {
    #[error("adapter error: {0}")]
    Adapter(#[from] AdapterError),

    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("upstream responded {status}")]
    UpstreamStatus { status: u16, body: Bytes },

    #[error("database error: {0}")]
    Database(DbErr),

    #[error("redis error: {0}")]
    Redis(String),

    #[error("missing config: {0}")]
    MissingConfig(&'static str),

    #[error("no available channel for model `{model}`")]
    NoAvailableChannel { model: String },

    #[error("not authenticated: {0}")]
    Unauthenticated(&'static str),

    #[error("token expired")]
    TokenExpired,

    #[error("token quota exhausted")]
    QuotaExhausted,

    #[error("not implemented: {0}")]
    NotImplemented(&'static str),

    /// Stream 消费 / 字节处理错误——UTF-8 解码失败、SSE 段 JSON 解析失败等。
    /// 用于 JSON-array 模式收敛上游 SSE 时出现的格式异常。
    #[error("stream processing: {0}")]
    StreamProcessing(String),
}

pub type RelayResult<T> = Result<T, RelayError>;

/// OpenAI handler 的 Result 别名。
pub type OpenAIResult<T> = Result<T, OpenAIError>;
/// Claude handler 的 Result 别名。
pub type ClaudeResult<T> = Result<T, ClaudeError>;
/// Gemini handler 的 Result 别名。
pub type GeminiResult<T> = Result<T, GeminiError>;

impl RelayError {
    /// 映射到 HTTP 状态码。
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::Adapter(AdapterError::Unsupported { .. }) => StatusCode::BAD_REQUEST,
            Self::Adapter(AdapterError::SerializeRequest(_)) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Adapter(AdapterError::DeserializeResponse(_)) => StatusCode::BAD_GATEWAY,
            Self::Adapter(AdapterError::InvalidHeader(_)) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Adapter(AdapterError::ResolveAuth(_)) => StatusCode::UNAUTHORIZED,
            Self::Adapter(AdapterError::UpstreamStatus { status, .. }) => {
                StatusCode::from_u16(*status).unwrap_or(StatusCode::BAD_GATEWAY)
            }
            Self::Adapter(AdapterError::Network(_)) => StatusCode::BAD_GATEWAY,
            Self::Http(_) => StatusCode::BAD_GATEWAY,
            Self::UpstreamStatus { status, .. } => {
                StatusCode::from_u16(*status).unwrap_or(StatusCode::BAD_GATEWAY)
            }
            Self::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Redis(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::MissingConfig(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::NoAvailableChannel { .. } => StatusCode::SERVICE_UNAVAILABLE,
            Self::Unauthenticated(_) => StatusCode::UNAUTHORIZED,
            Self::TokenExpired => StatusCode::UNAUTHORIZED,
            Self::QuotaExhausted => StatusCode::PAYMENT_REQUIRED,
            Self::NotImplemented(_) => StatusCode::NOT_IMPLEMENTED,
            Self::StreamProcessing(_) => StatusCode::BAD_GATEWAY,
        }
    }

    /// OpenAI 风格 `error.type` / 内部日志 kind 标签。
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Adapter(AdapterError::Unsupported { .. }) => "unsupported_feature",
            Self::Adapter(AdapterError::SerializeRequest(_)) => "request_error",
            Self::Adapter(AdapterError::DeserializeResponse(_)) => "upstream_parse_error",
            Self::Adapter(AdapterError::InvalidHeader(_)) => "header_error",
            Self::Adapter(AdapterError::ResolveAuth(_)) => "auth_error",
            Self::Adapter(AdapterError::UpstreamStatus { .. }) => "upstream_error",
            Self::Adapter(AdapterError::Network(_)) => "upstream_unreachable",
            Self::Http(_) => "upstream_unreachable",
            Self::UpstreamStatus { .. } => "upstream_error",
            Self::Database(_) => "database_error",
            Self::Redis(_) => "redis_error",
            Self::MissingConfig(_) => "configuration_error",
            Self::NoAvailableChannel { .. } => "no_available_channel",
            Self::Unauthenticated(_) => "authentication_error",
            Self::TokenExpired => "token_expired",
            Self::QuotaExhausted => "insufficient_quota",
            Self::NotImplemented(_) => "not_implemented",
            Self::StreamProcessing(_) => "stream_processing_error",
        }
    }

    /// 面向用户展示的文案。
    ///
    /// 鉴权 / 配额 / 路由这类业务错误走"人话"；调试类（Database、Redis、MissingConfig、
    /// SerializeRequest 等）仍旧返回内部 `Display`，生产上再视情况脱敏。
    pub fn public_message(&self, flavor: ErrorFlavor) -> String {
        match self {
            Self::Unauthenticated(_) => match flavor {
                ErrorFlavor::OpenAI => "Incorrect API key provided. \
                    You can find your API key in your account settings."
                    .to_string(),
                ErrorFlavor::Claude => "invalid x-api-key".to_string(),
                ErrorFlavor::Gemini => {
                    "API key not valid. Please pass a valid API key.".to_string()
                }
            },
            Self::TokenExpired => match flavor {
                ErrorFlavor::OpenAI => "Your API key has expired.".to_string(),
                ErrorFlavor::Claude => "api key expired".to_string(),
                ErrorFlavor::Gemini => "API key expired.".to_string(),
            },
            Self::QuotaExhausted => match flavor {
                ErrorFlavor::OpenAI => "You exceeded your current quota, \
                    please check your plan and billing details."
                    .to_string(),
                ErrorFlavor::Claude => {
                    "Your credit balance is too low to access the API.".to_string()
                }
                ErrorFlavor::Gemini => "Quota exceeded.".to_string(),
            },
            Self::NoAvailableChannel { model } => {
                format!("The model `{model}` is currently unavailable. Please try again later.")
            }
            Self::NotImplemented(what) => format!("Not implemented: {what}"),
            // 其余按调试信息返回——`Display` impl 由 thiserror 生成
            _ => self.to_string(),
        }
    }

    /// 生成指定风格的 HTTP Response。
    pub fn into_response_with(self, flavor: ErrorFlavor) -> Response {
        let status = self.status_code();
        let message = self.public_message(flavor);

        tracing::warn!(
            %status,
            kind = self.kind(),
            ?flavor,
            %message,
            internal = %self,
            "relay error"
        );

        // 上游透传 body 原样回（上游已经是对应家族的错误 JSON）
        if let Self::UpstreamStatus { body, .. } = &self {
            use summer_web::axum::http::HeaderValue;
            use summer_web::axum::http::header;
            return (
                status,
                [(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("application/json"),
                )],
                body.clone(),
            )
                .into_response();
        }

        let body = match flavor {
            ErrorFlavor::OpenAI => self.openai_body(&message),
            ErrorFlavor::Claude => self.claude_body(&message),
            ErrorFlavor::Gemini => self.gemini_body(&message, status),
        };
        (status, Json(body)).into_response()
    }

    // ---------- body builders ----------

    fn openai_body(&self, message: &str) -> Value {
        json!({
            "error": {
                "message": message,
                "type": self.openai_type(),
                "code": self.openai_code(),
                "param": Value::Null,
            }
        })
    }

    fn openai_type(&self) -> &'static str {
        match self {
            Self::Unauthenticated(_) | Self::TokenExpired => "invalid_request_error",
            Self::QuotaExhausted => "insufficient_quota",
            Self::NoAvailableChannel { .. } => "api_error",
            Self::Adapter(AdapterError::Unsupported { .. }) => "invalid_request_error",
            _ => self.kind(),
        }
    }

    fn openai_code(&self) -> Value {
        match self {
            Self::Unauthenticated(_) => Value::String("invalid_api_key".into()),
            Self::TokenExpired => Value::String("expired_api_key".into()),
            Self::QuotaExhausted => Value::String("insufficient_quota".into()),
            Self::NoAvailableChannel { .. } => Value::String("model_not_found".into()),
            _ => Value::Null,
        }
    }

    fn claude_body(&self, message: &str) -> Value {
        json!({
            "type": "error",
            "error": {
                "type": self.claude_type(),
                "message": message,
            },
            "request_id": generate_request_id(),
        })
    }

    fn claude_type(&self) -> &'static str {
        match self {
            Self::Unauthenticated(_) | Self::TokenExpired => "authentication_error",
            // 402 Payment Required —— Claude 官方用 `billing_error`，不是 permission_error
            Self::QuotaExhausted => "billing_error",
            Self::NoAvailableChannel { .. } => "not_found_error",
            Self::Adapter(AdapterError::Unsupported { .. }) => "invalid_request_error",
            Self::Adapter(AdapterError::SerializeRequest(_))
            | Self::Adapter(AdapterError::DeserializeResponse(_)) => "invalid_request_error",
            Self::NotImplemented(_) => "invalid_request_error",
            Self::MissingConfig(_)
            | Self::Database(_)
            | Self::Redis(_)
            | Self::Adapter(AdapterError::InvalidHeader(_)) => "api_error",
            Self::StreamProcessing(_) => "api_error",
            Self::UpstreamStatus { .. } | Self::Http(_) | Self::Adapter(_) => "api_error",
        }
    }

    fn gemini_body(&self, message: &str, status: StatusCode) -> Value {
        json!({
            "error": {
                "code": status.as_u16(),
                "message": message,
                "status": self.gemini_status(),
                "details": [],
            }
        })
    }

    /// Gemini 沿用 Google API `google.rpc.Code` 字符串枚举。
    fn gemini_status(&self) -> &'static str {
        match self {
            Self::Unauthenticated(_) | Self::TokenExpired => "UNAUTHENTICATED",
            Self::QuotaExhausted => "RESOURCE_EXHAUSTED",
            Self::Adapter(AdapterError::Unsupported { .. })
            | Self::Adapter(AdapterError::SerializeRequest(_))
            | Self::Adapter(AdapterError::DeserializeResponse(_)) => "INVALID_ARGUMENT",
            Self::NoAvailableChannel { .. } => "UNAVAILABLE",
            Self::NotImplemented(_) => "UNIMPLEMENTED",
            Self::MissingConfig(_) | Self::Database(_) | Self::Redis(_) => "INTERNAL",
            Self::StreamProcessing(_) => "INTERNAL",
            Self::Http(_) | Self::Adapter(AdapterError::Network(_)) => "UNAVAILABLE",
            Self::UpstreamStatus { .. } | Self::Adapter(_) => "INTERNAL",
        }
    }
}

/// 生成 Claude 风格的 `request_id`——官方示例形如 `req_011CSHoEeqs5C35K2UUqR7Fy`。
///
/// 实现策略：纳秒时间戳 + 进程内自增计数器混合，hex 化后截断。不依赖 uuid/rand
/// crate；跨进程/跨机器的全局唯一性由 `(timestamp, process_id, counter)` 的组合近似保证。
/// 实际用途只是给 SDK 一个能打印的调试标识，无强唯一性要求。
fn generate_request_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("req_{:016x}{:08x}", nanos, n & 0xFFFF_FFFF)
}

/// OpenAI `/v1/chat/completions` · `/v1/responses` · `/v1/models` 入口的错误 newtype。
///
/// Handler 签名写 `OpenAIResult<Response>`，`?` 自动把 [`RelayError`] / [`AdapterError`]
/// 转成这里；`IntoResponse` 走 OpenAI 官方格式
/// `{"error":{"message","type","code","param"}}`。
#[derive(Debug)]
pub struct OpenAIError(pub RelayError);

impl From<RelayError> for OpenAIError {
    fn from(e: RelayError) -> Self {
        Self(e)
    }
}

impl From<AdapterError> for OpenAIError {
    fn from(e: AdapterError) -> Self {
        Self(e.into())
    }
}

impl IntoResponse for OpenAIError {
    fn into_response(self) -> Response {
        self.0.into_response_with(ErrorFlavor::OpenAI)
    }
}

/// Claude `/v1/messages` 入口的错误 newtype。
///
/// Handler 签名写成 `ClaudeResult<Response>`，`?` 自动把 [`RelayError`] / [`AdapterError`]
/// 转成这里；`IntoResponse` 走 Anthropic 官方格式
/// `{"type":"error","error":{"type","message"}}`。
#[derive(Debug)]
pub struct ClaudeError(pub RelayError);

impl From<RelayError> for ClaudeError {
    fn from(e: RelayError) -> Self {
        Self(e)
    }
}

impl From<AdapterError> for ClaudeError {
    fn from(e: AdapterError) -> Self {
        Self(e.into())
    }
}

impl IntoResponse for ClaudeError {
    fn into_response(self) -> Response {
        self.0.into_response_with(ErrorFlavor::Claude)
    }
}

/// Gemini `/v1beta/*` 入口的错误 newtype。
///
/// Handler 签名写成 `GeminiResult<Response>`，`?` 自动把 [`RelayError`] / [`AdapterError`]
/// 转成这里；`IntoResponse` 走 Google API 官方格式
/// `{"error":{"code","message","status"}}`。
#[derive(Debug)]
pub struct GeminiError(pub RelayError);

impl From<RelayError> for GeminiError {
    fn from(e: RelayError) -> Self {
        Self(e)
    }
}

impl From<AdapterError> for GeminiError {
    fn from(e: AdapterError) -> Self {
        Self(e.into())
    }
}

impl IntoResponse for GeminiError {
    fn into_response(self) -> Response {
        self.0.into_response_with(ErrorFlavor::Gemini)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flavor_from_path_routes_correctly() {
        assert_eq!(
            ErrorFlavor::from_path("/v1beta/models/gemini-2.5:generateContent"),
            ErrorFlavor::Gemini
        );
        assert_eq!(ErrorFlavor::from_path("/v1/messages"), ErrorFlavor::Claude);
        assert_eq!(
            ErrorFlavor::from_path("/v1/chat/completions"),
            ErrorFlavor::OpenAI
        );
        assert_eq!(ErrorFlavor::from_path("/v1/responses"), ErrorFlavor::OpenAI);
        assert_eq!(ErrorFlavor::from_path("/v1/models"), ErrorFlavor::OpenAI);
        assert_eq!(
            ErrorFlavor::from_path("/anything-else"),
            ErrorFlavor::OpenAI
        );
    }

    #[test]
    fn unauthenticated_openai_body_has_openai_shape() {
        let err = RelayError::Unauthenticated("missing header");
        let msg = err.public_message(ErrorFlavor::OpenAI);
        let body = err.openai_body(&msg);
        let err_obj = body.get("error").unwrap();
        assert!(
            err_obj
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap()
                .contains("API key")
        );
        assert_eq!(err_obj.get("type").unwrap(), "invalid_request_error");
        assert_eq!(err_obj.get("code").unwrap(), "invalid_api_key");
        assert!(err_obj.get("param").unwrap().is_null());
    }

    #[test]
    fn unauthenticated_claude_body_has_type_error_outer() {
        let err = RelayError::Unauthenticated("missing header");
        let msg = err.public_message(ErrorFlavor::Claude);
        let body = err.claude_body(&msg);
        assert_eq!(body.get("type").unwrap(), "error");
        let err_obj = body.get("error").unwrap();
        assert_eq!(err_obj.get("type").unwrap(), "authentication_error");
        assert_eq!(
            err_obj.get("message").and_then(|v| v.as_str()).unwrap(),
            "invalid x-api-key"
        );
    }

    #[test]
    fn unauthenticated_gemini_body_has_google_rpc_shape() {
        let err = RelayError::Unauthenticated("missing header");
        let msg = err.public_message(ErrorFlavor::Gemini);
        let status = err.status_code();
        let body = err.gemini_body(&msg, status);
        let err_obj = body.get("error").unwrap();
        assert_eq!(err_obj.get("code").and_then(|v| v.as_u64()), Some(401));
        assert_eq!(err_obj.get("status").unwrap(), "UNAUTHENTICATED");
        assert!(
            err_obj
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap()
                .contains("API key")
        );
    }

    #[test]
    fn quota_exhausted_gemini_status_is_resource_exhausted() {
        let err = RelayError::QuotaExhausted;
        assert_eq!(err.gemini_status(), "RESOURCE_EXHAUSTED");
        assert_eq!(err.status_code(), StatusCode::PAYMENT_REQUIRED);
    }

    #[test]
    fn token_expired_has_expired_code_in_openai() {
        let err = RelayError::TokenExpired;
        assert_eq!(err.openai_code(), Value::String("expired_api_key".into()));
    }

    #[test]
    fn no_available_channel_gemini_is_unavailable() {
        let err = RelayError::NoAvailableChannel {
            model: "gpt-4".into(),
        };
        assert_eq!(err.gemini_status(), "UNAVAILABLE");
        assert_eq!(err.status_code(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn quota_exhausted_claude_type_is_billing_error() {
        // Claude 官方 402 用 `billing_error`，不是 `permission_error`
        let err = RelayError::QuotaExhausted;
        assert_eq!(err.claude_type(), "billing_error");
        assert_eq!(err.status_code(), StatusCode::PAYMENT_REQUIRED);
    }

    #[test]
    fn claude_body_has_outer_request_id() {
        let err = RelayError::Unauthenticated("x");
        let body = err.claude_body(&err.public_message(ErrorFlavor::Claude));
        let rid = body.get("request_id").and_then(|v| v.as_str()).unwrap();
        assert!(rid.starts_with("req_"), "request_id should start with req_");
        assert!(rid.len() > 10);
    }

    #[test]
    fn gemini_body_has_empty_details_array() {
        let err = RelayError::Unauthenticated("x");
        let body = err.gemini_body(&err.public_message(ErrorFlavor::Gemini), err.status_code());
        let details = body
            .get("error")
            .and_then(|e| e.get("details"))
            .and_then(|d| d.as_array())
            .expect("error.details should be array");
        assert!(details.is_empty());
    }

    #[test]
    fn generate_request_id_is_monotonic_within_process() {
        let a = generate_request_id();
        let b = generate_request_id();
        assert_ne!(a, b);
        assert!(a.starts_with("req_"));
        assert!(b.starts_with("req_"));
    }

    #[test]
    fn stream_processing_maps_to_bad_gateway_across_flavors() {
        let err = RelayError::StreamProcessing("bad chunk".into());
        assert_eq!(err.status_code(), StatusCode::BAD_GATEWAY);
        assert_eq!(err.gemini_status(), "INTERNAL");
        assert_eq!(err.claude_type(), "api_error");
    }
}
