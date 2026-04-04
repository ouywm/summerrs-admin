use super::*;
use crate::service::openai_passthrough_relay::OpenAiPassthroughRelayService;

/// GET /v1/fine_tuning/jobs
#[get_api("/v1/fine_tuning/jobs")]
pub async fn list_fine_tuning_jobs(
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
            "/v1/fine_tuning/jobs".into(),
            ResourceRequestSpec {
                endpoint_scope: "fine_tuning",
                bind_resource_kind: None,
                delete_resource_kind: None,
            },
            Vec::new(),
        )
        .await
}

/// POST /v1/fine_tuning/jobs
#[post_api("/v1/fine_tuning/jobs")]
pub async fn create_fine_tuning_job(
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
            "/v1/fine_tuning/jobs".into(),
            ResourceRequestSpec {
                endpoint_scope: "fine_tuning",
                bind_resource_kind: Some("fine_tuning_job"),
                delete_resource_kind: None,
            },
            Vec::new(),
            None,
        )
        .await
}

/// GET /v1/fine_tuning/jobs/{job_id}
#[get_api("/v1/fine_tuning/jobs/{job_id}")]
pub async fn get_fine_tuning_job(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiPassthroughRelayService>,
    ClientIp(client_ip): ClientIp,
    Path(job_id): Path<String>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_svc
        .relay_resource_get(
            token_info,
            client_ip,
            req,
            format!("/v1/fine_tuning/jobs/{job_id}"),
            ResourceRequestSpec {
                endpoint_scope: "fine_tuning",
                bind_resource_kind: None,
                delete_resource_kind: None,
            },
            vec![("fine_tuning_job", job_id)],
        )
        .await
}

/// POST /v1/fine_tuning/jobs/{job_id}/cancel
#[post_api("/v1/fine_tuning/jobs/{job_id}/cancel")]
pub async fn cancel_fine_tuning_job(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiPassthroughRelayService>,
    ClientIp(client_ip): ClientIp,
    Path(job_id): Path<String>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_svc
        .relay_resource_bodyless_post(
            token_info,
            client_ip,
            req,
            format!("/v1/fine_tuning/jobs/{job_id}/cancel"),
            ResourceRequestSpec {
                endpoint_scope: "fine_tuning",
                bind_resource_kind: None,
                delete_resource_kind: None,
            },
            vec![("fine_tuning_job", job_id)],
        )
        .await
}

/// GET /v1/fine_tuning/jobs/{job_id}/events
#[get_api("/v1/fine_tuning/jobs/{job_id}/events")]
pub async fn list_fine_tuning_events(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiPassthroughRelayService>,
    ClientIp(client_ip): ClientIp,
    Path(job_id): Path<String>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_svc
        .relay_resource_get(
            token_info,
            client_ip,
            req,
            format!("/v1/fine_tuning/jobs/{job_id}/events"),
            ResourceRequestSpec {
                endpoint_scope: "fine_tuning",
                bind_resource_kind: None,
                delete_resource_kind: None,
            },
            vec![("fine_tuning_job", job_id)],
        )
        .await
}

/// GET /v1/fine_tuning/jobs/{job_id}/checkpoints
#[get_api("/v1/fine_tuning/jobs/{job_id}/checkpoints")]
pub async fn list_fine_tuning_checkpoints(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiPassthroughRelayService>,
    ClientIp(client_ip): ClientIp,
    Path(job_id): Path<String>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_svc
        .relay_resource_get(
            token_info,
            client_ip,
            req,
            format!("/v1/fine_tuning/jobs/{job_id}/checkpoints"),
            ResourceRequestSpec {
                endpoint_scope: "fine_tuning",
                bind_resource_kind: None,
                delete_resource_kind: None,
            },
            vec![("fine_tuning_job", job_id)],
        )
        .await
}
