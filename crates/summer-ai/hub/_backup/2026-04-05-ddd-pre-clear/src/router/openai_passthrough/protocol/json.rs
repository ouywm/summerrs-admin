use super::relay_stream::{
    bind_resource_affinities, build_generic_stream_response, ensure_json_model,
    estimate_json_tokens, estimate_total_tokens_for_rate_limit, extract_model_from_response_value,
    extract_usage_from_value, json_body_requests_stream, mapped_model, model_from_json_body,
    relay_resource_request, spawn_resource_usage_accounting_task,
};
use super::*;

#[allow(dead_code)]
pub(crate) async fn relay_json_model_request(
    token_info: TokenInfo,
    router_svc: ChannelRouter,
    billing: BillingEngine,
    rate_limiter: RateLimitEngine,
    http_client: UpstreamHttpClient,
    log_svc: LogService,
    channel_svc: ChannelService,
    token_svc: TokenService,
    resource_affinity: ResourceAffinityService,
    client_ip: String,
    headers: HeaderMap,
    mut body: Value,
    spec: JsonModelRelaySpec,
) -> OpenAiApiResult<Response> {
    let request_id = extract_request_id(&headers);
    let user_agent = UserAgentInfo::from_headers(&headers).raw;
    let requested_model = model_from_json_body(&body, spec.default_model)
        .ok_or_else(|| OpenAiErrorResponse::invalid_request("missing model"))?;
    token_info
        .ensure_endpoint_allowed(spec.endpoint_scope)
        .map_err(|error| OpenAiErrorResponse::from_api_error(&error))?;
    token_info
        .ensure_model_allowed(&requested_model)
        .map_err(|error| OpenAiErrorResponse::from_api_error(&error))?;
    ensure_json_model(&mut body, &requested_model)?;
    token_svc.update_last_used_ip_async(token_info.token_id, client_ip.clone());

    let model_config = billing
        .get_model_config_for_endpoint(&requested_model, spec.endpoint_scope)
        .await
        .map_err(|error| OpenAiErrorResponse::from_api_error(&error))?;
    let group_ratio = billing
        .get_group_ratio(&token_info.group)
        .await
        .map_err(|error| {
            OpenAiErrorResponse::internal_with("failed to load group pricing", error)
        })?;

    let estimated_tokens = estimate_json_tokens(&body);
    let estimated_total_tokens = estimate_total_tokens_for_rate_limit(&body);
    let is_stream = body.get("stream").and_then(Value::as_bool).unwrap_or(false);

    rate_limiter
        .reserve(&token_info, &request_id, estimated_total_tokens)
        .await
        .map_err(|error| OpenAiErrorResponse::from_quota_error(&error))?;

    let route_exclusions = RouteSelectionExclusions::default();
    let mut route_plan = router_svc
        .build_channel_plan_with_exclusions(
            &token_info.group,
            &requested_model,
            spec.endpoint_scope,
            &route_exclusions,
        )
        .await
        .map_err(|error| {
            OpenAiErrorResponse::internal_with("failed to build channel plan", error)
        })?;
    let max_retries = 3;
    let start = std::time::Instant::now();

    for attempt in 0..max_retries {
        let Some(channel) = route_plan.next() else {
            let _ = rate_limiter.finalize_failure_with_retry(&request_id).await;
            return Err(OpenAiErrorResponse::no_available_channel(
                "no available channel",
            ));
        };

        let actual_model = mapped_model(&channel, &requested_model);
        ensure_json_model(&mut body, &actual_model)?;

        let pre_consumed = match billing
            .pre_consume(
                &request_id,
                &token_info,
                estimated_tokens,
                model_config.input_ratio,
                group_ratio,
            )
            .await
        {
            Ok(pre_consumed) => pre_consumed,
            Err(error) => {
                let _ = rate_limiter.finalize_failure_with_retry(&request_id).await;
                return Err(OpenAiErrorResponse::from_quota_error(&error));
            }
        };

        let mut request_builder = http_client
            .client()
            .post(build_upstream_url(
                &channel.base_url,
                spec.upstream_path,
                None,
            ))
            .json(&body);
        request_builder =
            apply_upstream_auth(request_builder, channel.channel_type, &channel.api_key);
        request_builder = apply_forward_headers(request_builder, &headers, false);

        match request_builder.send().await {
            Ok(resp) if resp.status().is_success() => {
                let elapsed = start.elapsed().as_millis() as i64;
                let upstream_request_id = extract_upstream_request_id(resp.headers());

                if is_stream {
                    return Ok(build_generic_stream_response(
                        resp,
                        token_info,
                        pre_consumed,
                        Some(model_config),
                        group_ratio,
                        channel,
                        Some(requested_model),
                        estimated_tokens,
                        spec.endpoint,
                        spec.request_format,
                        elapsed,
                        client_ip,
                        log_svc,
                        channel_svc,
                        rate_limiter,
                        billing,
                        request_id,
                        upstream_request_id,
                        user_agent,
                        resource_affinity,
                        None,
                        spec.endpoint_scope,
                    ));
                }

                let status = resp.status();
                let content_type = resp.headers().get(CONTENT_TYPE).cloned();
                let body_bytes = match resp.bytes().await {
                    Ok(body) => body,
                    Err(error) => {
                        let _ = billing
                            .refund_with_retry(&request_id, token_info.token_id, pre_consumed)
                            .await;
                        channel_svc.record_relay_failure_async(
                            channel.channel_id,
                            channel.account_id,
                            elapsed,
                            0,
                            format!("failed to read upstream response: {error}"),
                        );
                        route_plan.exclude_selected_account(&channel);
                        if attempt == max_retries - 1 {
                            record_passthrough_failure(
                                &log_svc,
                                &token_info,
                                &channel,
                                spec.endpoint,
                                spec.request_format,
                                &requested_model,
                                &actual_model,
                                &model_config.model_name,
                                &request_id,
                                &upstream_request_id,
                                elapsed,
                                is_stream,
                                &client_ip,
                                &user_agent,
                                0,
                                format!("failed to read upstream response: {error}"),
                            );
                            let _ = rate_limiter.finalize_failure_with_retry(&request_id).await;
                            return Err(OpenAiErrorResponse::internal_with(
                                "failed to read upstream response",
                                error,
                            ));
                        }
                        continue;
                    }
                };
                let json_value = serde_json::from_slice::<Value>(&body_bytes).ok();
                if let Some(message) =
                    unusable_success_response_message(status, &body_bytes, spec.endpoint, false)
                {
                    let _ = billing
                        .refund_with_retry(&request_id, token_info.token_id, pre_consumed)
                        .await;
                    channel_svc.record_relay_failure_async(
                        channel.channel_id,
                        channel.account_id,
                        elapsed,
                        status.as_u16() as i32,
                        message.clone(),
                    );
                    route_plan.exclude_selected_channel(&channel);
                    if attempt == max_retries - 1 {
                        record_passthrough_failure(
                            &log_svc,
                            &token_info,
                            &channel,
                            spec.endpoint,
                            spec.request_format,
                            &requested_model,
                            &actual_model,
                            &model_config.model_name,
                            &request_id,
                            &upstream_request_id,
                            elapsed,
                            is_stream,
                            &client_ip,
                            &user_agent,
                            status.as_u16() as i32,
                            message.clone(),
                        );
                        let _ = rate_limiter.finalize_failure_with_retry(&request_id).await;
                        return Err(OpenAiErrorResponse::unsupported_endpoint(message));
                    }
                    continue;
                }
                let usage = json_value
                    .as_ref()
                    .and_then(extract_usage_from_value)
                    .unwrap_or_else(|| fallback_usage(estimated_tokens));
                let upstream_model = json_value
                    .as_ref()
                    .and_then(extract_model_from_response_value)
                    .unwrap_or(actual_model.clone());

                spawn_resource_usage_accounting_task(
                    billing,
                    rate_limiter,
                    log_svc,
                    channel_svc,
                    token_info,
                    channel,
                    Some(model_config),
                    group_ratio,
                    pre_consumed,
                    usage,
                    request_id.clone(),
                    upstream_request_id.clone(),
                    Some(requested_model),
                    upstream_model,
                    client_ip,
                    user_agent,
                    spec.endpoint,
                    spec.request_format,
                    elapsed,
                    0,
                    false,
                    spec.endpoint_scope,
                );

                let mut response =
                    build_bytes_response(status, body_bytes, content_type, &request_id);
                insert_upstream_request_id_header(&mut response, &upstream_request_id);
                return Ok(response);
            }
            Ok(resp) => {
                let elapsed = start.elapsed().as_millis() as i64;
                let status_code = resp.status().as_u16() as i32;
                let status = resp.status();
                let headers = resp.headers().clone();
                let body = resp.bytes().await.unwrap_or_default();
                let _ = billing
                    .refund_with_retry(&request_id, token_info.token_id, pre_consumed)
                    .await;
                let failure = classify_upstream_provider_failure(
                    channel.channel_type,
                    status,
                    &headers,
                    &body,
                );
                channel_svc.record_relay_failure_async(
                    channel.channel_id,
                    channel.account_id,
                    elapsed,
                    status_code,
                    failure.message.clone(),
                );
                apply_upstream_failure_scope(&mut route_plan, &channel, failure.scope);
                if attempt == max_retries - 1 {
                    record_passthrough_failure(
                        &log_svc,
                        &token_info,
                        &channel,
                        spec.endpoint,
                        spec.request_format,
                        &requested_model,
                        &actual_model,
                        &model_config.model_name,
                        &request_id,
                        &extract_upstream_request_id(&headers),
                        elapsed,
                        is_stream,
                        &client_ip,
                        &user_agent,
                        status_code,
                        failure.message.clone(),
                    );
                    let _ = rate_limiter.finalize_failure_with_retry(&request_id).await;
                    return Err(failure.error);
                }
            }
            Err(error) => {
                let elapsed = start.elapsed().as_millis() as i64;
                let _ = billing
                    .refund_with_retry(&request_id, token_info.token_id, pre_consumed)
                    .await;
                channel_svc.record_relay_failure_async(
                    channel.channel_id,
                    channel.account_id,
                    elapsed,
                    0,
                    error.to_string(),
                );
                route_plan.exclude_selected_account(&channel);
                if attempt == max_retries - 1 {
                    record_passthrough_failure(
                        &log_svc,
                        &token_info,
                        &channel,
                        spec.endpoint,
                        spec.request_format,
                        &requested_model,
                        &actual_model,
                        &model_config.model_name,
                        &request_id,
                        "",
                        elapsed,
                        is_stream,
                        &client_ip,
                        &user_agent,
                        0,
                        error.to_string(),
                    );
                    let _ = rate_limiter.finalize_failure_with_retry(&request_id).await;
                    return Err(OpenAiErrorResponse::internal_with(
                        "failed to send upstream request",
                        error,
                    ));
                }
            }
        }
    }

    let _ = rate_limiter.finalize_failure_with_retry(&request_id).await;
    Err(OpenAiErrorResponse::no_available_channel(
        "all channels failed",
    ))
}

