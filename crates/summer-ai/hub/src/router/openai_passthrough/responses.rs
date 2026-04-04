use super::*;

pub async fn get_response(
    AiToken(token_info): AiToken,
    Component(response_bridge): Component<ResponseBridgeService>,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    Path(response_id): Path<String>,
    req: Request,
) -> OpenAiApiResult<Response> {
    let client_ip = client_ip.to_string();
    if let Some(snapshot) = response_bridge
        .get_response(&token_info, &response_id)
        .await
        .map_err(|error| map_response_bridge_error("failed to load bridged response", error))?
    {
        token_info
            .ensure_endpoint_allowed("responses")
            .map_err(|error| OpenAiErrorResponse::from_api_error(&error))?;
        token_svc.update_last_used_ip_async(token_info.token_id, client_ip.clone());
        let request_id = extract_request_id(req.headers());
        let mut response = Json(snapshot.payload).into_response();
        insert_request_id_header(&mut response, &request_id);
        insert_upstream_request_id_header(&mut response, &snapshot.upstream_request_id);
        return Ok(response);
    }

    relay_resource_get(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip,
        req,
        format!("/v1/responses/{response_id}"),
        ResourceRequestSpec {
            endpoint_scope: "responses",
            bind_resource_kind: None,
            delete_resource_kind: None,
        },
        vec![("response", response_id)],
    )
    .await
}

/// GET /v1/responses/{response_id}/input_items
#[get_api("/v1/responses/{response_id}/input_items")]
pub async fn get_response_input_items(
    AiToken(token_info): AiToken,
    Component(response_bridge): Component<ResponseBridgeService>,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    Path(response_id): Path<String>,
    req: Request,
) -> OpenAiApiResult<Response> {
    let client_ip = client_ip.to_string();
    if let Some(snapshot) = response_bridge
        .get_input_items(&token_info, &response_id)
        .await
        .map_err(|error| {
            map_response_bridge_error("failed to load bridged response input items", error)
        })?
    {
        token_info
            .ensure_endpoint_allowed("responses")
            .map_err(|error| OpenAiErrorResponse::from_api_error(&error))?;
        token_svc.update_last_used_ip_async(token_info.token_id, client_ip.clone());
        let request_id = extract_request_id(req.headers());
        let mut response = Json(snapshot.payload).into_response();
        insert_request_id_header(&mut response, &request_id);
        insert_upstream_request_id_header(&mut response, &snapshot.upstream_request_id);
        return Ok(response);
    }

    relay_resource_get(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip,
        req,
        format!("/v1/responses/{response_id}/input_items"),
        ResourceRequestSpec {
            endpoint_scope: "responses",
            bind_resource_kind: None,
            delete_resource_kind: None,
        },
        vec![("response", response_id)],
    )
    .await
}

/// POST /v1/responses/{response_id}/cancel
#[post_api("/v1/responses/{response_id}/cancel")]
pub async fn cancel_response(
    AiToken(token_info): AiToken,
    Component(response_bridge): Component<ResponseBridgeService>,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    Path(response_id): Path<String>,
    req: Request,
) -> OpenAiApiResult<Response> {
    let client_ip = client_ip.to_string();
    if let Some(snapshot) = response_bridge
        .cancel(&token_info, &response_id)
        .await
        .map_err(|error| map_response_bridge_error("failed to cancel bridged response", error))?
    {
        token_info
            .ensure_endpoint_allowed("responses")
            .map_err(|error| OpenAiErrorResponse::from_api_error(&error))?;
        token_svc.update_last_used_ip_async(token_info.token_id, client_ip.clone());
        let request_id = extract_request_id(req.headers());
        let mut response = Json(snapshot.payload).into_response();
        insert_request_id_header(&mut response, &request_id);
        insert_upstream_request_id_header(&mut response, &snapshot.upstream_request_id);
        return Ok(response);
    }

    relay_resource_bodyless_post(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip,
        req,
        format!("/v1/responses/{response_id}/cancel"),
        ResourceRequestSpec {
            endpoint_scope: "responses",
            bind_resource_kind: None,
            delete_resource_kind: None,
        },
        vec![("response", response_id)],
    )
    .await
}
