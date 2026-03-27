use std::fmt::Display;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use summer_common::error::ApiErrors;
use summer_web::axum::http::StatusCode;
use summer_web::axum::response::{IntoResponse, Response};

pub type OpenAiApiResult<T, E = OpenAiErrorResponse> = Result<T, E>;

/// OpenAI 兼容错误响应体
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OpenAiError {
    pub error: OpenAiErrorBody,
}

/// OpenAI 兼容错误详情
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OpenAiErrorBody {
    pub message: String,
    pub r#type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub param: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

/// AI 接口专用错误返回类型
#[derive(Debug, Clone)]
pub struct OpenAiErrorResponse {
    pub status: StatusCode,
    pub error: OpenAiError,
}

impl OpenAiError {
    /// 构造 400 invalid_request_error
    pub fn invalid_request(msg: impl Into<String>) -> (StatusCode, Self) {
        (
            StatusCode::BAD_REQUEST,
            Self {
                error: OpenAiErrorBody {
                    message: msg.into(),
                    r#type: "invalid_request_error".into(),
                    param: None,
                    code: Some("invalid_request".into()),
                },
            },
        )
    }

    /// 构造 401 invalid_api_key
    pub fn invalid_api_key(msg: impl Into<String>) -> (StatusCode, Self) {
        (
            StatusCode::UNAUTHORIZED,
            Self {
                error: OpenAiErrorBody {
                    message: msg.into(),
                    r#type: "invalid_request_error".into(),
                    param: None,
                    code: Some("invalid_api_key".into()),
                },
            },
        )
    }

    /// 构造 429 insufficient_quota
    pub fn insufficient_quota(msg: impl Into<String>) -> (StatusCode, Self) {
        (
            StatusCode::TOO_MANY_REQUESTS,
            Self {
                error: OpenAiErrorBody {
                    message: msg.into(),
                    r#type: "insufficient_quota".into(),
                    param: None,
                    code: Some("insufficient_quota".into()),
                },
            },
        )
    }

    /// 构造 403 model_not_available
    pub fn model_not_available(msg: impl Into<String>) -> (StatusCode, Self) {
        (
            StatusCode::FORBIDDEN,
            Self {
                error: OpenAiErrorBody {
                    message: msg.into(),
                    r#type: "invalid_request_error".into(),
                    param: None,
                    code: Some("model_not_available".into()),
                },
            },
        )
    }

    /// 构造 404 not_found
    pub fn not_found(msg: impl Into<String>) -> (StatusCode, Self) {
        (
            StatusCode::NOT_FOUND,
            Self {
                error: OpenAiErrorBody {
                    message: msg.into(),
                    r#type: "invalid_request_error".into(),
                    param: None,
                    code: Some("not_found".into()),
                },
            },
        )
    }

    /// 构造 503 no_available_channel
    pub fn no_available_channel(msg: impl Into<String>) -> (StatusCode, Self) {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Self {
                error: OpenAiErrorBody {
                    message: msg.into(),
                    r#type: "server_error".into(),
                    param: None,
                    code: Some("no_available_channel".into()),
                },
            },
        )
    }

    /// 构造 504 upstream_timeout
    pub fn upstream_timeout(msg: impl Into<String>) -> (StatusCode, Self) {
        (
            StatusCode::GATEWAY_TIMEOUT,
            Self {
                error: OpenAiErrorBody {
                    message: msg.into(),
                    r#type: "server_error".into(),
                    param: None,
                    code: Some("upstream_timeout".into()),
                },
            },
        )
    }

    /// 构造 502 unsupported_endpoint
    pub fn unsupported_endpoint(msg: impl Into<String>) -> (StatusCode, Self) {
        (
            StatusCode::BAD_GATEWAY,
            Self {
                error: OpenAiErrorBody {
                    message: msg.into(),
                    r#type: "upstream_error".into(),
                    param: None,
                    code: Some("unsupported_endpoint".into()),
                },
            },
        )
    }

    /// 构造 429 rate_limit_exceeded
    pub fn rate_limit_exceeded(msg: impl Into<String>) -> (StatusCode, Self) {
        (
            StatusCode::TOO_MANY_REQUESTS,
            Self {
                error: OpenAiErrorBody {
                    message: msg.into(),
                    r#type: "rate_limit_exceeded".into(),
                    param: None,
                    code: Some("rate_limit_exceeded".into()),
                },
            },
        )
    }

    /// 构造 500 internal_error
    pub fn internal_error(msg: impl Into<String>) -> (StatusCode, Self) {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Self {
                error: OpenAiErrorBody {
                    message: msg.into(),
                    r#type: "server_error".into(),
                    param: None,
                    code: Some("internal_error".into()),
                },
            },
        )
    }

    /// 将 (StatusCode, OpenAiError) 工厂方法结果转为可直接响应的类型
    pub fn into_response_with_status(pair: (StatusCode, Self)) -> OpenAiErrorResponse {
        OpenAiErrorResponse {
            status: pair.0,
            error: pair.1,
        }
    }
}

impl IntoResponse for OpenAiError {
    fn into_response(self) -> Response {
        OpenAiErrorResponse {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            error: self,
        }
        .into_response()
    }
}

impl OpenAiErrorResponse {
    pub fn invalid_request(msg: impl Into<String>) -> Self {
        OpenAiError::into_response_with_status(OpenAiError::invalid_request(msg))
    }

    pub fn invalid_api_key(msg: impl Into<String>) -> Self {
        OpenAiError::into_response_with_status(OpenAiError::invalid_api_key(msg))
    }

    pub fn insufficient_quota(msg: impl Into<String>) -> Self {
        OpenAiError::into_response_with_status(OpenAiError::insufficient_quota(msg))
    }

