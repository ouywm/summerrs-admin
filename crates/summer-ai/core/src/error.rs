//! Adapter / Dispatcher 层错误类型。
//!
//! 这些错误属于"协议转换"与"上游 HTTP 非 2xx"两类；网络层错误（dns / connect /
//! timeout）由 Client 层的 `reqwest::Error` 包装，不在这里建模。

use thiserror::Error;

/// 协议/分派层错误。
#[derive(Debug, Error)]
pub enum AdapterError {
    #[error("serialize request: {0}")]
    SerializeRequest(#[source] serde_json::Error),
    #[error("deserialize response: {0}")]
    DeserializeResponse(#[source] serde_json::Error),
    #[error("invalid header value: {0}")]
    InvalidHeader(String),
    #[error("resolve auth: {0}")]
    ResolveAuth(#[from] AuthResolveError),
    #[error("upstream responded {status}: {message}")]
    UpstreamStatus { status: u16, message: String },
    #[error("feature `{feature}` not supported by adapter `{adapter}`")]
    Unsupported {
        adapter: &'static str,
        feature: &'static str,
    },
}

/// [`crate::AuthData::resolve`] 的失败类型。
#[derive(Debug, Error)]
pub enum AuthResolveError {
    #[error("environment variable `{0}` is not set")]
    MissingEnv(String),
    #[error("environment variable `{0}` is empty")]
    EmptyEnv(String),
}

pub type AdapterResult<T> = Result<T, AdapterError>;
