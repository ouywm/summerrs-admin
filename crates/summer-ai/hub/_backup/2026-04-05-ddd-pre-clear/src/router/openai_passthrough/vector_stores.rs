use super::*;
use crate::service::openai_passthrough_relay::OpenAiPassthroughRelayService;

pub async fn list_vector_stores(
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
            "/v1/vector_stores".into(),
            ResourceRequestSpec {
                endpoint_scope: "vector_stores",
                bind_resource_kind: None,
                delete_resource_kind: None,
            },
            Vec::new(),
        )
        .await
}

/// POST /v1/vector_stores
#[post_api("/v1/vector_stores")]
pub async fn create_vector_store(
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
            "/v1/vector_stores".into(),
            ResourceRequestSpec {
                endpoint_scope: "vector_stores",
                bind_resource_kind: Some("vector_store"),
                delete_resource_kind: None,
            },
            Vec::new(),
            None,
        )
        .await
}

/// GET /v1/vector_stores/{vector_store_id}
#[get_api("/v1/vector_stores/{vector_store_id}")]
pub async fn get_vector_store(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiPassthroughRelayService>,
    ClientIp(client_ip): ClientIp,
    Path(vector_store_id): Path<String>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_svc
        .relay_resource_get(
            token_info,
            client_ip,
            req,
            format!("/v1/vector_stores/{vector_store_id}"),
            ResourceRequestSpec {
                endpoint_scope: "vector_stores",
                bind_resource_kind: None,
                delete_resource_kind: None,
            },
            vec![("vector_store", vector_store_id)],
        )
        .await
}

