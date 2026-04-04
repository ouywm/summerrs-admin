use super::*;
use crate::service::openai_passthrough_relay::OpenAiPassthroughRelayService;

pub async fn list_batches(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiPassthroughRelayService>,
    ClientIp(client_ip): ClientIp,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_svc
        .relay_resource_get(
            token_info,
            client_ip,
            req,
            "/v1/batches".into(),
            ResourceRequestSpec {
                endpoint_scope: "batches",
                bind_resource_kind: None,
                delete_resource_kind: None,
            },
            Vec::new(),
        )
        .await
}

/// POST /v1/batches
#[post_api("/v1/batches")]
pub async fn create_batch(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiPassthroughRelayService>,
    ClientIp(client_ip): ClientIp,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> OpenAiApiResult<Response> {
    relay_svc
        .relay_resource_json_post(
            token_info,
            client_ip,
            headers,
            body,
            "/v1/batches".into(),
            ResourceRequestSpec {
                endpoint_scope: "batches",
                bind_resource_kind: Some("batch"),
                delete_resource_kind: None,
            },
            Vec::new(),
            None,
        )
        .await
}

/// GET /v1/batches/{batch_id}
#[get_api("/v1/batches/{batch_id}")]
pub async fn get_batch(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiPassthroughRelayService>,
    ClientIp(client_ip): ClientIp,
    Path(batch_id): Path<String>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_svc
        .relay_resource_get(
            token_info,
            client_ip,
            req,
            format!("/v1/batches/{batch_id}"),
            ResourceRequestSpec {
                endpoint_scope: "batches",
                bind_resource_kind: None,
                delete_resource_kind: None,
            },
            vec![("batch", batch_id)],
        )
        .await
}

/// POST /v1/batches/{batch_id}/cancel
#[post_api("/v1/batches/{batch_id}/cancel")]
pub async fn cancel_batch(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiPassthroughRelayService>,
    ClientIp(client_ip): ClientIp,
    Path(batch_id): Path<String>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_svc
        .relay_resource_bodyless_post(
            token_info,
            client_ip,
            req,
            format!("/v1/batches/{batch_id}/cancel"),
            ResourceRequestSpec {
                endpoint_scope: "batches",
                bind_resource_kind: None,
                delete_resource_kind: None,
            },
            vec![("batch", batch_id)],
        )
        .await
}
