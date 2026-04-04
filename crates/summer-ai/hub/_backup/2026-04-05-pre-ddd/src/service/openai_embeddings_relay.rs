use summer::plugin::Service;
use summer_web::axum::http::HeaderMap;
use summer_web::axum::response::{IntoResponse, Response};

use summer_ai_core::provider::get_adapter;
use summer_ai_core::types::embedding::{EmbeddingRequest, EmbeddingResponse};
use summer_ai_core::types::error::{OpenAiApiResult, OpenAiErrorResponse};
use summer_ai_model::entity::request::RequestStatus;
use summer_ai_model::entity::request_execution::ExecutionStatus;
use summer_common::response::Json;
use summer_common::user_agent::UserAgentInfo;

use crate::relay::billing::BillingEngine;
use crate::relay::channel_router::{ChannelRouter, RouteSelectionExclusions};
use crate::relay::http_client::UpstreamHttpClient;
use crate::relay::rate_limit::RateLimitEngine;
use crate::router::openai::{
    apply_upstream_failure_scope, classify_upstream_provider_failure, spawn_usage_accounting_task,
};
use crate::router::openai_passthrough::unusable_success_response_message;
use crate::service::channel::ChannelService;
use crate::service::log::LogService;
use crate::service::openai_http::{
    extract_request_id, extract_upstream_request_id, fallback_usage, insert_request_id_header,
    insert_upstream_request_id_header,
};
use crate::service::openai_tracking::{
    FailureTrackingUpdate, RequestTrackingIds, map_adapter_build_error, record_terminal_failure,
    update_request_failure_tracking, update_request_success_tracking,
};
use crate::service::request::{
    ExecutionSnapshotInput, ExecutionStatusUpdate, RequestService, RequestSnapshotInput,
    RequestStatusUpdate, build_execution_active_model, build_request_active_model,
    snapshot_response_body_bytes,
};
use crate::service::runtime_ops::RuntimeOpsService;
use crate::service::token::TokenService;

#[derive(Clone, Service)]
pub struct OpenAiEmbeddingsRelayService {
    #[inject(component)]
    router_svc: ChannelRouter,
    #[inject(component)]
    billing: BillingEngine,
    #[inject(component)]
    rate_limiter: RateLimitEngine,
    #[inject(component)]
    http_client: UpstreamHttpClient,
    #[inject(component)]
    log_svc: LogService,
    #[inject(component)]
    channel_svc: ChannelService,
    #[inject(component)]
    token_svc: TokenService,
    #[inject(component)]
    runtime_ops: RuntimeOpsService,
    #[inject(component)]
    request_svc: RequestService,
}

impl OpenAiEmbeddingsRelayService {
    pub async fn relay(
        &self,
        token_info: crate::service::token::TokenInfo,
        client_ip: std::net::IpAddr,
        headers: HeaderMap,
        req: EmbeddingRequest,
    ) -> OpenAiApiResult<Response> {
        relay_impl(
            token_info,
            self.router_svc.clone(),
            self.billing.clone(),
            self.rate_limiter.clone(),
            self.http_client.clone(),
            self.log_svc.clone(),
            self.channel_svc.clone(),
            self.token_svc.clone(),
            self.runtime_ops.clone(),
            self.request_svc.clone(),
            client_ip,
            headers,
            req,
        )
        .await
    }
}