/// POST /v1/vector_stores/{vector_store_id}
#[post_api("/v1/vector_stores/{vector_store_id}")]
pub async fn update_vector_store(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiPassthroughRelayService>,
    ClientIp(client_ip): ClientIp,
    Path(vector_store_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> OpenAiApiResult<Response> {
    relay_svc
        .relay_resource_json_post(
            token_info,
            client_ip,
            headers,
            body,
            format!("/v1/vector_stores/{vector_store_id}"),
            ResourceRequestSpec {
                endpoint_scope: "vector_stores",
                bind_resource_kind: Some("vector_store"),
                delete_resource_kind: None,
            },
            vec![("vector_store", vector_store_id)],
            None,
        )
        .await
}

/// DELETE /v1/vector_stores/{vector_store_id}
#[delete_api("/v1/vector_stores/{vector_store_id}")]
pub async fn delete_vector_store(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiPassthroughRelayService>,
    ClientIp(client_ip): ClientIp,
    Path(vector_store_id): Path<String>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_svc
        .relay_resource_delete(
            token_info,
            client_ip,
            req,
            format!("/v1/vector_stores/{vector_store_id}"),
            ResourceRequestSpec {
                endpoint_scope: "vector_stores",
                bind_resource_kind: None,
                delete_resource_kind: Some("vector_store"),
            },
            vec![("vector_store", vector_store_id.clone())],
            Some(("vector_store", vector_store_id)),
        )
        .await
}

/// POST /v1/vector_stores/{vector_store_id}/search
#[post_api("/v1/vector_stores/{vector_store_id}/search")]
pub async fn search_vector_store(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiPassthroughRelayService>,
    ClientIp(client_ip): ClientIp,
    Path(vector_store_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> OpenAiApiResult<Response> {
    relay_svc
        .relay_resource_json_post(
            token_info,
            client_ip,
            headers,
            body,
            format!("/v1/vector_stores/{vector_store_id}/search"),
            ResourceRequestSpec {
                endpoint_scope: "vector_stores",
                bind_resource_kind: None,
                delete_resource_kind: None,
            },
            vec![("vector_store", vector_store_id)],
            None,
        )
        .await
}

/// GET /v1/vector_stores/{vector_store_id}/files
#[get_api("/v1/vector_stores/{vector_store_id}/files")]
pub async fn list_vector_store_files(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiPassthroughRelayService>,
    ClientIp(client_ip): ClientIp,
    Path(vector_store_id): Path<String>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_svc
        .relay_resource_get(
            token_info,
            client_ip,
            req,
            format!("/v1/vector_stores/{vector_store_id}/files"),
            ResourceRequestSpec {
                endpoint_scope: "vector_stores",
                bind_resource_kind: None,
                delete_resource_kind: None,
            },
            vec![("vector_store", vector_store_id)],
        )
        .await
}

/// POST /v1/vector_stores/{vector_store_id}/files
#[post_api("/v1/vector_stores/{vector_store_id}/files")]
pub async fn create_vector_store_file(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiPassthroughRelayService>,
    ClientIp(client_ip): ClientIp,
    Path(vector_store_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> OpenAiApiResult<Response> {
    relay_svc
        .relay_resource_json_post(
            token_info,
            client_ip,
            headers,
            body,
            format!("/v1/vector_stores/{vector_store_id}/files"),
            ResourceRequestSpec {
                endpoint_scope: "vector_stores",
                bind_resource_kind: Some("file"),
                delete_resource_kind: None,
            },
            vec![("vector_store", vector_store_id)],
            None,
        )
        .await
}

/// GET /v1/vector_stores/{vector_store_id}/files/{file_id}
#[get_api("/v1/vector_stores/{vector_store_id}/files/{file_id}")]
pub async fn get_vector_store_file(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiPassthroughRelayService>,
    ClientIp(client_ip): ClientIp,
    Path((vector_store_id, file_id)): Path<(String, String)>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_svc
        .relay_resource_get(
            token_info,
            client_ip,
            req,
            format!("/v1/vector_stores/{vector_store_id}/files/{file_id}"),
            ResourceRequestSpec {
                endpoint_scope: "vector_stores",
                bind_resource_kind: None,
                delete_resource_kind: None,
            },
            vec![("file", file_id), ("vector_store", vector_store_id)],
        )
        .await
}

/// DELETE /v1/vector_stores/{vector_store_id}/files/{file_id}
#[delete_api("/v1/vector_stores/{vector_store_id}/files/{file_id}")]
pub async fn delete_vector_store_file(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiPassthroughRelayService>,
    ClientIp(client_ip): ClientIp,
    Path((vector_store_id, file_id)): Path<(String, String)>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_svc
        .relay_resource_delete(
            token_info,
            client_ip,
            req,
            format!("/v1/vector_stores/{vector_store_id}/files/{file_id}"),
            ResourceRequestSpec {
                endpoint_scope: "vector_stores",
                bind_resource_kind: None,
                delete_resource_kind: None,
            },
            vec![("file", file_id), ("vector_store", vector_store_id)],
            None,
        )
        .await
}

/// GET /v1/vector_stores/{vector_store_id}/file_batches
#[get_api("/v1/vector_stores/{vector_store_id}/file_batches")]
pub async fn list_vector_store_file_batches(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiPassthroughRelayService>,
    ClientIp(client_ip): ClientIp,
    Path(vector_store_id): Path<String>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_svc
        .relay_resource_get(
            token_info,
            client_ip,
            req,
            format!("/v1/vector_stores/{vector_store_id}/file_batches"),
            ResourceRequestSpec {
                endpoint_scope: "vector_stores",
                bind_resource_kind: None,
                delete_resource_kind: None,
            },
            vec![("vector_store", vector_store_id)],
        )
        .await
}

/// POST /v1/vector_stores/{vector_store_id}/file_batches
#[post_api("/v1/vector_stores/{vector_store_id}/file_batches")]
pub async fn create_vector_store_file_batch(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiPassthroughRelayService>,
    ClientIp(client_ip): ClientIp,
    Path(vector_store_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> OpenAiApiResult<Response> {
    relay_svc
        .relay_resource_json_post(
            token_info,
            client_ip,
            headers,
            body,
            format!("/v1/vector_stores/{vector_store_id}/file_batches"),
            ResourceRequestSpec {
                endpoint_scope: "vector_stores",
                bind_resource_kind: Some("batch"),
                delete_resource_kind: None,
            },
            vec![("vector_store", vector_store_id)],
            None,
        )
        .await
}

/// GET /v1/vector_stores/{vector_store_id}/file_batches/{batch_id}
#[get_api("/v1/vector_stores/{vector_store_id}/file_batches/{batch_id}")]
pub async fn get_vector_store_file_batch(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiPassthroughRelayService>,
    ClientIp(client_ip): ClientIp,
    Path((vector_store_id, batch_id)): Path<(String, String)>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_svc
        .relay_resource_get(
            token_info,
            client_ip,
            req,
            format!("/v1/vector_stores/{vector_store_id}/file_batches/{batch_id}"),
            ResourceRequestSpec {
                endpoint_scope: "vector_stores",
                bind_resource_kind: None,
                delete_resource_kind: None,
            },
            vec![("batch", batch_id), ("vector_store", vector_store_id)],
        )
        .await
}

/// POST /v1/vector_stores/{vector_store_id}/file_batches/{batch_id}/cancel
#[post_api("/v1/vector_stores/{vector_store_id}/file_batches/{batch_id}/cancel")]
pub async fn cancel_vector_store_file_batch(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiPassthroughRelayService>,
    ClientIp(client_ip): ClientIp,
    Path((vector_store_id, batch_id)): Path<(String, String)>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_svc
        .relay_resource_bodyless_post(
            token_info,
            client_ip,
            req,
            format!("/v1/vector_stores/{vector_store_id}/file_batches/{batch_id}/cancel"),
            ResourceRequestSpec {
                endpoint_scope: "vector_stores",
                bind_resource_kind: None,
                delete_resource_kind: None,
            },
            vec![("batch", batch_id), ("vector_store", vector_store_id)],
        )
        .await
}
