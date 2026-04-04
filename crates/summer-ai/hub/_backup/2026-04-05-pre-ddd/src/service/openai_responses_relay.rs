use bytes::Bytes;
use summer::plugin::Service;
use summer_web::axum::body::Body;
use summer_web::axum::http::{HeaderMap, StatusCode};
use summer_web::axum::http::{HeaderValue, header::CONTENT_TYPE};
use summer_web::axum::response::{IntoResponse, Response};

use summer_ai_core::provider::{ResponsesRuntimeMode, get_adapter};
use summer_ai_core::types::common::Usage;
use summer_ai_core::types::error::{OpenAiApiResult, OpenAiErrorResponse};
use summer_ai_core::types::responses::{
    ResponsesRequest, ResponsesResponse, estimate_input_tokens as estimate_response_input_tokens,
    estimate_total_tokens_for_rate_limit as estimate_response_total_tokens_for_rate_limit,
};
use summer_ai_model::entity::log::LogStatus;
use summer_ai_model::entity::request::RequestStatus;
use summer_ai_model::entity::request_execution::ExecutionStatus;
use summer_common::response::Json;
use summer_common::user_agent::UserAgentInfo;

use crate::relay::billing::{BillingEngine, ModelConfigInfo};
use crate::relay::channel_router::{ChannelRouter, RouteSelectionExclusions};
use crate::relay::http_client::UpstreamHttpClient;
use crate::relay::rate_limit::RateLimitEngine;
use crate::router::openai::{apply_upstream_failure_scope, classify_upstream_provider_failure};
use crate::router::openai_passthrough::unusable_success_response_message;
use crate::service::channel::ChannelService;
use crate::service::log::{AiUsageLogRecord, LogService};
use crate::service::openai_http::{
    bridge_chat_completion_to_response, extract_request_id, extract_upstream_request_id,
    fallback_usage, insert_request_id_header, insert_upstream_request_id_header,
};
use crate::service::openai_responses_stream::{
    build_chat_bridged_responses_stream_response, build_responses_stream_response,
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
use crate::service::resource_affinity::ResourceAffinityService;
use crate::service::response_bridge::ResponseBridgeService;
use crate::service::runtime_ops::RuntimeOpsService;
use crate::service::token::TokenService;

#[derive(Clone, Service)]
pub struct OpenAiResponsesRelayService {
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
    response_bridge: ResponseBridgeService,
    #[inject(component)]
    resource_affinity: ResourceAffinityService,
    #[inject(component)]
    runtime_ops: RuntimeOpsService,
    #[inject(component)]
    request_svc: RequestService,
}

impl OpenAiResponsesRelayService {
    pub async fn relay(
        &self,
        token_info: crate::service::token::TokenInfo,
        client_ip: std::net::IpAddr,
        headers: HeaderMap,
        req: ResponsesRequest,
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
            self.response_bridge.clone(),
            self.resource_affinity.clone(),
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
pub(crate) async fn settle_usage_accounting(
    billing: BillingEngine,
    rate_limiter: RateLimitEngine,
    log_svc: LogService,
    channel_svc: ChannelService,
    token_info: crate::service::token::TokenInfo,
    channel: crate::relay::channel_router::SelectedChannel,
    model_config: ModelConfigInfo,
    group_ratio: f64,
    pre_consumed: i64,
    usage: Usage,
    request_id: String,
    upstream_request_id: String,
    requested_model: String,
    upstream_model: String,
    client_ip: String,
    user_agent: String,
    endpoint: &'static str,
    request_format: &'static str,
    elapsed: i64,
    first_token_time: i32,
    is_stream: bool,
) {
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
            requested_model,
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

    // Record metrics for successful relay.
    let metrics = crate::service::metrics::relay_metrics();
    metrics.record_request_success(elapsed as u64);
    metrics.record_tokens(
        usage.prompt_tokens,
        usage.completion_tokens,
        usage.cached_tokens,
    );
    metrics
        .quota_consumed
        .fetch_add(actual_quota, std::sync::atomic::Ordering::Relaxed);

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
pub(crate) fn spawn_usage_accounting_task(
    billing: BillingEngine,
    rate_limiter: RateLimitEngine,
    log_svc: LogService,
    channel_svc: ChannelService,
    token_info: crate::service::token::TokenInfo,
    channel: crate::relay::channel_router::SelectedChannel,
    model_config: ModelConfigInfo,
    group_ratio: f64,
    pre_consumed: i64,
    usage: Usage,
    request_id: String,
    upstream_request_id: String,
    requested_model: String,
    upstream_model: String,
    client_ip: String,
    user_agent: String,
    endpoint: &'static str,
    request_format: &'static str,
    elapsed: i64,
    first_token_time: i32,
    is_stream: bool,
) {
    tokio::spawn(async move {
        settle_usage_accounting(
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
        )
        .await;
    });
}

pub(crate) fn build_json_bytes_response(
    body: Bytes,
    content_type: Option<HeaderValue>,
    request_id: &str,
) -> Response {
    let mut response = Response::builder()
        .status(StatusCode::OK)
        .body(body.into())
        .unwrap_or_else(|_| Response::new(Body::empty()));
    response.headers_mut().insert(
        CONTENT_TYPE,
        content_type.unwrap_or_else(|| HeaderValue::from_static("application/json")),
    );
    insert_request_id_header(&mut response, request_id);
    response
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
    response_bridge: ResponseBridgeService,
    resource_affinity: ResourceAffinityService,
    runtime_ops: RuntimeOpsService,
    request_svc: RequestService,
    client_ip: std::net::IpAddr,
    headers: HeaderMap,
    req: ResponsesRequest,
) -> OpenAiApiResult<Response> {
    let request_id = extract_request_id(&headers);
    let user_agent = UserAgentInfo::from_headers(&headers).raw;
    token_info
        .ensure_endpoint_allowed("responses")
        .map_err(|e| OpenAiErrorResponse::from_api_error(&e))?;
    token_info
        .ensure_model_allowed(&req.model)
        .map_err(|e| OpenAiErrorResponse::from_api_error(&e))?;
    let client_ip = client_ip.to_string();
    token_svc.update_last_used_ip_async(token_info.token_id, client_ip.clone());

    let model_config = billing
        .get_model_config_for_endpoint(&req.model, "responses")
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
            "responses",
            &route_exclusions,
        )
        .await
        .map_err(|e| OpenAiErrorResponse::internal_with("failed to build channel plan", e))?;
    let max_retries = 3;
    let start = std::time::Instant::now();
    let is_stream = req.stream;
    let requested_model = req.model.clone();
    let input_snapshot = req.input.clone();
    let estimated_tokens = estimate_response_input_tokens(&req.input);
    let estimated_total_tokens =
        estimate_response_total_tokens_for_rate_limit(&req.input, req.max_output_tokens);
    let raw_request = serde_json::to_value(&req).map_err(|e| {
        OpenAiErrorResponse::internal_with("failed to serialize responses request", e)
    })?;
    let request_record_id = match request_svc
        .create_request(build_request_active_model(RequestSnapshotInput {
            request_id: &request_id,
            token_info: &token_info,
            endpoint: "responses",
            request_format: "openai/responses",
            requested_model: &requested_model,
            is_stream,
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
        .reserve(&token_info, &request_id, estimated_total_tokens)
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
        let responses_mode = adapter.responses_runtime_mode();
        let request_builder = match adapter.build_responses_request(
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
                    format!("failed to build upstream responses request: {error}"),
                );
                route_plan.exclude_selected_channel(&channel);
                if attempt == max_retries - 1 {
                    let message = format!("failed to build upstream responses request: {error}");
                    record_terminal_failure(
                        &log_svc,
                        &token_info,
                        &channel,
                        "responses",
                        "openai/responses",
                        &requested_model,
                        &actual_model,
                        &model_config.model_name,
                        &request_id,
                        "",
                        start.elapsed().as_millis() as i64,
                        is_stream,
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
                        "failed to build upstream responses request",
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
                    format!("failed to materialize upstream responses request: {error}"),
                );
                route_plan.exclude_selected_channel(&channel);
                if attempt == max_retries - 1 {
                    let message =
                        format!("failed to materialize upstream responses request: {error}");
                    record_terminal_failure(
                        &log_svc,
                        &token_info,
                        &channel,
                        "responses",
                        "openai/responses",
                        &requested_model,
                        &actual_model,
                        &model_config.model_name,
                        &request_id,
                        "",
                        start.elapsed().as_millis() as i64,
                        is_stream,
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
                        "failed to materialize upstream responses request",
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
                    endpoint: "responses",
                    request_format: "openai/responses",
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

                if is_stream {
                    if responses_mode == ResponsesRuntimeMode::ChatBridge {
                        let stream = match adapter.parse_stream(resp, &actual_model) {
                            Ok(stream) => stream,
                            Err(error) => {
                                if let Err(e) = billing
                                    .refund_with_retry(
                                        &request_id,
                                        token_info.token_id,
                                        pre_consumed,
                                    )
                                    .await
                                {
                                    tracing::warn!(error = %e, "failed to refund quota");
                                }
                                channel_svc.record_relay_failure_async(
                                    channel.channel_id,
                                    channel.account_id,
                                    elapsed,
                                    0,
                                    format!(
                                        "failed to parse upstream bridged responses stream: {error}"
                                    ),
                                );
                                route_plan.exclude_selected_channel(&channel);
                                request_svc
                                    .try_update_execution_status(
                                        tracking.execution_id,
                                        ExecutionStatusUpdate {
                                            status: ExecutionStatus::Failed,
                                            error_message: Some(format!(
                                                "failed to parse upstream bridged responses stream: {error}"
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
                                    let message = format!(
                                        "failed to parse upstream bridged responses stream: {error}"
                                    );
                                    record_terminal_failure(
                                        &log_svc,
                                        &token_info,
                                        &channel,
                                        "responses",
                                        "openai/responses",
                                        &requested_model,
                                        &actual_model,
                                        &model_config.model_name,
                                        &request_id,
                                        &upstream_request_id,
                                        elapsed,
                                        true,
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
                                        tracing::warn!(error = %e, "failed to finalize rate limit failure (bridged stream parse error)");
                                    }
                                    return Err(OpenAiErrorResponse::internal_with(
                                        "failed to parse upstream bridged responses stream",
                                        error,
                                    ));
                                }
                                continue;
                            }
                        };
                        return Ok(build_chat_bridged_responses_stream_response(
                            stream,
                            token_info,
                            pre_consumed,
                            model_config,
                            group_ratio,
                            channel,
                            requested_model,
                            estimated_tokens,
                            elapsed,
                            client_ip,
                            log_svc,
                            channel_svc,
                            rate_limiter,
                            billing,
                            request_id,
                            upstream_request_id,
                            user_agent,
                            response_bridge,
                            input_snapshot.clone(),
                            request_svc,
                            tracking,
                        ));
                    }
                    return Ok(build_responses_stream_response(
                        resp,
                        token_info,
                        pre_consumed,
                        model_config,
                        group_ratio,
                        channel,
                        requested_model,
                        estimated_tokens,
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
                        request_svc,
                        tracking,
                    ));
                }

                let content_type = resp.headers().get(CONTENT_TYPE).cloned();
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
                            format!("failed to read upstream responses response: {error}"),
                        );
                        route_plan.exclude_selected_account(&channel);
                        request_svc
                            .try_update_execution_status(
                                tracking.execution_id,
                                ExecutionStatusUpdate {
                                    status: ExecutionStatus::Failed,
                                    error_message: Some(format!(
                                        "failed to read upstream responses response: {error}"
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
                                format!("failed to read upstream responses response: {error}");
                            record_terminal_failure(
                                &log_svc,
                                &token_info,
                                &channel,
                                "responses",
                                "openai/responses",
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
                                "failed to read upstream responses response",
                                error,
                            ));
                        }
                        continue;
                    }
                };
                if let Some(message) =
                    unusable_success_response_message(status, &body, "responses", false)
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
                            "responses",
                            "openai/responses",
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
                let parsed: ResponsesResponse = match responses_mode {
                    ResponsesRuntimeMode::Native => match serde_json::from_slice(&body) {
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
                                format!("failed to parse upstream responses response: {error}"),
                            );
                            route_plan.exclude_selected_account(&channel);
                            request_svc
                                .try_update_execution_status(
                                    tracking.execution_id,
                                    ExecutionStatusUpdate {
                                        status: ExecutionStatus::Failed,
                                        error_message: Some(format!(
                                            "failed to parse upstream responses response: {error}"
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
                                    format!("failed to parse upstream responses response: {error}");
                                record_terminal_failure(
                                    &log_svc,
                                    &token_info,
                                    &channel,
                                    "responses",
                                    "openai/responses",
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
                                    "failed to parse upstream responses response",
                                    error,
                                ));
                            }
                            continue;
                        }
                    },
                    ResponsesRuntimeMode::ChatBridge => match adapter
                        .parse_response(body.clone(), &actual_model)
                    {
                        Ok(parsed) => bridge_chat_completion_to_response(parsed),
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
                                format!("failed to parse bridged responses response: {error}"),
                            );
                            route_plan.exclude_selected_account(&channel);
                            request_svc
                                .try_update_execution_status(
                                    tracking.execution_id,
                                    ExecutionStatusUpdate {
                                        status: ExecutionStatus::Failed,
                                        error_message: Some(format!(
                                            "failed to parse bridged responses response: {error}"
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
                                    format!("failed to parse bridged responses response: {error}");
                                record_terminal_failure(
                                    &log_svc,
                                    &token_info,
                                    &channel,
                                    "responses",
                                    "openai/responses",
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
                                    "failed to parse bridged responses response",
                                    error,
                                ));
                            }
                            continue;
                        }
                    },
                };

                let usage = parsed
                    .usage
                    .as_ref()
                    .map(|usage| usage.to_usage())
                    .unwrap_or_else(|| fallback_usage(estimated_tokens));
                if responses_mode == ResponsesRuntimeMode::ChatBridge
                    && let Err(error) = response_bridge
                        .store(
                            &token_info,
                            parsed.clone(),
                            &input_snapshot,
                            &upstream_request_id,
                        )
                        .await
                {
                    tracing::warn!("failed to store bridged response snapshot: {error}");
                }
                if responses_mode == ResponsesRuntimeMode::Native
                    && let Err(error) = resource_affinity
                        .bind(&token_info, "response", &parsed.id, &channel)
                        .await
                {
                    tracing::warn!("failed to bind response affinity: {error}");
                }
                let upstream_model = if parsed.model.is_empty() {
                    actual_model.clone()
                } else {
                    parsed.model.clone()
                };
                update_request_success_tracking(
                    &request_svc,
                    tracking,
                    elapsed,
                    0,
                    upstream_model.clone(),
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
                    upstream_model,
                    client_ip,
                    user_agent,
                    "responses",
                    "openai/responses",
                    elapsed,
                    0,
                    false,
                );

                if responses_mode == ResponsesRuntimeMode::Native {
                    let mut response = build_json_bytes_response(body, content_type, &request_id);
                    insert_upstream_request_id_header(&mut response, &upstream_request_id);
                    return Ok(response);
                }

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
                        "responses",
                        "openai/responses",
                        &requested_model,
                        &actual_model,
                        &model_config.model_name,
                        &request_id,
                        &upstream_request_id,
                        elapsed,
                        is_stream,
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
                        "responses",
                        "openai/responses",
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
                        "failed to send upstream responses request",
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
