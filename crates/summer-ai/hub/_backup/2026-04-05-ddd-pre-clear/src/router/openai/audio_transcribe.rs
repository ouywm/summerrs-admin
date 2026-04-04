use summer_ai_core::types::error::OpenAiApiResult;
use summer_common::extractor::{ClientIp, Multipart};
use summer_web::axum::http::HeaderMap;
use summer_web::axum::response::Response;
use summer_web::extractor::Component;
use summer_web::post_api;

use crate::auth::extractor::AiToken;
use crate::service::openai_audio_multipart_relay::OpenAiAudioMultipartRelayService;

/// POST /v1/audio/transcriptions
#[post_api("/v1/audio/transcriptions")]
pub async fn audio_transcriptions(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiAudioMultipartRelayService>,
    ClientIp(client_ip): ClientIp,
    headers: HeaderMap,
    Multipart(multipart): Multipart,
) -> OpenAiApiResult<Response> {
    relay_svc
        .relay_transcriptions(token_info, client_ip, headers, multipart)
        .await
}

/// POST /v1/audio/translations
#[post_api("/v1/audio/translations")]
pub async fn audio_translations(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiAudioMultipartRelayService>,
    ClientIp(client_ip): ClientIp,
    headers: HeaderMap,
    Multipart(multipart): Multipart,
) -> OpenAiApiResult<Response> {
    relay_svc
        .relay_translations(token_info, client_ip, headers, multipart)
        .await
}
