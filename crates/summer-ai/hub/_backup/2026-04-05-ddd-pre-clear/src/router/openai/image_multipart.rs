use summer_ai_core::types::error::OpenAiApiResult;
use summer_common::extractor::{ClientIp, Multipart};
use summer_web::axum::http::HeaderMap;
use summer_web::axum::response::Response;
use summer_web::extractor::Component;
use summer_web::post_api;

use crate::auth::extractor::AiToken;
use crate::service::openai_image_multipart_relay::OpenAiImageMultipartRelayService;

/// POST /v1/images/edits
#[post_api("/v1/images/edits")]
pub async fn image_edits(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiImageMultipartRelayService>,
    ClientIp(client_ip): ClientIp,
    headers: HeaderMap,
    Multipart(multipart): Multipart,
) -> OpenAiApiResult<Response> {
    relay_svc
        .relay_edits(token_info, client_ip, headers, multipart)
        .await
}

/// POST /v1/images/variations
#[post_api("/v1/images/variations")]
pub async fn image_variations(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiImageMultipartRelayService>,
    ClientIp(client_ip): ClientIp,
    headers: HeaderMap,
    Multipart(multipart): Multipart,
) -> OpenAiApiResult<Response> {
    relay_svc
        .relay_variations(token_info, client_ip, headers, multipart)
        .await
}
