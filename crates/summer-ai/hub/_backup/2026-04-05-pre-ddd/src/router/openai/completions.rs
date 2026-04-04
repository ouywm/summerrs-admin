use summer_ai_core::types::completion::CompletionRequest;
use summer_ai_core::types::error::OpenAiApiResult;
use summer_common::extractor::ClientIp;
use summer_common::response::Json;
use summer_web::axum::http::HeaderMap;
use summer_web::axum::response::Response;
use summer_web::extractor::Component;
use summer_web::post_api;

use crate::auth::extractor::AiToken;
use crate::service::openai_completions_relay::OpenAiCompletionsRelayService;

/// POST /v1/completions
#[post_api("/v1/completions")]
pub async fn completions(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiCompletionsRelayService>,
    ClientIp(client_ip): ClientIp,
    headers: HeaderMap,
    Json(req): Json<CompletionRequest>,
) -> OpenAiApiResult<Response> {
    relay_svc.relay(token_info, client_ip, headers, req).await
}