#[allow(clippy::too_many_arguments)]
async fn relay_impl(
    token_info: crate::service::token::TokenInfo,
    router_svc: ChannelRouter,
    billing: BillingEngine,
    rate_limiter: RateLimitEngine,
    http_client: UpstreamHttpClient,
    log_svc: LogService,
    channel_svc: ChannelService,
    token_svc: TokenService,
    runtime_ops: RuntimeOpsService,
    request_svc: RequestService,
    client_ip: std::net::IpAddr,
    headers: HeaderMap,
    req: EmbeddingRequest,
) -> OpenAiApiResult<Response> {
    let request_id = extract_request_id(&headers);
    let user_agent = UserAgentInfo::from_headers(&headers).raw;
    token_info
        .ensure_endpoint_allowed("embeddings")
        .map_err(|e| OpenAiErrorResponse::from_api_error(&e))?;
    token_info
        .ensure_model_allowed(&req.model)
        .map_err(|e| OpenAiErrorResponse::from_api_error(&e))?;
    let client_ip = client_ip.to_string();
    token_svc.update_last_used_ip_async(token_info.token_id, client_ip.clone());

    let model_config = billing
        .get_model_config_for_endpoint(&req.model, "embeddings")
        .await
        .map_err(|e| OpenAiErrorResponse::from_api_error(&e))?;
    let group_ratio = billing
        .get_group_ratio(&token_info.group)
        .await
        .map_err(|e| OpenAiErrorResponse::internal_with("failed to load group pricing", e))?;

    let route_exclusions = RouteSelectionExclusions::default();
    let mut route_plan = router_svc
        .build_channel_plan_with_exclusions(
            &token_info.group,
            &req.model,
            "embeddings",
            &route_exclusions,
        )
        .await
        .map_err(|e| OpenAiErrorResponse::internal_with("failed to build channel plan", e))?;
    let max_retries = 3;
    let start = std::time::Instant::now();
    let requested_model = req.model.clone();
    let estimated_tokens = summer_ai_core::types::embedding::estimate_input_tokens(&req.input);
    let raw_request = serde_json::to_value(&req).map_err(|e| {
        OpenAiErrorResponse::internal_with("failed to serialize embeddings request", e)
    })?;
    let request_record_id = match request_svc
        .create_request(build_request_active_model(RequestSnapshotInput {
            request_id: &request_id,
            token_info: &token_info,
            endpoint: "embeddings",
            request_format: "openai/embeddings",
            requested_model: &requested_model,
            is_stream: false,
            client_ip: &client_ip,
            user_agent: &user_agent,
            headers: &headers,
            request_body: raw_request.clone(),
        }))
        .await
    {
        Ok(record) => Some(record.id),
        Err(error) => {
            tracing::warn!(error = %error, request_id = %request_id, "failed to persist AI request snapshot");
            None
        }
    };

    if let Err(error) = rate_limiter
        .reserve(&token_info, &request_id, i64::from(estimated_tokens))
        .await
    {
        request_svc
            .try_update_request_status(
                request_record_id,
                RequestStatusUpdate {
                    status: RequestStatus::Failed,
                    error_message: Some(error.to_string()),
                    duration_ms: Some(start.elapsed().as_millis() as i32),
                    first_token_ms: Some(0),
                    response_status_code: Some(0),
                    response_body: None,
                    upstream_model: None,
                },
            )
            .await;
        return Err(OpenAiErrorResponse::from_quota_error(&error));
    }

    for attempt in 0..max_retries {
        if attempt > 0 {
            runtime_ops.record_fallback_async();
        }
        let Some(channel) = route_plan.next() else {
            if let Err(e) = rate_limiter.finalize_failure_with_retry(&request_id).await {
                tracing::warn!(error = %e, "failed to finalize rate limit failure (no channel)");
            }
            request_svc
                .try_update_request_status(
                    request_record_id,
                    RequestStatusUpdate {
                        status: RequestStatus::Failed,
                        error_message: Some("no available channel".to_string()),
                        duration_ms: Some(start.elapsed().as_millis() as i32),
                        first_token_ms: Some(0),
                        response_status_code: Some(0),
                        response_body: None,
                        upstream_model: None,
                    },
                )
                .await;
            return Err(OpenAiErrorResponse::no_available_channel(
                "no available channel",
            ));
        };

        let actual_model = channel
            .model_mapping
            .get(&req.model)
            .and_then(|value| value.as_str())
            .unwrap_or(&req.model)
            .to_string();

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
                if let Err(e) = rate_limiter.finalize_failure_with_retry(&request_id).await {
                    tracing::warn!(error = %e, "failed to finalize rate limit failure (quota error)");
                }
                request_svc
                    .try_update_request_status(
                        request_record_id,
                        RequestStatusUpdate {
                            status: RequestStatus::Failed,
                            error_message: Some(error.to_string()),
                            duration_ms: Some(start.elapsed().as_millis() as i32),
                            first_token_ms: Some(0),
                            response_status_code: Some(0),
                            response_body: None,
                            upstream_model: Some(actual_model.clone()),
                        },
                    )
                    .await;
                return Err(OpenAiErrorResponse::from_quota_error(&error));
            }
        };

        let adapter = get_adapter(channel.channel_type);
        let request_builder = match adapter.build_embeddings_request(
            http_client.client(),
            &channel.base_url,
            &channel.api_key,
            &raw_request,
            &actual_model,
        ) {
            Ok(builder) => builder,
            Err(error) => {
                if let Err(e) = billing
                    .refund_with_retry(&request_id, token_info.token_id, pre_consumed)
                    .await
                {
                    tracing::warn!(error = %e, "failed to refund quota");
                }
                channel_svc.record_relay_failure_async(
                    channel.channel_id,
                    channel.account_id,
                    start.elapsed().as_millis() as i64,
                    0,
                    format!("failed to build upstream embeddings request: {error}"),
                );
                route_plan.exclude_selected_channel(&channel);
                if attempt == max_retries - 1 {
                    let message = format!("failed to build upstream embeddings request: {error}");
                    record_terminal_failure(
                        &log_svc,
                        &token_info,
                        &channel,
                        "embeddings",
                        "openai/embeddings",
                        &requested_model,
                        &actual_model,
                        &model_config.model_name,
                        &request_id,
                        "",
                        start.elapsed().as_millis() as i64,
                        false,
                        &client_ip,
                        &user_agent,
                        0,
                        message.clone(),
                    );
                    update_request_failure_tracking(
                        &request_svc,
                        RequestTrackingIds {
                            request_id: request_record_id,
                            execution_id: None,
                        },
                        FailureTrackingUpdate {
                            status_code: 0,
                            message,
                            elapsed_ms: start.elapsed().as_millis() as i64,
                            upstream_model: Some(actual_model.clone()),
                            upstream_request_id: None,
                            response_body: None,
                        },
                    )
                    .await;
                    if let Err(e) = rate_limiter.finalize_failure_with_retry(&request_id).await {
                        tracing::warn!(error = %e, "failed to finalize rate limit failure");
                    }
                    return Err(map_adapter_build_error(
                        "failed to build upstream embeddings request",
                        error,
                    ));
                }
                continue;
            }
        };
        let mut tracking = RequestTrackingIds {
            request_id: request_record_id,
            execution_id: None,
        };
        let attempt_started_at = chrono::Utc::now().fixed_offset();
        let upstream_request = match request_builder.build() {
            Ok(request) => request,
            Err(error) => {
                if let Err(e) = billing
                    .refund_with_retry(&request_id, token_info.token_id, pre_consumed)
                    .await
                {
                    tracing::warn!(
                        error = %e,
                        "failed to refund quota (build request object error)"
                    );
                }
                channel_svc.record_relay_failure_async(
                    channel.channel_id,
                    channel.account_id,
                    start.elapsed().as_millis() as i64,
                    0,
                    format!("failed to materialize upstream embeddings request: {error}"),
                );
                route_plan.exclude_selected_channel(&channel);
                if attempt == max_retries - 1 {
                    let message =
                        format!("failed to materialize upstream embeddings request: {error}");
                    record_terminal_failure(
                        &log_svc,
                        &token_info,
                        &channel,
                        "embeddings",
                        "openai/embeddings",
                        &requested_model,
                        &actual_model,
                        &model_config.model_name,
                        &request_id,
                        "",
                        start.elapsed().as_millis() as i64,
                        false,
                        &client_ip,
                        &user_agent,
                        0,
                        message.clone(),
                    );
                    update_request_failure_tracking(
                        &request_svc,
                        tracking,
                        FailureTrackingUpdate {
                            status_code: 0,
                            message,
                            elapsed_ms: start.elapsed().as_millis() as i64,
                            upstream_model: Some(actual_model.clone()),
                            upstream_request_id: None,
                            response_body: None,
                        },
                    )
                    .await;
                    if let Err(e) = rate_limiter.finalize_failure_with_retry(&request_id).await {
                        tracing::warn!(error = %e, "failed to finalize rate limit failure");
                    }
                    return Err(OpenAiErrorResponse::internal_with(
                        "failed to materialize upstream embeddings request",
                        error,
                    ));
                }
                continue;
            }
        };

        if let Some(ai_request_id) = request_record_id {
            match request_svc
                .record_execution(build_execution_active_model(ExecutionSnapshotInput {
                    ai_request_id,
                    request_id: &request_id,
                    attempt_no: attempt + 1,
                    channel: &channel,
                    endpoint: "embeddings",
                    request_format: "openai/embeddings",
                    requested_model: &requested_model,
                    upstream_model: &actual_model,
                    request: &upstream_request,
                    started_at: attempt_started_at,
                }))
                .await
            {
                Ok(record) => tracking.execution_id = Some(record.id),
                Err(error) => {
                    tracing::warn!(
                        error = %error,
                        request_id = %request_id,
                        attempt = attempt + 1,
                        "failed to persist AI request execution snapshot"
                    );
                }
            }
        }

        match http_client.client().execute(upstream_request).await {
            Ok(resp) if resp.status().is_success() => {
                let status = resp.status();
                let elapsed = start.elapsed().as_millis() as i64;
                let upstream_request_id = extract_upstream_request_id(resp.headers());
                let body = match resp.bytes().await {
                    Ok(body) => body,
                    Err(error) => {
                        if let Err(e) = billing
                            .refund_with_retry(&request_id, token_info.token_id, pre_consumed)
                            .await
                        {
                            tracing::warn!(error = %e, "failed to refund quota");
                        }
                        channel_svc.record_relay_failure_async(
                            channel.channel_id,
                            channel.account_id,
                            elapsed,
                            0,
                            format!("failed to read upstream embeddings response: {error}"),
                        );
                        route_plan.exclude_selected_account(&channel);
                        request_svc
                            .try_update_execution_status(
                                tracking.execution_id,
                                ExecutionStatusUpdate {
                                    status: ExecutionStatus::Failed,
                                    error_message: Some(format!(
                                        "failed to read upstream embeddings response: {error}"
                                    )),
                                    duration_ms: Some(elapsed as i32),
                                    first_token_ms: Some(0),
                                    response_status_code: Some(0),
                                    response_body: None,
                                    upstream_request_id: Some(upstream_request_id.clone()),
                                },
                            )
                            .await;
                        if attempt == max_retries - 1 {
                            let message =
                                format!("failed to read upstream embeddings response: {error}");
                            record_terminal_failure(
                                &log_svc,
                                &token_info,
                                &channel,
                                "embeddings",
                                "openai/embeddings",
                                &requested_model,
                                &actual_model,
                                &model_config.model_name,
                                &request_id,
                                &upstream_request_id,
                                elapsed,
                                false,
                                &client_ip,
                                &user_agent,
                                0,
                                message.clone(),
                            );
                            update_request_failure_tracking(
                                &request_svc,
                                tracking,
                                FailureTrackingUpdate {
                                    status_code: 0,
                                    message,
                                    elapsed_ms: elapsed,
                                    upstream_model: Some(actual_model.clone()),
                                    upstream_request_id: Some(upstream_request_id.clone()),
                                    response_body: None,
                                },
                            )
                            .await;
                            if let Err(e) =
                                rate_limiter.finalize_failure_with_retry(&request_id).await
                            {
                                tracing::warn!(error = %e, "failed to finalize rate limit failure");
                            }
                            return Err(OpenAiErrorResponse::internal_with(
                                "failed to read upstream embeddings response",
                                error,
                            ));
                        }
                        continue;
                    }
                };
                if let Some(message) =
                    unusable_success_response_message(status, &body, "embeddings", false)
                {
                    if let Err(e) = billing
                        .refund_with_retry(&request_id, token_info.token_id, pre_consumed)
                        .await
                    {
                        tracing::warn!(error = %e, "failed to refund quota");
                    }
                    channel_svc.record_relay_failure_async(
                        channel.channel_id,
                        channel.account_id,
                        elapsed,
                        status.as_u16() as i32,
                        message.clone(),
                    );
                    route_plan.exclude_selected_channel(&channel);
                    request_svc
                        .try_update_execution_status(
                            tracking.execution_id,
                            ExecutionStatusUpdate {
                                status: ExecutionStatus::Failed,
                                error_message: Some(message.clone()),
                                duration_ms: Some(elapsed as i32),
                                first_token_ms: Some(0),
                                response_status_code: Some(status.as_u16() as i32),
                                response_body: Some(snapshot_response_body_bytes(&body)),
                                upstream_request_id: Some(upstream_request_id.clone()),
                            },
                        )
                        .await;
                    if attempt == max_retries - 1 {
                        record_terminal_failure(
                            &log_svc,
                            &token_info,
                            &channel,
                            "embeddings",
                            "openai/embeddings",
                            &requested_model,
                            &actual_model,
                            &model_config.model_name,
                            &request_id,
                            &upstream_request_id,
                            elapsed,
                            false,
                            &client_ip,
                            &user_agent,
                            status.as_u16() as i32,
                            message.clone(),
                        );
                        update_request_failure_tracking(
                            &request_svc,
                            tracking,
                            FailureTrackingUpdate {
                                status_code: status.as_u16() as i32,
                                message: message.clone(),
                                elapsed_ms: elapsed,
                                upstream_model: Some(actual_model.clone()),
                                upstream_request_id: Some(upstream_request_id.clone()),
                                response_body: Some(snapshot_response_body_bytes(&body)),
                            },
                        )
                        .await;
                        if let Err(e) = rate_limiter.finalize_failure_with_retry(&request_id).await
                        {
                            tracing::warn!(error = %e, "failed to finalize rate limit failure");
                        }
                        return Err(OpenAiErrorResponse::unsupported_endpoint(message));
                    }
                    continue;
                }
                let parsed: EmbeddingResponse = match adapter.parse_embeddings_response(
                    body.clone(),
                    &actual_model,
                    estimated_tokens,
                ) {
                    Ok(parsed) => parsed,
                    Err(error) => {
                        let response_body = snapshot_response_body_bytes(&body);
                        if let Err(e) = billing
                            .refund_with_retry(&request_id, token_info.token_id, pre_consumed)
                            .await
                        {
                            tracing::warn!(error = %e, "failed to refund quota");
                        }
                        channel_svc.record_relay_failure_async(
                            channel.channel_id,
                            channel.account_id,
                            elapsed,
                            0,
                            format!("failed to parse upstream embeddings response: {error}"),
                        );
                        route_plan.exclude_selected_account(&channel);
                        request_svc
                            .try_update_execution_status(
                                tracking.execution_id,
                                ExecutionStatusUpdate {
                                    status: ExecutionStatus::Failed,
                                    error_message: Some(format!(
                                        "failed to parse upstream embeddings response: {error}"
                                    )),
                                    duration_ms: Some(elapsed as i32),
                                    first_token_ms: Some(0),
                                    response_status_code: Some(0),
                                    response_body: Some(response_body.clone()),
                                    upstream_request_id: Some(upstream_request_id.clone()),
                                },
                            )
                            .await;
                        if attempt == max_retries - 1 {
                            let message =
                                format!("failed to parse upstream embeddings response: {error}");
                            record_terminal_failure(
                                &log_svc,
                                &token_info,
                                &channel,
                                "embeddings",
                                "openai/embeddings",
                                &requested_model,
                                &actual_model,
                                &model_config.model_name,
                                &request_id,
                                &upstream_request_id,
                                elapsed,
                                false,
                                &client_ip,
                                &user_agent,
                                0,
                                message.clone(),
                            );
                            update_request_failure_tracking(
                                &request_svc,
                                tracking,
                                FailureTrackingUpdate {
                                    status_code: 0,
                                    message,
                                    elapsed_ms: elapsed,
                                    upstream_model: Some(actual_model.clone()),
                                    upstream_request_id: Some(upstream_request_id.clone()),
                                    response_body: Some(response_body),
                                },
                            )
                            .await;
                            if let Err(e) =
                                rate_limiter.finalize_failure_with_retry(&request_id).await
                            {
                                tracing::warn!(error = %e, "failed to finalize rate limit failure");
                            }
                            return Err(OpenAiErrorResponse::internal_with(
                                "failed to parse upstream embeddings response",
                                error,
                            ));
                        }
                        continue;
                    }
                };

                let usage = if parsed.usage.total_tokens > 0 {
                    parsed.usage.clone()
                } else {
                    fallback_usage(estimated_tokens)
                };
                update_request_success_tracking(
                    &request_svc,
                    tracking,
                    elapsed,
                    0,
                    actual_model.clone(),
                    upstream_request_id.clone(),
                    Some(serde_json::to_value(&parsed).unwrap_or(serde_json::Value::Null)),
                )
                .await;
                spawn_usage_accounting_task(
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
                    request_id.clone(),
                    upstream_request_id.clone(),
                    requested_model,
                    actual_model,
                    client_ip,
                    user_agent,
                    "embeddings",
                    "openai/embeddings",
                    elapsed,
                    0,
                    false,
                );

                let mut response = Json(parsed).into_response();
                insert_request_id_header(&mut response, &request_id);
                insert_upstream_request_id_header(&mut response, &upstream_request_id);
                return Ok(response);
            }
            Ok(resp) => {
                let elapsed = start.elapsed().as_millis() as i64;
                let status_code = resp.status().as_u16() as i32;
                let status = resp.status();
                let headers = resp.headers().clone();
                let body = resp.bytes().await.unwrap_or_default();
                if let Err(e) = billing
                    .refund_with_retry(&request_id, token_info.token_id, pre_consumed)
                    .await
                {
                    tracing::warn!(error = %e, "failed to refund quota");
                }
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
                let upstream_request_id = extract_upstream_request_id(&headers);
                request_svc
                    .try_update_execution_status(
                        tracking.execution_id,
                        ExecutionStatusUpdate {
                            status: ExecutionStatus::Failed,
                            error_message: Some(failure.message.clone()),
                            duration_ms: Some(elapsed as i32),
                            first_token_ms: Some(0),
                            response_status_code: Some(status_code),
                            response_body: Some(snapshot_response_body_bytes(&body)),
                            upstream_request_id: Some(upstream_request_id.clone()),
                        },
                    )
                    .await;
                if attempt == max_retries - 1 {
                    record_terminal_failure(
                        &log_svc,
                        &token_info,
                        &channel,
                        "embeddings",
                        "openai/embeddings",
                        &requested_model,
                        &actual_model,
                        &model_config.model_name,
                        &request_id,
                        &upstream_request_id,
                        elapsed,
                        false,
                        &client_ip,
                        &user_agent,
                        status_code,
                        failure.message.clone(),
                    );
                    update_request_failure_tracking(
                        &request_svc,
                        tracking,
                        FailureTrackingUpdate {
                            status_code,
                            message: failure.message.clone(),
                            elapsed_ms: elapsed,
                            upstream_model: Some(actual_model.clone()),
                            upstream_request_id: Some(upstream_request_id),
                            response_body: Some(snapshot_response_body_bytes(&body)),
                        },
                    )
                    .await;
                    if let Err(e) = rate_limiter.finalize_failure_with_retry(&request_id).await {
                        tracing::warn!(error = %e, "failed to finalize rate limit failure");
                    }
                    return Err(failure.error);
                }
            }
            Err(error) => {
                let elapsed = start.elapsed().as_millis() as i64;
                if let Err(e) = billing
                    .refund_with_retry(&request_id, token_info.token_id, pre_consumed)
                    .await
                {
                    tracing::warn!(error = %e, "failed to refund quota");
                }
                channel_svc.record_relay_failure_async(
                    channel.channel_id,
                    channel.account_id,
                    elapsed,
                    0,
                    error.to_string(),
                );
                route_plan.exclude_selected_account(&channel);
                request_svc
                    .try_update_execution_status(
                        tracking.execution_id,
                        ExecutionStatusUpdate {
                            status: ExecutionStatus::Failed,
                            error_message: Some(error.to_string()),
                            duration_ms: Some(elapsed as i32),
                            first_token_ms: Some(0),
                            response_status_code: Some(0),
                            response_body: None,
                            upstream_request_id: None,
                        },
                    )
                    .await;
                if attempt == max_retries - 1 {
                    let message = error.to_string();
                    record_terminal_failure(
                        &log_svc,
                        &token_info,
                        &channel,
                        "embeddings",
                        "openai/embeddings",
                        &requested_model,
                        &actual_model,
                        &model_config.model_name,
                        &request_id,
                        "",
                        elapsed,
                        false,
                        &client_ip,
                        &user_agent,
                        0,
                        message.clone(),
                    );
                    update_request_failure_tracking(
                        &request_svc,
                        tracking,
                        FailureTrackingUpdate {
                            status_code: 0,
                            message,
                            elapsed_ms: elapsed,
                            upstream_model: Some(actual_model.clone()),
                            upstream_request_id: None,
                            response_body: None,
                        },
                    )
                    .await;
                    if let Err(e) = rate_limiter.finalize_failure_with_retry(&request_id).await {
                        tracing::warn!(error = %e, "failed to finalize rate limit failure");
                    }
                    return Err(OpenAiErrorResponse::internal_with(
                        "failed to send upstream embeddings request",
                        error,
                    ));
                }
            }
        }
    }

    if let Err(e) = rate_limiter.finalize_failure_with_retry(&request_id).await {
        tracing::warn!(error = %e, "failed to finalize rate limit failure");
    }
    request_svc
        .try_update_request_status(
            request_record_id,
            RequestStatusUpdate {
                status: RequestStatus::Failed,
                error_message: Some("all channels failed".to_string()),
                duration_ms: Some(start.elapsed().as_millis() as i32),
                first_token_ms: Some(0),
                response_status_code: Some(0),
                response_body: None,
                upstream_model: None,
            },
        )
        .await;
    Err(OpenAiErrorResponse::no_available_channel(
        "all channels failed",
    ))
}
