use super::*;
use crate::service::openai_passthrough_relay::OpenAiPassthroughRelayService;

pub async fn get_response(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiPassthroughRelayService>,
    ClientIp(client_ip): ClientIp,
    Path(response_id): Path<String>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_svc
        .get_response(token_info, client_ip, response_id, req)
        .await
}

/// GET /v1/responses/{response_id}/input_items
#[get_api("/v1/responses/{response_id}/input_items")]
pub async fn get_response_input_items(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiPassthroughRelayService>,
    ClientIp(client_ip): ClientIp,
    Path(response_id): Path<String>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_svc
        .get_response_input_items(token_info, client_ip, response_id, req)
        .await
}

/// POST /v1/responses/{response_id}/cancel
#[post_api("/v1/responses/{response_id}/cancel")]
pub async fn cancel_response(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiPassthroughRelayService>,
    ClientIp(client_ip): ClientIp,
    Path(response_id): Path<String>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_svc
        .cancel_response(token_info, client_ip, response_id, req)
        .await
}
