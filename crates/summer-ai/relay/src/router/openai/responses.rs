use summer_ai_core::types::error::OpenAiApiResult;
use summer_ai_core::types::responses::ResponsesRequest;
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
use crate::service::chat::RelayChatContext;
use crate::service::responses::ResponsesRelayService;

pub fn routes() -> Router {
    Router::new().typed_route(responses)
}

#[post_api("/v1/responses")]
pub async fn responses(
    AiToken(token_info): AiToken,
    Component(svc): Component<ResponsesRelayService>,
    ClientIp(client_ip): ClientIp,
    headers: HeaderMap,
    Json(req): Json<ResponsesRequest>,
) -> OpenAiApiResult<Response> {
    svc.relay(
        RelayChatContext {
            token_info,
            client_ip: client_ip.to_string(),
            user_agent: UserAgentInfo::from_headers(&headers).raw,
            request_headers: headers,
        },
        req,
    )
    .await
}
