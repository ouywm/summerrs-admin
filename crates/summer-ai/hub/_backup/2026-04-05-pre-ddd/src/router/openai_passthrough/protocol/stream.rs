use super::*;

pub(crate) async fn relay_resource_multipart_post(
    token_info: TokenInfo,
    router_svc: ChannelRouter,
    http_client: UpstreamHttpClient,
    channel_svc: ChannelService,
    token_svc: TokenService,
    resource_affinity: ResourceAffinityService,
    client_ip: String,
    headers: HeaderMap,
    multipart: summer_web::axum::extract::Multipart,
    upstream_path: String,
    spec: ResourceRequestSpec,
    affinity_keys: Vec<(&'static str, String)>,
    delete_affinity: Option<(&'static str, String)>,
) -> OpenAiApiResult<Response> {
    let payload = parse_multipart_payload(multipart).await?;
    if let Some(model) = payload.model.as_ref() {
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
        None,
        Some(payload),
        delete_affinity,
    )
    .await
}

pub(crate) async fn relay_resource_request(
    token_info: TokenInfo,
    router_svc: ChannelRouter,
    http_client: UpstreamHttpClient,
    channel_svc: ChannelService,
    token_svc: TokenService,
    resource_affinity: ResourceAffinityService,
    client_ip: String,
    headers: HeaderMap,
    query: Option<String>,
    method: Method,
    upstream_path: String,
    spec: ResourceRequestSpec,
    affinity_keys: Vec<(&'static str, String)>,
    mut json_body: Option<&mut Value>,
    multipart_body: Option<ParsedMultipartPayload>,
    delete_affinity: Option<(&'static str, String)>,
) -> OpenAiApiResult<Response> {
    let request_id = extract_request_id(&headers);
    let mut last_unsupported_message: Option<String> = None;
    let requested_model = json_body
        .as_deref()
        .and_then(|body| model_from_json_body(body, None));
    let is_stream = json_body.as_deref().is_some_and(json_body_requests_stream);
    token_info
        .ensure_endpoint_allowed(spec.endpoint_scope)
        .map_err(|error| OpenAiErrorResponse::from_api_error(&error))?;
    token_svc.update_last_used_ip_async(token_info.token_id, client_ip);

    let request_method = reqwest::Method::from_bytes(method.as_str().as_bytes())
        .map_err(|error| OpenAiErrorResponse::internal_with("invalid request method", error))?;
    let max_retries = 3;
    let start = std::time::Instant::now();
    let requested_model_from_multipart = multipart_body
        .as_ref()
        .and_then(|payload| payload.model.as_deref());
    let mut route_state = ResourceRouteState::new(
        &token_info,
        &router_svc,
        spec.endpoint_scope,
        requested_model
            .as_deref()
            .or(requested_model_from_multipart),
    )
    .await?;

    for attempt in 0..max_retries {
        let Some(channel) = route_state
            .select(
                &token_info,
                &resource_affinity,
                &affinity_keys,
                json_body.as_deref(),
            )
            .await?
        else {
            return Err(match last_unsupported_message.clone() {
                Some(message) => OpenAiErrorResponse::unsupported_endpoint(message),
                None => OpenAiErrorResponse::no_available_channel(if attempt == 0 {
                    "no available channel"
                } else {
                    "all channels failed"
                }),
            });
        };

        if !resource_endpoint_supported(channel.channel_type) {
            let message = format!("{upstream_path} endpoint is not supported for this provider");
            channel_svc.record_relay_failure_async(
                channel.channel_id,
                channel.account_id,
                start.elapsed().as_millis() as i64,
                0,
                message.clone(),
            );
            route_state.exclude_selected_channel(&channel);
            last_unsupported_message = Some(message);
            continue;
        };

        let mut request_builder = http_client.client().request(
            request_method.clone(),
            build_upstream_url(&channel.base_url, &upstream_path, query.as_deref()),
        );
        request_builder =
            apply_upstream_auth(request_builder, channel.channel_type, &channel.api_key);

        if let Some(body) = json_body.as_deref_mut() {
            if let Some(model) = requested_model.as_deref() {
                ensure_json_model(body, &mapped_model(&channel, model))?;
            }
            request_builder = request_builder.json(body);
        } else if let Some(payload) = multipart_body.as_ref() {
            let actual_model = payload
                .model
                .as_ref()
                .map(|model| mapped_model(&channel, model));
            request_builder =
                request_builder.multipart(payload.to_form(actual_model.as_deref().unwrap_or(""))?);
        }
        request_builder = apply_forward_headers(request_builder, &headers, false);

        let response = match request_builder.send().await {
            Ok(response) => response,
            Err(error) => {
                channel_svc.record_relay_failure_async(
                    channel.channel_id,
                    channel.account_id,
                    start.elapsed().as_millis() as i64,
                    0,
                    error.to_string(),
                );
                route_state.exclude_selected_account(&channel);
                if attempt == max_retries - 1 {
                    return Err(OpenAiErrorResponse::internal_with(
                        "failed to send upstream request",
                        error,
                    ));
                }
                continue;
            }
        };

        let status = response.status();
        let response_headers = response.headers().clone();
        let upstream_request_id = extract_upstream_request_id(&response_headers);
        let content_type = response_headers.get(CONTENT_TYPE).cloned();
        if status.is_success() && is_stream {
            return Ok(build_resource_passthrough_stream_response(
                response,
                token_info,
                channel,
                request_id,
                upstream_request_id,
                start.elapsed().as_millis() as i64,
                channel_svc,
                resource_affinity,
                spec.bind_resource_kind,
            ));
        }
        let upstream_body = match response.bytes().await {
            Ok(body) => body,
            Err(error) => {
                channel_svc.record_relay_failure_async(
                    channel.channel_id,
                    channel.account_id,
                    start.elapsed().as_millis() as i64,
                    0,
                    format!("failed to read upstream response: {error}"),
                );
                route_state.exclude_selected_account(&channel);
                if attempt == max_retries - 1 {
                    return Err(OpenAiErrorResponse::internal_with(
                        "failed to read upstream response",
                        error,
                    ));
                }
                continue;
            }
        };

        if status.is_success() {
            if let Some(message) = unusable_success_response_message(
                status,
                &upstream_body,
                &upstream_path,
                allow_empty_success_body_for_upstream_path(&upstream_path),
            ) {
                channel_svc.record_relay_failure_async(
                    channel.channel_id,
                    channel.account_id,
                    start.elapsed().as_millis() as i64,
                    status.as_u16() as i32,
                    message.clone(),
                );
                route_state.exclude_selected_channel(&channel);
                if attempt == max_retries - 1 {
                    return Err(OpenAiErrorResponse::unsupported_endpoint(message));
                }
                continue;
            }

            if let Ok(value) = serde_json::from_slice::<Value>(&upstream_body) {
                bind_resource_affinities(
                    &token_info,
                    &resource_affinity,
                    &channel,
                    spec.bind_resource_kind,
                    &value,
                )
                .await;
            }

            if let Some((kind, id)) = delete_affinity.as_ref()
                && let Err(error) = resource_affinity.delete(&token_info, kind, id).await
            {
                tracing::warn!("failed to delete resource affinity: {error}");
            }

            if let Err(error) = channel_svc
                .record_relay_success(
                    channel.channel_id,
                    channel.account_id,
                    start.elapsed().as_millis() as i64,
                )
                .await
            {
                tracing::warn!("failed to update relay success health state: {error}");
            }

            let mut response =
                build_bytes_response(status, upstream_body, content_type, &request_id);
            insert_upstream_request_id_header(&mut response, &upstream_request_id);
            return Ok(response);
        }

        let failure = classify_upstream_provider_failure(
            channel.channel_type,
            status,
            &response_headers,
            &upstream_body,
        );
        channel_svc.record_relay_failure_async(
            channel.channel_id,
            channel.account_id,
            start.elapsed().as_millis() as i64,
            status.as_u16() as i32,
            failure.message.clone(),
        );
        apply_upstream_failure_scope(&mut route_state, &channel, failure.scope);
        if attempt == max_retries - 1 {
            return Err(failure.error);
        }
    }

    Err(match last_unsupported_message {
        Some(message) => OpenAiErrorResponse::unsupported_endpoint(message),
        None => OpenAiErrorResponse::no_available_channel("all channels failed"),
    })
}

pub(crate) fn resource_endpoint_supported(channel_type: i16) -> bool {
    matches!(
        channel_type,
        value if value == ChannelType::OpenAi as i16 || value == ChannelType::Azure as i16
    )
}

pub(crate) fn build_generic_stream_response(
    upstream: reqwest::Response,
    token_info: TokenInfo,
    pre_consumed: i64,
    model_config: Option<ModelConfigInfo>,
    group_ratio: f64,
    channel: SelectedChannel,
    requested_model: Option<String>,
    estimated_prompt_tokens: i32,
    endpoint: &'static str,
    request_format: &'static str,
    start_elapsed: i64,
    client_ip: String,
    log_svc: LogService,
    channel_svc: ChannelService,
    rate_limiter: RateLimitEngine,
    billing: BillingEngine,
    request_id: String,
    upstream_request_id: String,
    user_agent: String,
    resource_affinity: ResourceAffinityService,
    bind_resource_kind: Option<&'static str>,
    endpoint_scope: &'static str,
) -> Response {
    let status = upstream.status();
    let content_type = upstream.headers().get(CONTENT_TYPE).cloned();
    let response_request_id = request_id.clone();
    let response_upstream_request_id = upstream_request_id.clone();

    let stream = async_stream::stream! {
        let start = std::time::Instant::now();
        let mut tracker = GenericStreamTracker::default();
        let mut first_token_time = None;
        let mut stream_error = None;
        let mut byte_stream = upstream.bytes_stream();

        while let Some(result) = byte_stream.next().await {
            match result {
                Ok(chunk) => {
                    tracker.ingest(&chunk, &start, &mut first_token_time);
                    yield Ok::<Bytes, Infallible>(chunk);
                }
                Err(error) => {
                    tracing::error!("generic stream read error: {error}");
                    stream_error = Some(error.to_string());
                    break;
                }
            }
        }

        let total_elapsed = start_elapsed + start.elapsed().as_millis() as i64;
        if let Some(resource_kind) = bind_resource_kind
            && !tracker.resource_id.is_empty()
        {
            bind_resource_affinity(
                &token_info,
                &resource_affinity,
                &channel,
                resource_kind,
                &tracker.resource_id,
            )
            .await;
        }
        bind_resource_affinity_refs(
            &token_info,
            &resource_affinity,
            &channel,
            &tracker.resource_refs,
        )
        .await;

        if let Some(usage) = tracker.usage {
            let upstream_model = if tracker.upstream_model.is_empty() {
                requested_model.clone().unwrap_or_default()
            } else {
                tracker.upstream_model
            };

            settle_resource_usage_accounting(
                billing,
                rate_limiter,
                log_svc,
                channel_svc,
                token_info,
                channel,
                model_config,
                    group_ratio,
                    pre_consumed,
                    usage,
                    request_id,
                    upstream_request_id,
                    requested_model,
                upstream_model,
                client_ip,
                user_agent,
                endpoint,
                request_format,
                total_elapsed,
                first_token_time.unwrap_or(0) as i32,
                true,
                endpoint_scope,
            )
            .await;
        } else {
            if let Err(error) = billing
                .refund_with_retry(&request_id, token_info.token_id, pre_consumed)
                .await
            {
                tracing::warn!("failed to refund resource stream reservation: {error}");
            }
            if let Err(error) = rate_limiter.finalize_failure_with_retry(&request_id).await {
                tracing::warn!("failed to finalize rate limit failure: {error}");
            }
            let failure_message = stream_error.unwrap_or_else(|| {
                format!("stream ended without usage; estimated_prompt_tokens={estimated_prompt_tokens}")
            });
            if let Err(error) = channel_svc
                .record_relay_failure(
                    channel.channel_id,
                    channel.account_id,
                    total_elapsed,
                    0,
                    &failure_message,
                )
                .await
            {
                tracing::warn!("failed to update relay failure health state: {error}");
            }
        }
    };

    let mut response = Response::builder()
        .status(status)
        .body(Body::from_stream(stream))
        .unwrap();
    response.headers_mut().insert(
        CONTENT_TYPE,
        content_type.unwrap_or_else(|| HeaderValue::from_static("text/event-stream")),
    );
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    insert_request_id_header(&mut response, &response_request_id);
    insert_upstream_request_id_header(&mut response, &response_upstream_request_id);
    response
}

pub(crate) fn build_resource_passthrough_stream_response(
    upstream: reqwest::Response,
    token_info: TokenInfo,
    channel: SelectedChannel,
    request_id: String,
    upstream_request_id: String,
    start_elapsed: i64,
    channel_svc: ChannelService,
    resource_affinity: ResourceAffinityService,
    bind_resource_kind: Option<&'static str>,
) -> Response {
    let status = upstream.status();
    let content_type = upstream.headers().get(CONTENT_TYPE).cloned();
    let response_request_id = request_id;

    let stream = async_stream::stream! {
        let start = std::time::Instant::now();
        let mut tracker = GenericStreamTracker::default();
        let mut first_token_time = None;
        let mut stream_error = None;
        let mut byte_stream = upstream.bytes_stream();

        while let Some(result) = byte_stream.next().await {
            match result {
                Ok(chunk) => {
                    tracker.ingest(&chunk, &start, &mut first_token_time);
                    yield Ok::<Bytes, Infallible>(chunk);
                }
                Err(error) => {
                    tracing::error!("resource stream read error: {error}");
                    stream_error = Some(error.to_string());
                    break;
                }
            }
        }

        let total_elapsed = start_elapsed + start.elapsed().as_millis() as i64;
        if let Some(resource_kind) = bind_resource_kind
            && !tracker.resource_id.is_empty()
        {
            bind_resource_affinity(
                &token_info,
                &resource_affinity,
                &channel,
                resource_kind,
                &tracker.resource_id,
            )
            .await;
        }
        bind_resource_affinity_refs(
            &token_info,
            &resource_affinity,
            &channel,
            &tracker.resource_refs,
        )
        .await;

        if let Some(error) = stream_error {
            channel_svc.record_relay_failure_async(
                channel.channel_id,
                channel.account_id,
                total_elapsed,
                0,
                error,
            );
        } else if let Err(error) = channel_svc
            .record_relay_success(channel.channel_id, channel.account_id, total_elapsed)
            .await
        {
            tracing::warn!("failed to update relay success health state: {error}");
        }
    };

    let mut response = Response::builder()
        .status(status)
        .body(Body::from_stream(stream))
        .unwrap();
    response.headers_mut().insert(
        CONTENT_TYPE,
        content_type.unwrap_or_else(|| HeaderValue::from_static("text/event-stream")),
    );
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    insert_request_id_header(&mut response, &response_request_id);
    insert_upstream_request_id_header(&mut response, &upstream_request_id);
    response
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn settle_resource_usage_accounting(
    billing: BillingEngine,
    rate_limiter: RateLimitEngine,
    log_svc: LogService,
    channel_svc: ChannelService,
    token_info: TokenInfo,
    channel: SelectedChannel,
    model_config: Option<ModelConfigInfo>,
    group_ratio: f64,
    pre_consumed: i64,
    usage: Usage,
    request_id: String,
    upstream_request_id: String,
    requested_model: Option<String>,
    upstream_model: String,
    client_ip: String,
    user_agent: String,
    endpoint: &'static str,
    request_format: &'static str,
    elapsed: i64,
    first_token_time: i32,
    is_stream: bool,
    endpoint_scope: &'static str,
) {
    let Some(accounting_model) =
        usage_accounting_model(requested_model.as_deref(), &upstream_model)
    else {
        tracing::warn!("failed to determine usage accounting model for endpoint {endpoint}");
        if let Err(error) = rate_limiter
            .finalize_success_with_retry(&request_id, i64::from(usage.total_tokens))
            .await
        {
            tracing::warn!("failed to finalize rate limit success: {error}");
        }
        if let Err(error) = channel_svc
            .record_relay_success(channel.channel_id, channel.account_id, elapsed)
            .await
        {
            tracing::warn!("failed to update relay success health state: {error}");
        }
        return;
    };

    let model_config = match model_config {
        Some(model_config) => model_config,
        None => match billing
            .get_model_config_for_endpoint(&accounting_model, endpoint_scope)
            .await
        {
            Ok(model_config) => model_config,
            Err(error) => {
                tracing::warn!("failed to load model config for usage accounting: {error}");
                if let Err(error) = rate_limiter
                    .finalize_success_with_retry(&request_id, i64::from(usage.total_tokens))
                    .await
                {
                    tracing::warn!("failed to finalize rate limit success: {error}");
                }
                if let Err(error) = channel_svc
                    .record_relay_success(channel.channel_id, channel.account_id, elapsed)
                    .await
                {
                    tracing::warn!("failed to update relay success health state: {error}");
                }
                return;
            }
        },
    };

    let logged_quota = BillingEngine::calculate_actual_quota(&usage, &model_config, group_ratio);
    let actual_quota = match billing
        .post_consume_with_retry(
            &request_id,
            &token_info,
            pre_consumed,
            &usage,
            &model_config,
            group_ratio,
        )
        .await
    {
        Ok(quota) => quota,
        Err(error) => {
            tracing::error!("failed to settle usage asynchronously: {error}");
            logged_quota
        }
    };

    log_svc.record_usage_async(
        &token_info,
        &channel,
        &usage,
        AiUsageLogRecord {
            endpoint: endpoint.into(),
            request_format: request_format.into(),
            request_id: request_id.clone(),
            upstream_request_id,
            requested_model: requested_model.unwrap_or_else(|| accounting_model.clone()),
            upstream_model,
            model_name: model_config.model_name.clone(),
            quota: actual_quota,
            elapsed_time: elapsed as i32,
            first_token_time,
            is_stream,
            client_ip,
            user_agent,
            status_code: 200,
            content: String::new(),
            status: LogStatus::Success,
        },
    );

    if let Err(error) = rate_limiter
        .finalize_success_with_retry(&request_id, i64::from(usage.total_tokens))
        .await
    {
        tracing::warn!("failed to finalize rate limit success: {error}");
    }

    if let Err(error) = channel_svc
        .record_relay_success(channel.channel_id, channel.account_id, elapsed)
        .await
    {
        tracing::warn!("failed to update relay success health state: {error}");
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn spawn_resource_usage_accounting_task(
    billing: BillingEngine,
    rate_limiter: RateLimitEngine,
    log_svc: LogService,
    channel_svc: ChannelService,
    token_info: TokenInfo,
    channel: SelectedChannel,
    model_config: Option<ModelConfigInfo>,
    group_ratio: f64,
    pre_consumed: i64,
    usage: Usage,
    request_id: String,
    upstream_request_id: String,
    requested_model: Option<String>,
    upstream_model: String,
    client_ip: String,
    user_agent: String,
    endpoint: &'static str,
    request_format: &'static str,
    elapsed: i64,
    first_token_time: i32,
    is_stream: bool,
    endpoint_scope: &'static str,
) {
    tokio::spawn(async move {
        settle_resource_usage_accounting(
            billing,
            rate_limiter,
            log_svc,
            channel_svc,
            token_info,
            channel,
            model_config,
            group_ratio,
            pre_consumed,
            usage,
            request_id,
            upstream_request_id,
            requested_model,
            upstream_model,
            client_ip,
            user_agent,
            endpoint,
            request_format,
            elapsed,
            first_token_time,
            is_stream,
            endpoint_scope,
        )
        .await;
    });
}

pub(crate) fn usage_accounting_model(
    requested_model: Option<&str>,
    upstream_model: &str,
) -> Option<String> {
    requested_model
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            let upstream_model = upstream_model.trim();
            (!upstream_model.is_empty()).then(|| upstream_model.to_string())
        })
}

pub(crate) async fn bind_resource_affinities(
    token_info: &TokenInfo,
    resource_affinity: &ResourceAffinityService,
    channel: &SelectedChannel,
    primary_kind: Option<&'static str>,
    value: &Value,
) {
    if let Some(resource_kind) = primary_kind
        && let Some(id) = extract_generic_resource_id(value)
    {
        bind_resource_affinity(token_info, resource_affinity, channel, resource_kind, &id).await;
    }

    let refs = referenced_resource_ids(value);
    bind_resource_affinity_refs(token_info, resource_affinity, channel, &refs).await;
}

pub(crate) async fn bind_resource_affinity_refs(
    token_info: &TokenInfo,
    resource_affinity: &ResourceAffinityService,
    channel: &SelectedChannel,
    refs: &[(&'static str, String)],
) {
    for (resource_kind, resource_id) in refs {
        bind_resource_affinity(
            token_info,
            resource_affinity,
            channel,
            resource_kind,
            resource_id,
        )
        .await;
    }
}

pub(crate) async fn bind_resource_affinity(
    token_info: &TokenInfo,
    resource_affinity: &ResourceAffinityService,
    channel: &SelectedChannel,
    resource_kind: &'static str,
    resource_id: &str,
) {
    if resource_id.trim().is_empty() {
        return;
    }

    if let Err(error) = resource_affinity
        .bind(token_info, resource_kind, resource_id, channel)
        .await
    {
        tracing::warn!("failed to bind resource affinity: {error}");
    }
}

pub(crate) fn mapped_model(channel: &SelectedChannel, requested_model: &str) -> String {
    channel
        .model_mapping
        .get(requested_model)
        .and_then(Value::as_str)
        .unwrap_or(requested_model)
        .to_string()
}

pub(crate) fn model_from_json_body(body: &Value, default_model: Option<&str>) -> Option<String> {
    body.get("model")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .or_else(|| default_model.map(ToOwned::to_owned))
}

pub(crate) fn json_body_requests_stream(body: &Value) -> bool {
    body.get("stream").and_then(Value::as_bool).unwrap_or(false)
}

pub(crate) fn ensure_json_model(body: &mut Value, model: &str) -> OpenAiApiResult<()> {
    let Some(map) = body.as_object_mut() else {
        return Err(OpenAiErrorResponse::invalid_request(
            "request body must be a JSON object",
        ));
    };
    map.insert("model".into(), Value::String(model.to_string()));
    Ok(())
}

pub(crate) fn estimate_json_tokens(body: &Value) -> i32 {
    let tokens = ((body.to_string().len() as f64) / 4.0).ceil() as i32;
    tokens.max(1)
}

pub(crate) fn estimate_total_tokens_for_rate_limit(body: &Value) -> i64 {
    let output_tokens = body
        .get("max_tokens")
        .or_else(|| body.get("max_output_tokens"))
        .and_then(Value::as_i64)
        .unwrap_or(0)
        .max(0);
    i64::from(estimate_json_tokens(body)) + output_tokens.max(1)
}

pub(crate) fn extract_usage_from_value(value: &Value) -> Option<Usage> {
    let usage = value.get("usage")?;

    if usage.get("prompt_tokens").is_some() {
        return Some(Usage {
            prompt_tokens: usage.get("prompt_tokens")?.as_i64()? as i32,
            completion_tokens: usage
                .get("completion_tokens")
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32,
            total_tokens: usage.get("total_tokens")?.as_i64()? as i32,
            cached_tokens: usage
                .get("cached_tokens")
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32,
            reasoning_tokens: usage
                .get("reasoning_tokens")
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32,
        });
    }

    Some(Usage {
        prompt_tokens: usage
            .get("input_tokens")
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32,
        completion_tokens: usage
            .get("output_tokens")
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32,
        total_tokens: usage.get("total_tokens")?.as_i64()? as i32,
        cached_tokens: usage
            .get("input_tokens_details")
            .and_then(|details| details.get("cached_tokens"))
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32,
        reasoning_tokens: usage
            .get("output_tokens_details")
            .and_then(|details| details.get("reasoning_tokens"))
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32,
    })
}

pub(crate) fn extract_model_from_response_value(value: &Value) -> Option<String> {
    value
        .get("model")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .or_else(|| {
            value
                .get("response")
                .and_then(|response| response.get("model"))
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
}

pub(crate) fn payload_has_text_delta(payload: &Value) -> bool {
    payload
        .get("choices")
        .and_then(Value::as_array)
        .is_some_and(|choices| {
            choices.iter().any(|choice| {
                choice
                    .get("text")
                    .and_then(Value::as_str)
                    .is_some_and(|text| !text.is_empty())
                    || choice
                        .get("delta")
                        .and_then(|delta| delta.get("content"))
                        .and_then(Value::as_str)
                        .is_some_and(|text| !text.is_empty())
            })
        })
        || payload
            .get("type")
            .and_then(Value::as_str)
            .is_some_and(|event_type| event_type == "response.output_text.delta")
}

pub(crate) async fn parse_multipart_payload(
    mut multipart: summer_web::axum::extract::Multipart,
) -> OpenAiApiResult<ParsedMultipartPayload> {
    let mut fields = Vec::new();
    let mut model = None;

    while let Some(mut field) = multipart.next_field().await.map_err(|error| {
        OpenAiErrorResponse::internal_with("failed to read multipart field", error)
    })? {
        let Some(name) = field.name().map(ToOwned::to_owned) else {
            continue;
        };

        if let Some(file_name) = field.file_name().map(ToOwned::to_owned) {
            let content_type = field.content_type().map(ToOwned::to_owned);
            let bytes = read_multipart_field_bytes_limited(&mut field, &name).await?;
            fields.push(MultipartField::File {
                name,
                file_name,
                content_type,
                bytes,
            });
            continue;
        }

        let bytes = read_multipart_field_bytes_limited(&mut field, &name).await?;
        let value = String::from_utf8(bytes.to_vec()).map_err(|error| {
            OpenAiErrorResponse::invalid_request(format!(
                "multipart field '{name}' is not valid UTF-8: {error}"
            ))
        })?;
        if name == "model" && !value.trim().is_empty() {
            model = Some(value.clone());
        }
        fields.push(MultipartField::Text { name, value });
    }

    Ok(ParsedMultipartPayload { fields, model })
}

impl ParsedMultipartPayload {
    fn to_form(&self, actual_model: &str) -> OpenAiApiResult<Form> {
        let mut form = Form::new();
        let mut wrote_model = false;

        for field in &self.fields {
            match field {
                MultipartField::Text { name, value } => {
                    if name == "model" {
                        wrote_model = true;
                        form = form.text(name.clone(), actual_model.to_string());
                    } else {
                        form = form.text(name.clone(), value.clone());
                    }
                }
                MultipartField::File {
                    name,
                    file_name,
                    content_type,
                    bytes,
                } => {
                    let mut part = Part::bytes(bytes.clone().to_vec()).file_name(file_name.clone());
                    if let Some(content_type) = content_type {
                        part = part.mime_str(content_type).map_err(|error| {
                            OpenAiErrorResponse::internal_with(
                                "failed to build multipart file part",
                                error,
                            )
                        })?;
                    }
                    form = form.part(name.clone(), part);
                }
            }
        }

        if !wrote_model && !actual_model.is_empty() {
            form = form.text("model".to_string(), actual_model.to_string());
        }

        Ok(form)
    }
}