pub(crate) async fn relay_resource_get(
    token_info: TokenInfo,
    router_svc: ChannelRouter,
    http_client: UpstreamHttpClient,
    channel_svc: ChannelService,
    token_svc: TokenService,
    resource_affinity: ResourceAffinityService,
    client_ip: String,
    req: Request,
    upstream_path: String,
    spec: ResourceRequestSpec,
    affinity_keys: Vec<(&'static str, String)>,
) -> OpenAiApiResult<Response> {
    relay_resource_request(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip,
        req.headers().clone(),
        req.uri().query().map(ToOwned::to_owned),
        Method::GET,
        upstream_path,
        spec,
        affinity_keys,
        None,
        None,
        None,
    )
    .await
}

pub(crate) async fn relay_resource_delete(
    token_info: TokenInfo,
    router_svc: ChannelRouter,
    http_client: UpstreamHttpClient,
    channel_svc: ChannelService,
    token_svc: TokenService,
    resource_affinity: ResourceAffinityService,
    client_ip: String,
    req: Request,
    upstream_path: String,
    spec: ResourceRequestSpec,
    affinity_keys: Vec<(&'static str, String)>,
    delete_affinity: Option<(&'static str, String)>,
) -> OpenAiApiResult<Response> {
    relay_resource_request(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip,
        req.headers().clone(),
        req.uri().query().map(ToOwned::to_owned),
        Method::DELETE,
        upstream_path,
        spec,
        affinity_keys,
        None,
        None,
        delete_affinity,
    )
    .await
}

pub(crate) async fn relay_resource_bodyless_post(
    token_info: TokenInfo,
    router_svc: ChannelRouter,
    http_client: UpstreamHttpClient,
    channel_svc: ChannelService,
    token_svc: TokenService,
    resource_affinity: ResourceAffinityService,
    client_ip: String,
    req: Request,
    upstream_path: String,
    spec: ResourceRequestSpec,
    affinity_keys: Vec<(&'static str, String)>,
) -> OpenAiApiResult<Response> {
    relay_resource_request(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip,
        req.headers().clone(),
        req.uri().query().map(ToOwned::to_owned),
        Method::POST,
        upstream_path,
        spec,
        affinity_keys,
        None,
        None,
        None,
    )
    .await
}

pub(crate) async fn relay_resource_json_post(
    token_info: TokenInfo,
    router_svc: ChannelRouter,
    http_client: UpstreamHttpClient,
    channel_svc: ChannelService,
    token_svc: TokenService,
    resource_affinity: ResourceAffinityService,
    client_ip: String,
    headers: HeaderMap,
    mut body: Value,
    upstream_path: String,
    spec: ResourceRequestSpec,
    affinity_keys: Vec<(&'static str, String)>,
    delete_affinity: Option<(&'static str, String)>,
) -> OpenAiApiResult<Response> {
    let requested_model = model_from_json_body(&body, None);
    if let Some(model) = requested_model.as_ref() {
        token_info
            .ensure_model_allowed(model)
            .map_err(|error| OpenAiErrorResponse::from_api_error(&error))?;
    }

    relay_resource_request(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip,
        headers,
        None,
        Method::POST,
        upstream_path,
        spec,
        affinity_keys,
        Some(&mut body),
        None,
        delete_affinity,
    )
    .await
}

pub(crate) async fn relay_usage_resource_json_post(
    token_info: TokenInfo,
    router_svc: ChannelRouter,
    billing: BillingEngine,
    rate_limiter: RateLimitEngine,
    http_client: UpstreamHttpClient,
    log_svc: LogService,
    channel_svc: ChannelService,
    token_svc: TokenService,
    resource_affinity: ResourceAffinityService,
    client_ip: String,
    headers: HeaderMap,
    mut body: Value,
    upstream_path: String,
    spec: ResourceRequestSpec,
    endpoint: &'static str,
    request_format: &'static str,
    affinity_keys: Vec<(&'static str, String)>,
    delete_affinity: Option<(&'static str, String)>,
) -> OpenAiApiResult<Response> {
    let requested_model = model_from_json_body(&body, None);
    let request_id = extract_request_id(&headers);
    let user_agent = UserAgentInfo::from_headers(&headers).raw;
    token_info
        .ensure_endpoint_allowed(spec.endpoint_scope)
        .map_err(|error| OpenAiErrorResponse::from_api_error(&error))?;
    if let Some(requested_model) = requested_model.as_ref() {
        token_info
            .ensure_model_allowed(requested_model)
            .map_err(|error| OpenAiErrorResponse::from_api_error(&error))?;
    }
    token_svc.update_last_used_ip_async(token_info.token_id, client_ip.clone());

    let model_config = if let Some(requested_model) = requested_model.as_ref() {
        Some(
            billing
                .get_model_config_for_endpoint(requested_model, spec.endpoint_scope)
                .await
                .map_err(|error| OpenAiErrorResponse::from_api_error(&error))?,
        )
    } else {
        None
    };
    let group_ratio = billing
        .get_group_ratio(&token_info.group)
        .await
        .map_err(|error| {
            OpenAiErrorResponse::internal_with("failed to load group pricing", error)
        })?;

    let estimated_tokens = estimate_json_tokens(&body);
    let estimated_total_tokens = estimate_total_tokens_for_rate_limit(&body);
    let is_stream = json_body_requests_stream(&body);

    rate_limiter
        .reserve(&token_info, &request_id, estimated_total_tokens)
        .await
        .map_err(|error| OpenAiErrorResponse::from_quota_error(&error))?;

    let mut route_state = ResourceRouteState::new(
        &token_info,
        &router_svc,
        spec.endpoint_scope,
        requested_model.as_deref(),
    )
    .await?;
    let max_retries = 3;
    let start = std::time::Instant::now();

    for attempt in 0..max_retries {
        let Some(channel) = route_state
            .select(&token_info, &resource_affinity, &affinity_keys, Some(&body))
            .await?
        else {
            let _ = rate_limiter.finalize_failure_with_retry(&request_id).await;
            return Err(OpenAiErrorResponse::no_available_channel(if attempt == 0 {
                "no available channel"
            } else {
                "all channels failed"
            }));
        };

        let actual_model = requested_model
            .as_ref()
            .map(|requested_model| mapped_model(&channel, requested_model));
        if let Some(actual_model) = actual_model.as_ref() {
            ensure_json_model(&mut body, actual_model)?;
        }

        let pre_consumed = if let Some(model_config) = model_config.as_ref() {
            match billing
                .pre_consume(
                    &request_id,
                    &token_info,
                    estimated_tokens,
                    model_config.input_ratio,
                    group_ratio,
                )
                .await
            {
                Ok(pre_consumed) => pre_consumed,
                Err(error) => {
                    let _ = rate_limiter.finalize_failure_with_retry(&request_id).await;
                    return Err(OpenAiErrorResponse::from_quota_error(&error));
                }
            }
        } else {
            0
        };

        let mut request_builder = http_client
            .client()
            .post(build_upstream_url(&channel.base_url, &upstream_path, None))
            .json(&body);
        request_builder =
            apply_upstream_auth(request_builder, channel.channel_type, &channel.api_key);
        request_builder = apply_forward_headers(request_builder, &headers, false);

        match request_builder.send().await {
            Ok(resp) if resp.status().is_success() => {
                let elapsed = start.elapsed().as_millis() as i64;
                let upstream_request_id = extract_upstream_request_id(resp.headers());

                if is_stream {
                    return Ok(build_generic_stream_response(
                        resp,
                        token_info,
                        pre_consumed,
                        model_config.clone(),
                        group_ratio,
                        channel,
                        requested_model.clone(),
                        estimated_tokens,
                        endpoint,
                        request_format,
                        elapsed,
                        client_ip,
                        log_svc,
                        channel_svc,
                        rate_limiter,
                        billing,
                        request_id,
                        upstream_request_id,
                        user_agent,
                        resource_affinity,
                        spec.bind_resource_kind,
                        spec.endpoint_scope,
                    ));
                }

                let status = resp.status();
                let content_type = resp.headers().get(CONTENT_TYPE).cloned();
                let body_bytes = match resp.bytes().await {
                    Ok(body) => body,
                    Err(error) => {
                        let _ = billing
                            .refund_with_retry(&request_id, token_info.token_id, pre_consumed)
                            .await;
                        channel_svc.record_relay_failure_async(
                            channel.channel_id,
                            channel.account_id,
                            elapsed,
                            0,
                            format!("failed to read upstream response: {error}"),
                        );
                        route_state.exclude_selected_account(&channel);
                        if attempt == max_retries - 1 {
                            record_passthrough_failure(
                                &log_svc,
                                &token_info,
                                &channel,
                                endpoint,
                                request_format,
                                requested_model.as_deref().unwrap_or_default(),
                                actual_model.as_deref().unwrap_or_default(),
                                model_config
                                    .as_ref()
                                    .map(|config| config.model_name.as_str())
                                    .unwrap_or_else(|| {
                                        requested_model.as_deref().unwrap_or_default()
                                    }),
                                &request_id,
                                &upstream_request_id,
                                elapsed,
                                is_stream,
                                &client_ip,
                                &user_agent,
                                0,
                                format!("failed to read upstream response: {error}"),
                            );
                            let _ = rate_limiter.finalize_failure_with_retry(&request_id).await;
                            return Err(OpenAiErrorResponse::internal_with(
                                "failed to read upstream response",
                                error,
                            ));
                        }
                        continue;
                    }
                };
                let json_value = serde_json::from_slice::<Value>(&body_bytes).ok();
                if let Some(message) =
                    unusable_success_response_message(status, &body_bytes, endpoint, false)
                {
                    let _ = billing
                        .refund_with_retry(&request_id, token_info.token_id, pre_consumed)
                        .await;
                    channel_svc.record_relay_failure_async(
                        channel.channel_id,
                        channel.account_id,
                        elapsed,
                        status.as_u16() as i32,
                        message.clone(),
                    );
                    route_state.exclude_selected_channel(&channel);
                    if attempt == max_retries - 1 {
                        record_passthrough_failure(
                            &log_svc,
                            &token_info,
                            &channel,
                            endpoint,
                            request_format,
                            requested_model.as_deref().unwrap_or_default(),
                            actual_model.as_deref().unwrap_or_default(),
                            model_config
                                .as_ref()
                                .map(|config| config.model_name.as_str())
                                .unwrap_or_else(|| requested_model.as_deref().unwrap_or_default()),
                            &request_id,
                            &upstream_request_id,
                            elapsed,
                            is_stream,
                            &client_ip,
                            &user_agent,
                            status.as_u16() as i32,
                            message.clone(),
                        );
                        let _ = rate_limiter.finalize_failure_with_retry(&request_id).await;
                        return Err(OpenAiErrorResponse::unsupported_endpoint(message));
                    }
                    continue;
                }

                if let Some(value) = json_value.as_ref() {
                    bind_resource_affinities(
                        &token_info,
                        &resource_affinity,
                        &channel,
                        spec.bind_resource_kind,
                        value,
                    )
                    .await;
                }

                if let Some((kind, id)) = delete_affinity.as_ref()
                    && let Err(error) = resource_affinity.delete(&token_info, kind, id).await
                {
                    tracing::warn!("failed to delete resource affinity: {error}");
                }

                let usage = json_value
                    .as_ref()
                    .and_then(extract_usage_from_value)
                    .unwrap_or_else(|| fallback_usage(estimated_tokens));
                let upstream_model = json_value
                    .as_ref()
                    .and_then(extract_model_from_response_value)
                    .or_else(|| actual_model.clone())
                    .unwrap_or_default();

                spawn_resource_usage_accounting_task(
                    billing,
                    rate_limiter,
                    log_svc,
                    channel_svc,
                    token_info,
                    channel,
                    model_config.clone(),
                    group_ratio,
                    pre_consumed,
                    usage,
                    request_id.clone(),
                    upstream_request_id.clone(),
                    requested_model.clone(),
                    upstream_model,
                    client_ip,
                    user_agent,
                    endpoint,
                    request_format,
                    elapsed,
                    0,
                    false,
                    spec.endpoint_scope,
                );

                let mut response =
                    build_bytes_response(status, body_bytes, content_type, &request_id);
                insert_upstream_request_id_header(&mut response, &upstream_request_id);
                return Ok(response);
            }
            Ok(resp) => {
                let elapsed = start.elapsed().as_millis() as i64;
                let status_code = resp.status().as_u16() as i32;
                let status = resp.status();
                let response_headers = resp.headers().clone();
                let response_body = resp.bytes().await.unwrap_or_default();
                let _ = billing
                    .refund_with_retry(&request_id, token_info.token_id, pre_consumed)
                    .await;
                let failure = classify_upstream_provider_failure(
                    channel.channel_type,
                    status,
                    &response_headers,
                    &response_body,
                );
                channel_svc.record_relay_failure_async(
                    channel.channel_id,
                    channel.account_id,
                    elapsed,
                    status_code,
                    failure.message.clone(),
                );
                apply_upstream_failure_scope(&mut route_state, &channel, failure.scope);
                if attempt == max_retries - 1 {
                    record_passthrough_failure(
                        &log_svc,
                        &token_info,
                        &channel,
                        endpoint,
                        request_format,
                        requested_model.as_deref().unwrap_or_default(),
                        actual_model.as_deref().unwrap_or_default(),
                        model_config
                            .as_ref()
                            .map(|config| config.model_name.as_str())
                            .unwrap_or_else(|| requested_model.as_deref().unwrap_or_default()),
                        &request_id,
                        &extract_upstream_request_id(&response_headers),
                        elapsed,
                        is_stream,
                        &client_ip,
                        &user_agent,
                        status_code,
                        failure.message.clone(),
                    );
                    let _ = rate_limiter.finalize_failure_with_retry(&request_id).await;
                    return Err(failure.error);
                }
            }
            Err(error) => {
                let elapsed = start.elapsed().as_millis() as i64;
                let _ = billing
                    .refund_with_retry(&request_id, token_info.token_id, pre_consumed)
                    .await;
                channel_svc.record_relay_failure_async(
                    channel.channel_id,
                    channel.account_id,
                    elapsed,
                    0,
                    error.to_string(),
                );
                route_state.exclude_selected_account(&channel);
                if attempt == max_retries - 1 {
                    record_passthrough_failure(
                        &log_svc,
                        &token_info,
                        &channel,
                        endpoint,
                        request_format,
                        requested_model.as_deref().unwrap_or_default(),
                        actual_model.as_deref().unwrap_or_default(),
                        model_config
                            .as_ref()
                            .map(|config| config.model_name.as_str())
                            .unwrap_or_else(|| requested_model.as_deref().unwrap_or_default()),
                        &request_id,
                        "",
                        elapsed,
                        is_stream,
                        &client_ip,
                        &user_agent,
                        0,
                        error.to_string(),
                    );
                    let _ = rate_limiter.finalize_failure_with_retry(&request_id).await;
                    return Err(OpenAiErrorResponse::internal_with(
                        "failed to send upstream request",
                        error,
                    ));
                }
            }
        }
    }

    let _ = rate_limiter.finalize_failure_with_retry(&request_id).await;
    Err(OpenAiErrorResponse::no_available_channel(
        "all channels failed",
    ))
}
