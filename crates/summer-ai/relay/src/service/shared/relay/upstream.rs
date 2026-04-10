use summer_ai_core::provider::{ProviderErrorInfo, ProviderErrorKind};
use summer_ai_core::types::error::OpenAiErrorResponse;
use summer_web::axum::http::HeaderMap;

pub(crate) fn extract_upstream_request_id(headers: &HeaderMap) -> Option<String> {
    ["x-request-id", "request-id", "anthropic-request-id"]
        .into_iter()
        .find_map(|name| headers.get(name))
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub(crate) fn provider_error_to_openai_response(
    status: u16,
    info: &ProviderErrorInfo,
) -> OpenAiErrorResponse {
    let error_type = match info.kind {
        ProviderErrorKind::InvalidRequest => "invalid_request_error",
        ProviderErrorKind::Authentication => "authentication_error",
        ProviderErrorKind::RateLimit => "rate_limit_error",
        ProviderErrorKind::Server => "server_error",
        ProviderErrorKind::Api => "api_error",
    };

    let normalized_status = match info.kind {
        ProviderErrorKind::InvalidRequest => match status {
            404 => 404,
            400 | 413 | 422 => status,
            _ => 400,
        },
        ProviderErrorKind::Authentication => match status {
            403 => 403,
            _ => 401,
        },
        ProviderErrorKind::RateLimit => 429,
        ProviderErrorKind::Server => {
            if (500..=599).contains(&status) {
                status
            } else {
                502
            }
        }
        ProviderErrorKind::Api => {
            if status == 0 || (200..300).contains(&status) {
                502
            } else {
                status
            }
        }
    };

    OpenAiErrorResponse {
        status: normalized_status,
        error: summer_ai_core::types::error::OpenAiError {
            error: summer_ai_core::types::error::OpenAiErrorBody {
                message: info.message.clone(),
                r#type: error_type.to_string(),
                param: None,
                code: Some(info.code.to_lowercase()),
            },
        },
    }
}

pub(crate) fn is_retryable_upstream_error(error: &OpenAiErrorResponse) -> bool {
    error.status == 0 || error.status == 429 || error.status >= 500
}

pub(crate) fn stream_error_status_code(error: &anyhow::Error) -> i32 {
    error
        .downcast_ref::<summer_ai_core::provider::ProviderStreamError>()
        .map(|error| match error.info.kind {
            summer_ai_core::provider::ProviderErrorKind::InvalidRequest => 400,
            summer_ai_core::provider::ProviderErrorKind::Authentication => 401,
            summer_ai_core::provider::ProviderErrorKind::RateLimit => 429,
            summer_ai_core::provider::ProviderErrorKind::Server
            | summer_ai_core::provider::ProviderErrorKind::Api => 502,
        })
        .unwrap_or(0)
}

pub(crate) fn stream_error_message(error: &anyhow::Error) -> String {
    error
        .downcast_ref::<summer_ai_core::provider::ProviderStreamError>()
        .map(|error| error.info.message.clone())
        .unwrap_or_else(|| error.to_string())
}
