use summer_ai_core::types::error::OpenAiApiResult;
use summer_common::extractor::ClientIp;
use summer_common::response::Json;
use summer_web::axum::http::HeaderMap;
use summer_web::axum::response::Response;
use summer_web::extractor::Component;
use summer_web::post_api;

use crate::auth::extractor::AiToken;
use crate::service::openai_images_relay::OpenAiImagesRelayService;

/// POST /v1/images/generations
#[post_api("/v1/images/generations")]
pub async fn image_generations(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiImagesRelayService>,
    ClientIp(client_ip): ClientIp,
    headers: HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> OpenAiApiResult<Response> {
    relay_svc.relay(token_info, client_ip, headers, body).await
}
