use super::*;
use crate::service::openai_passthrough_relay::OpenAiPassthroughRelayService;

/// POST /v1/uploads
#[post_api("/v1/uploads")]
pub async fn create_upload(
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
            "/v1/uploads".into(),
            ResourceRequestSpec {
                endpoint_scope: "uploads",
                bind_resource_kind: Some("upload"),
                delete_resource_kind: None,
            },
            Vec::new(),
            None,
        )
        .await
}

/// GET /v1/uploads/{upload_id}
#[get_api("/v1/uploads/{upload_id}")]
pub async fn get_upload(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiPassthroughRelayService>,
    ClientIp(client_ip): ClientIp,
    Path(upload_id): Path<String>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_svc
        .relay_resource_get(
            token_info,
            client_ip,
            req,
            format!("/v1/uploads/{upload_id}"),
            ResourceRequestSpec {
                endpoint_scope: "uploads",
                bind_resource_kind: None,
                delete_resource_kind: None,
            },
            vec![("upload", upload_id)],
        )
        .await
}

/// POST /v1/uploads/{upload_id}/parts
#[post_api("/v1/uploads/{upload_id}/parts")]
pub async fn add_upload_part(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiPassthroughRelayService>,
    ClientIp(client_ip): ClientIp,
    Path(upload_id): Path<String>,
    headers: HeaderMap,
    Multipart(multipart): Multipart,
) -> OpenAiApiResult<Response> {
    relay_svc
        .relay_resource_multipart_post(
            token_info,
            client_ip,
            headers,
            multipart,
            format!("/v1/uploads/{upload_id}/parts"),
            ResourceRequestSpec {
                endpoint_scope: "uploads",
                bind_resource_kind: None,
                delete_resource_kind: None,
            },
            vec![("upload", upload_id)],
            None,
        )
        .await
}

/// POST /v1/uploads/{upload_id}/complete
#[post_api("/v1/uploads/{upload_id}/complete")]
pub async fn complete_upload(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiPassthroughRelayService>,
    ClientIp(client_ip): ClientIp,
    Path(upload_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> OpenAiApiResult<Response> {
    relay_svc
        .relay_resource_json_post(
            token_info,
            client_ip,
            headers,
            body,
            format!("/v1/uploads/{upload_id}/complete"),
            ResourceRequestSpec {
                endpoint_scope: "uploads",
                bind_resource_kind: Some("file"),
                delete_resource_kind: None,
            },
            vec![("upload", upload_id)],
            None,
        )
        .await
}

/// POST /v1/uploads/{upload_id}/cancel
#[post_api("/v1/uploads/{upload_id}/cancel")]
pub async fn cancel_upload(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiPassthroughRelayService>,
    ClientIp(client_ip): ClientIp,
    Path(upload_id): Path<String>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_svc
        .relay_resource_bodyless_post(
            token_info,
            client_ip,
            req,
            format!("/v1/uploads/{upload_id}/cancel"),
            ResourceRequestSpec {
                endpoint_scope: "uploads",
                bind_resource_kind: None,
                delete_resource_kind: None,
            },
            vec![("upload", upload_id)],
        )
        .await
}

/// DELETE /v1/models/{model}
#[delete_api("/v1/models/{model}")]
pub async fn delete_model(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiPassthroughRelayService>,
    ClientIp(client_ip): ClientIp,
    Path(model): Path<String>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_svc
        .relay_resource_delete(
            token_info,
            client_ip,
            req,
            format!("/v1/models/{model}"),
            ResourceRequestSpec {
                endpoint_scope: "models",
                bind_resource_kind: None,
                delete_resource_kind: None,
            },
            Vec::new(),
            None,
        )
        .await
}
