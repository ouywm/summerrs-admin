use summer_ai_core::types::chat::ChatCompletionRequest;
use summer_ai_core::types::error::OpenAiApiResult;
use summer_common::extractor::ClientIp;
use summer_common::response::Json;
use summer_common::user_agent::UserAgentInfo;
use summer_web::Router;
use summer_web::axum::http::HeaderMap;
use summer_web::axum::response::Response;
use summer_web::extractor::Component;
use summer_web::handler::TypeRouter;
use summer_web::post_api;

use crate::auth::extractor::AiToken;
use crate::service::chat::{ChatRelayService, RelayChatContext};

pub fn routes() -> Router {
    Router::new().typed_route(chat_completions)
}

#[post_api("/v1/chat/completions")]
pub async fn chat_completions(
    AiToken(token_info): AiToken,
    Component(svc): Component<ChatRelayService>,
    ClientIp(client_ip): ClientIp,
    headers: HeaderMap,
    Json(req): Json<ChatCompletionRequest>,
) -> OpenAiApiResult<Response> {
    let ctx = RelayChatContext {
        token_info,
        client_ip: client_ip.to_string(),
        user_agent: UserAgentInfo::from_headers(&headers).raw,
        request_headers: headers,
    };

    svc.relay(ctx, req).await
}