    pub fn model_not_available(msg: impl Into<String>) -> Self {
        OpenAiError::into_response_with_status(OpenAiError::model_not_available(msg))
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        OpenAiError::into_response_with_status(OpenAiError::not_found(msg))
    }

    pub fn no_available_channel(msg: impl Into<String>) -> Self {
        OpenAiError::into_response_with_status(OpenAiError::no_available_channel(msg))
    }

    pub fn upstream_timeout(msg: impl Into<String>) -> Self {
        OpenAiError::into_response_with_status(OpenAiError::upstream_timeout(msg))
    }

    pub fn unsupported_endpoint(msg: impl Into<String>) -> Self {
        OpenAiError::into_response_with_status(OpenAiError::unsupported_endpoint(msg))
    }

    pub fn rate_limit_exceeded(msg: impl Into<String>) -> Self {
        OpenAiError::into_response_with_status(OpenAiError::rate_limit_exceeded(msg))
    }

    pub fn internal_error(msg: impl Into<String>) -> Self {
        OpenAiError::into_response_with_status(OpenAiError::internal_error(msg))
    }

    pub fn internal_with(context: &str, error: impl Display) -> Self {
        Self::internal_error(format!("{context}: {error}"))
    }

    pub fn from_api_error(error: &ApiErrors) -> Self {
        match error {
            ApiErrors::BadRequest(msg)
            | ApiErrors::Conflict(msg)
            | ApiErrors::IncompleteUpload(msg)
            | ApiErrors::ValidationFailed(msg) => Self::invalid_request(msg.clone()),
            ApiErrors::Unauthorized(msg) => Self::invalid_api_key(msg.clone()),
            ApiErrors::Forbidden(msg) => Self::model_not_available(msg.clone()),
            ApiErrors::NotFound(msg) => Self::not_found(msg.clone()),
            ApiErrors::TooManyRequests(msg) => Self::rate_limit_exceeded(msg.clone()),
            ApiErrors::ServiceUnavailable(msg) => Self::no_available_channel(msg.clone()),
            ApiErrors::Internal(err) => Self::internal_error(err.to_string()),
        }
    }

    pub fn from_quota_error(error: &ApiErrors) -> Self {
        match error {
            ApiErrors::Forbidden(msg) => Self::insufficient_quota(msg.clone()),
            ApiErrors::TooManyRequests(msg) => Self::rate_limit_exceeded(msg.clone()),
            _ => Self::from_api_error(error),
        }
    }
}

impl From<ApiErrors> for OpenAiErrorResponse {
    fn from(error: ApiErrors) -> Self {
        Self::from_api_error(&error)
    }
}

impl From<&ApiErrors> for OpenAiErrorResponse {
    fn from(error: &ApiErrors) -> Self {
        Self::from_api_error(error)
    }
}

impl IntoResponse for OpenAiErrorResponse {
    fn into_response(self) -> Response {
        let body = serde_json::to_string(&self.error).unwrap_or_default();
        Response::builder()
            .status(self.status)
            .header("content-type", "application/json")
            .body(body.into())
            .unwrap()
    }
}

impl summer_web::aide::OperationOutput for OpenAiErrorResponse {
    type Inner = OpenAiError;

    fn operation_response(
        ctx: &mut summer_web::aide::generate::GenContext,
        operation: &mut summer_web::aide::openapi::Operation,
    ) -> Option<summer_web::aide::openapi::Response> {
        <summer_web::axum::Json<OpenAiError> as summer_web::aide::OperationOutput>::operation_response(
            ctx, operation,
        )
    }

    fn inferred_responses(
        ctx: &mut summer_web::aide::generate::GenContext,
        operation: &mut summer_web::aide::openapi::Operation,
    ) -> Vec<(
        Option<summer_web::aide::openapi::StatusCode>,
        summer_web::aide::openapi::Response,
    )> {
        <summer_web::axum::Json<OpenAiError> as summer_web::aide::OperationOutput>::inferred_responses(
            ctx, operation,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_endpoint_error_uses_bad_gateway_contract() {
        let error = OpenAiErrorResponse::unsupported_endpoint("endpoint disabled");
        assert_eq!(error.status, StatusCode::BAD_GATEWAY);
        assert_eq!(error.error.error.r#type, "upstream_error");
        assert_eq!(
            error.error.error.code.as_deref(),
            Some("unsupported_endpoint")
        );
        assert_eq!(error.error.error.message, "endpoint disabled");
    }

    #[test]
    fn invalid_api_key_error() {
        let err = OpenAiErrorResponse::invalid_api_key("bad key");
        assert_eq!(err.status, StatusCode::UNAUTHORIZED);
        assert_eq!(err.error.error.r#type, "invalid_request_error");
        assert_eq!(err.error.error.code.as_deref(), Some("invalid_api_key"));
        assert_eq!(err.error.error.message, "bad key");
    }

    #[test]
    fn insufficient_quota_error() {
        let err = OpenAiErrorResponse::insufficient_quota("no quota");
        assert_eq!(err.status, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(err.error.error.code.as_deref(), Some("insufficient_quota"));
    }

    #[test]
    fn model_not_available_error() {
        let err = OpenAiErrorResponse::model_not_available("gpt-5 not found");
        assert_eq!(err.status, StatusCode::FORBIDDEN);
        assert_eq!(err.error.error.code.as_deref(), Some("model_not_available"));
    }

    #[test]
    fn serialize_error() {
        let err = OpenAiErrorResponse::invalid_api_key("test msg");
        let json = serde_json::to_value(&err.error).unwrap();
        assert_eq!(json["error"]["message"], "test msg");
        assert_eq!(json["error"]["type"], "invalid_request_error");
        assert_eq!(json["error"]["code"], "invalid_api_key");
        assert!(json["error"].get("param").is_none());
    }
}
