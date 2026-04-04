use summer_web::axum::http::{HeaderMap, StatusCode};
use summer_web::axum::response::Response;
use summer_web::extractor::Component;
use summer_web::{get_api, post_api};

use summer_ai_core::provider::{ProviderErrorInfo, ProviderErrorKind, get_adapter};
use summer_ai_core::types::chat::ChatCompletionRequest;
use summer_ai_core::types::embedding::EmbeddingRequest;
use summer_ai_core::types::error::{
    OpenAiApiResult, OpenAiError, OpenAiErrorBody, OpenAiErrorResponse,
};
use summer_ai_core::types::model::ModelListResponse;
use summer_ai_core::types::responses::ResponsesRequest;

use crate::auth::extractor::AiToken;
use crate::relay::channel_router::RouteSelectionState;
use crate::service::model::ModelService;
use crate::service::openai_chat_relay::OpenAiChatRelayService;
use crate::service::openai_embeddings_relay::OpenAiEmbeddingsRelayService;
use crate::service::openai_responses_relay::OpenAiResponsesRelayService;
use crate::service::token::TokenService;
use summer_common::extractor::ClientIp;
use summer_common::response::Json;

mod audio;
mod audio_transcribe;
mod completions;
mod files;
mod image_multipart;
mod images;
mod moderations;
mod rerank;
#[cfg(test)]
mod tests;

pub use files::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UpstreamFailureScope {
    Account,
    Channel,
}

#[derive(Debug, Clone)]
pub(crate) struct UpstreamProviderFailure {
    pub scope: UpstreamFailureScope,
    pub error: OpenAiErrorResponse,
    pub message: String,
}

/// POST /v1/chat/completions
#[post_api("/v1/chat/completions")]
pub async fn chat_completions(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiChatRelayService>,
    ClientIp(client_ip): ClientIp,
    headers: HeaderMap,
    Json(req): Json<ChatCompletionRequest>,
) -> OpenAiApiResult<Response> {
    relay_svc.relay(token_info, client_ip, headers, req).await
}

#[allow(unused_imports)]
pub(crate) use crate::service::openai_http::{
    extract_request_id, extract_upstream_request_id, fallback_usage, insert_request_id_header,
    insert_upstream_request_id_header,
};
pub(crate) use crate::service::openai_responses_relay::{
    settle_usage_accounting, spawn_usage_accounting_task,
};

/// POST /v1/responses
#[post_api("/v1/responses")]
pub async fn responses(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiResponsesRelayService>,
    ClientIp(client_ip): ClientIp,
    headers: HeaderMap,
    Json(req): Json<ResponsesRequest>,
) -> OpenAiApiResult<Response> {
    relay_svc.relay(token_info, client_ip, headers, req).await
}

/// POST /v1/embeddings
#[post_api("/v1/embeddings")]
pub async fn embeddings(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiEmbeddingsRelayService>,
    ClientIp(client_ip): ClientIp,
    headers: HeaderMap,
    Json(req): Json<EmbeddingRequest>,
) -> OpenAiApiResult<Response> {
    relay_svc.relay(token_info, client_ip, headers, req).await
}
pub(crate) fn classify_upstream_provider_failure(
    channel_type: i16,
    status: StatusCode,
    headers: &HeaderMap,
    body: &[u8],
) -> UpstreamProviderFailure {
    let info = get_adapter(channel_type).parse_error(status.as_u16(), headers, body);
    let scope = match info.kind {
        ProviderErrorKind::InvalidRequest => UpstreamFailureScope::Channel,
        ProviderErrorKind::Authentication
        | ProviderErrorKind::RateLimit
        | ProviderErrorKind::Server
        | ProviderErrorKind::Api => UpstreamFailureScope::Account,
    };
    let message = if info.message.is_empty() {
        String::from_utf8_lossy(body).trim().to_string()
    } else {
        info.message.clone()
    };

    UpstreamProviderFailure {
        scope,
        error: provider_error_to_openai_response(status, &info),
        message,
    }
}

pub(crate) fn apply_upstream_failure_scope<T: RouteSelectionState>(
    exclusions: &mut T,
    channel: &crate::relay::channel_router::SelectedChannel,
    scope: UpstreamFailureScope,
) {
    match scope {
        UpstreamFailureScope::Account => exclusions.exclude_selected_account(channel),
        UpstreamFailureScope::Channel => exclusions.exclude_selected_channel(channel),
    }
}

fn provider_error_to_openai_response(
    status: StatusCode,
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
        ProviderErrorKind::InvalidRequest => match status.as_u16() {
            404 => StatusCode::NOT_FOUND,
            400 | 413 | 422 => status,
            _ => StatusCode::BAD_REQUEST,
        },
        ProviderErrorKind::Authentication => match status.as_u16() {
            403 => StatusCode::FORBIDDEN,
            _ => StatusCode::UNAUTHORIZED,
        },
        ProviderErrorKind::RateLimit => StatusCode::TOO_MANY_REQUESTS,
        ProviderErrorKind::Server => {
            if status.is_server_error() {
                status
            } else {
                StatusCode::BAD_GATEWAY
            }
        }
        ProviderErrorKind::Api => {
            if status.is_success() {
                StatusCode::BAD_GATEWAY
            } else {
                status
            }
        }
    };

    OpenAiErrorResponse {
        status: normalized_status.into(),
        error: OpenAiError {
            error: OpenAiErrorBody {
                message: info.message.clone(),
                r#type: error_type.into(),
                param: None,
                code: Some(info.code.to_lowercase()),
            },
        },
    }
}

/// GET /v1/models
#[get_api("/v1/models")]
pub async fn list_models(
    AiToken(token_info): AiToken,
    Component(model_svc): Component<ModelService>,
    Component(token_svc): Component<TokenService>,
    ClientIp(client_ip): ClientIp,
) -> OpenAiApiResult<Json<ModelListResponse>> {
    token_svc.update_last_used_ip_async(token_info.token_id, client_ip.to_string());

    let models = model_svc
        .list_available(&token_info.group)
        .await
        .map_err(|e| OpenAiErrorResponse::internal_with("failed to list available models", e))?;

    Ok(Json(models))
}

/// GET /v1/models/{model}
#[get_api("/v1/models/{model}")]
pub async fn retrieve_model(
    AiToken(token_info): AiToken,
    Component(model_svc): Component<ModelService>,
    Component(token_svc): Component<TokenService>,
    ClientIp(client_ip): ClientIp,
    summer_common::extractor::Path(model): summer_common::extractor::Path<String>,
) -> OpenAiApiResult<Json<summer_ai_core::types::model::ModelObject>> {
    token_svc.update_last_used_ip_async(token_info.token_id, client_ip.to_string());

    let model = model_svc
        .get_available(&token_info.group, &model)
        .await
        .map_err(|e| OpenAiErrorResponse::internal_with("failed to query available model", e))?
        .ok_or_else(|| OpenAiErrorResponse::not_found("model not found"))?;

    Ok(Json(model))
}
