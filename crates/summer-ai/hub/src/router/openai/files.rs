use summer_common::extractor::{ClientIp, Multipart, Path};
use summer_web::axum::extract::Request;
use summer_web::axum::http::HeaderMap;
use summer_web::axum::response::Response;
use summer_web::extractor::Component;
use summer_web::{delete_api, get_api, post_api};

use summer_ai_core::types::error::OpenAiApiResult;

use crate::auth::extractor::AiToken;
use crate::router::openai_passthrough::ResourceRequestSpec;
pub use crate::router::openai_passthrough::{
    cancel_batch as batches_cancel, create_batch as batches_create, get_batch as batches_get,
    list_batches as batches_list,
};
use crate::service::openai_passthrough_relay::OpenAiPassthroughRelayService;

/// GET /v1/files
#[get_api("/v1/files")]
#[allow(clippy::too_many_arguments)]
pub async fn files_list(
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
            "/v1/files".into(),
            ResourceRequestSpec {
                endpoint_scope: "files",
                bind_resource_kind: None,
                delete_resource_kind: None,
            },
            Vec::new(),
        )
        .await
}

/// POST /v1/files
#[post_api("/v1/files")]
#[allow(clippy::too_many_arguments)]
pub async fn files_upload(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiPassthroughRelayService>,
    ClientIp(client_ip): ClientIp,
    headers: HeaderMap,
    Multipart(multipart): Multipart,
) -> OpenAiApiResult<Response> {
    relay_svc
        .relay_resource_multipart_post(
            token_info,
            client_ip,
            headers,
            multipart,
            "/v1/files".into(),
            ResourceRequestSpec {
                endpoint_scope: "files",
                bind_resource_kind: Some("file"),
                delete_resource_kind: None,
            },
            Vec::new(),
            None,
        )
        .await
}

/// GET /v1/files/{file_id}
#[get_api("/v1/files/{file_id}")]
#[allow(clippy::too_many_arguments)]
pub async fn files_get(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiPassthroughRelayService>,
    ClientIp(client_ip): ClientIp,
    Path(file_id): Path<String>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_svc
        .relay_resource_get(
            token_info,
            client_ip,
            req,
            format!("/v1/files/{file_id}"),
            ResourceRequestSpec {
                endpoint_scope: "files",
                bind_resource_kind: None,
                delete_resource_kind: None,
            },
            vec![("file", file_id)],
        )
        .await
}

/// DELETE /v1/files/{file_id}
#[delete_api("/v1/files/{file_id}")]
#[allow(clippy::too_many_arguments)]
pub async fn files_delete(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiPassthroughRelayService>,
    ClientIp(client_ip): ClientIp,
    Path(file_id): Path<String>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_svc
        .relay_resource_delete(
            token_info,
            client_ip,
            req,
            format!("/v1/files/{file_id}"),
            ResourceRequestSpec {
                endpoint_scope: "files",
                bind_resource_kind: None,
                delete_resource_kind: Some("file"),
            },
            vec![("file", file_id.clone())],
            Some(("file", file_id)),
        )
        .await
}

/// GET /v1/files/{file_id}/content
#[get_api("/v1/files/{file_id}/content")]
#[allow(clippy::too_many_arguments)]
pub async fn files_content(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiPassthroughRelayService>,
    ClientIp(client_ip): ClientIp,
    Path(file_id): Path<String>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_svc
        .relay_resource_get(
            token_info,
            client_ip,
            req,
            format!("/v1/files/{file_id}/content"),
            ResourceRequestSpec {
                endpoint_scope: "files",
                bind_resource_kind: None,
                delete_resource_kind: None,
            },
            vec![("file", file_id)],
        )
        .await
}
