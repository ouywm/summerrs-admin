//! summer-ai-relay 层错误。
//!
//! 分四类：
//!
//! - [`RelayError::Adapter`] —— 协议层（`build_chat_request` / `parse_chat_response` 失败）
//! - [`RelayError::Http`] —— 网络层（DNS / connect / timeout / read）
//! - [`RelayError::UpstreamStatus`] —— 上游非 2xx
//! - [`RelayError::MissingConfig`] —— 运行时必要环境变量缺失（P3 walking skeleton 用）
//!
//! 本错误会被 handler 映射成 axum [`Response`](summer_web::axum::response::Response)。

use bytes::Bytes;
use sea_orm::DbErr;
use summer_ai_core::AdapterError;
use summer_web::axum::Json;
use summer_web::axum::http::StatusCode;
use summer_web::axum::response::{IntoResponse, Response};

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
}

pub type RelayResult<T> = Result<T, RelayError>;

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
        }
    }

    /// 错误码（对应 OpenAI `error.type` 字段）。
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
        }
    }
}

impl IntoResponse for RelayError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let kind = self.kind();
        let message = self.to_string();

        tracing::warn!(%status, %kind, %message, "relay error");

        // 特例：上游非 2xx 时，**原样**透传 body（保留 OpenAI error 结构）
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

        let body = serde_json::json!({
            "error": {
                "message": message,
                "type": kind,
            }
        });
        (status, Json(body)).into_response()
    }
}
