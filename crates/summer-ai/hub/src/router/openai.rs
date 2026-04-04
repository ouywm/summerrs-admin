use summer_web::axum::http::{HeaderMap, StatusCode};
use summer_web::axum::response::{IntoResponse, Response};
use summer_web::extractor::Component;
use summer_web::{get_api, post_api};

use summer_ai_core::provider::{ProviderErrorInfo, ProviderErrorKind, get_adapter};
use summer_ai_core::types::chat::ChatCompletionRequest;
use summer_ai_core::types::embedding::EmbeddingRequest;
use summer_ai_core::types::error::{
    OpenAiApiResult, OpenAiError, OpenAiErrorBody, OpenAiErrorResponse,
};
use summer_ai_core::types::model::ModelListResponse;
use summer_ai_core::types::responses::ResponsesRequest;
use summer_ai_model::entity::request::RequestStatus;
use summer_ai_model::entity::request_execution::ExecutionStatus;

use crate::auth::extractor::AiToken;
use crate::relay::billing::{
    BillingEngine, estimate_prompt_tokens, estimate_total_tokens_for_rate_limit,
};
use crate::relay::channel_router::{ChannelRouter, RouteSelectionExclusions, RouteSelectionState};
use crate::relay::http_client::UpstreamHttpClient;
use crate::relay::rate_limit::RateLimitEngine;
use crate::relay::stream::build_sse_response;
use crate::router::openai_passthrough::unusable_success_response_message;
use crate::service::channel::ChannelService;
use crate::service::log::LogService;
use crate::service::model::ModelService;
use crate::service::openai_chat_relay::OpenAiChatRelayService;
use crate::service::openai_embeddings_relay::OpenAiEmbeddingsRelayService;
use crate::service::openai_http::{
    extract_request_id, extract_upstream_request_id, fallback_usage, insert_request_id_header,
    insert_upstream_request_id_header,
};
use crate::service::openai_responses_relay::OpenAiResponsesRelayService;
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
use summer_common::extractor::ClientIp;
use summer_common::response::Json;

mod audio;
mod audio_transcribe;
mod completions;
mod files;
mod image_multipart;
mod images;
mod moderations;
mod rerank;
mod support;
#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) use completions::{
    bridge_chat_completion_to_completion, completion_request_to_chat_request,
};
pub use files::*;
#[allow(unused_imports)]
pub(crate) use support::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UpstreamFailureScope {
    Account,
    Channel,
}

#[derive(Debug, Clone)]
pub(crate) struct UpstreamProviderFailure {
    pub scope: UpstreamFailureScope,
    pub error: OpenAiErrorResponse,
    pub message: String,
}
use summer_common::user_agent::UserAgentInfo;

/// POST /v1/chat/completions
#[post_api("/v1/chat/completions")]
pub async fn chat_completions(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiChatRelayService>,
    ClientIp(client_ip): ClientIp,
    headers: HeaderMap,
    Json(req): Json<ChatCompletionRequest>,
) -> OpenAiApiResult<Response> {
    relay_svc.relay(token_info, client_ip, headers, req).await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn relay_chat_completions_impl(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(billing): Component<BillingEngine>,
    Component(rate_limiter): Component<RateLimitEngine>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(log_svc): Component<LogService>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(runtime_ops): Component<RuntimeOpsService>,
    Component(request_svc): Component<RequestService>,
    ClientIp(client_ip): ClientIp,
    headers: HeaderMap,
    Json(req): Json<ChatCompletionRequest>,
) -> OpenAiApiResult<Response> {
    let request_id = extract_request_id(&headers);
    let user_agent = UserAgentInfo::from_headers(&headers).raw;
    token_info
        .ensure_endpoint_allowed("chat")
        .map_err(|e| OpenAiErrorResponse::from_api_error(&e))?;
    token_info
        .ensure_model_allowed(&req.model)
        .map_err(|e| OpenAiErrorResponse::from_api_error(&e))?;
    let client_ip = client_ip.to_string();
    token_svc.update_last_used_ip_async(token_info.token_id, client_ip.clone());

    let model_config = billing
        .get_model_config_for_endpoint(&req.model, "chat")
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
            "chat",
            &route_exclusions,
        )
        .await
        .map_err(|e| OpenAiErrorResponse::internal_with("failed to build channel plan", e))?;
    let max_retries = 3;
    let start = std::time::Instant::now();
    let is_stream = req.stream;
    let requested_model = req.model.clone();
    let request_record_id = match request_svc
        .create_request(build_request_active_model(RequestSnapshotInput {
            request_id: &request_id,
            token_info: &token_info,
            endpoint: "chat/completions",
            request_format: "openai/chat_completions",
            requested_model: &requested_model,
            is_stream,
            client_ip: &client_ip,
            user_agent: &user_agent,
            headers: &headers,
            request_body: serde_json::to_value(&req).unwrap_or(serde_json::Value::Null),
        }))
        .await
    {
        Ok(record) => Some(record.id),
        Err(error) => {
            tracing::warn!(error = %error, request_id = %request_id, "failed to persist AI request snapshot");
            None
        }
    };
    let estimated_tokens = estimate_prompt_tokens(&req.messages);
    let estimated_total_tokens =
        estimate_total_tokens_for_rate_limit(&req.messages, req.max_tokens);

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
            .and_then(|v| v.as_str())
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

        let request_builder = match adapter.build_request(
            http_client.client(),
            &channel.base_url,
            &channel.api_key,
            &req,
            &actual_model,
        ) {
            Ok(rb) => rb,
            Err(e) => {
                if let Err(e) = billing
                    .refund_with_retry(&request_id, token_info.token_id, pre_consumed)
                    .await
                {
                    tracing::warn!(error = %e, "failed to refund quota (build request error)");
                }
                channel_svc.record_relay_failure_async(
                    channel.channel_id,
                    channel.account_id,
                    start.elapsed().as_millis() as i64,
                    0,
                    format!("failed to build upstream request: {e}"),
                );
                route_plan.exclude_selected_channel(&channel);
                tracing::warn!(
                    "failed to build upstream request: {e}, channel_id={}",
                    channel.channel_id
                );
                if attempt == max_retries - 1 {
                    let message = format!("failed to build upstream request: {e}");
                    record_terminal_failure(
                        &log_svc,
                        &token_info,
                        &channel,
                        "chat/completions",
                        "openai/chat_completions",
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
                    return Err(OpenAiErrorResponse::internal_with(
                        "failed to build upstream request",
                        e,
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
                    tracing::warn!(error = %e, "failed to refund quota (build request object error)");
                }
                channel_svc.record_relay_failure_async(
                    channel.channel_id,
                    channel.account_id,
                    start.elapsed().as_millis() as i64,
                    0,
                    format!("failed to materialize upstream request: {error}"),
                );
                route_plan.exclude_selected_channel(&channel);
                if attempt == max_retries - 1 {
                    let message = format!("failed to materialize upstream request: {error}");
                    record_terminal_failure(
                        &log_svc,
                        &token_info,
                        &channel,
                        "chat/completions",
                        "openai/chat_completions",
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
                        "failed to materialize upstream request",
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
                    endpoint: "chat/completions",
                    request_format: "openai/chat_completions",
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
                    let stream = match adapter.parse_stream(resp, &actual_model) {
                        Ok(stream) => stream,
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
                                format!("failed to parse upstream stream: {error}"),
                            );
                            route_plan.exclude_selected_channel(&channel);
                            request_svc
                                .try_update_execution_status(
                                    tracking.execution_id,
                                    ExecutionStatusUpdate {
                                        status: ExecutionStatus::Failed,
                                        error_message: Some(format!(
                                            "failed to parse upstream stream: {error}"
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
                                let message = format!("failed to parse upstream stream: {error}");
                                record_terminal_failure(
                                    &log_svc,
                                    &token_info,
                                    &channel,
                                    "chat/completions",
                                    "openai/chat_completions",
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
                                    tracing::warn!(error = %e, "failed to finalize rate limit failure");
                                }
                                return Err(OpenAiErrorResponse::internal_with(
                                    "failed to parse upstream stream",
                                    error,
                                ));
                            }
                            continue;
                        }
                    };
                    return Ok(build_sse_response(
                        stream,
                        token_info,
                        pre_consumed,
                        model_config,
                        group_ratio,
                        channel,
                        requested_model,
                        elapsed,
                        client_ip,
                        log_svc,
                        channel_svc,
                        rate_limiter,
                        billing,
                        request_id,
                        upstream_request_id,
                        user_agent,
                        request_svc,
                        tracking,
                    ));
                } else {
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
                                format!("failed to read upstream response: {error}"),
                            );
                            route_plan.exclude_selected_channel(&channel);
                            request_svc
                                .try_update_execution_status(
                                    tracking.execution_id,
                                    ExecutionStatusUpdate {
                                        status: ExecutionStatus::Failed,
                                        error_message: Some(format!(
                                            "failed to read upstream response: {error}"
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
                                let message = format!("failed to read upstream response: {error}");
                                record_terminal_failure(
                                    &log_svc,
                                    &token_info,
                                    &channel,
                                    "chat/completions",
                                    "openai/chat_completions",
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
                                    "failed to read upstream response",
                                    error,
                                ));
                            }
                            continue;
                        }
                    };
                    if let Some(message) =
                        unusable_success_response_message(status, &body, "chat/completions", false)
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
                                "chat/completions",
                                "openai/chat_completions",
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
                            if let Err(e) =
                                rate_limiter.finalize_failure_with_retry(&request_id).await
                            {
                                tracing::warn!(error = %e, "failed to finalize rate limit failure");
                            }
                            return Err(OpenAiErrorResponse::unsupported_endpoint(message));
                        }
                        continue;
                    }
                    let response_body_snapshot = snapshot_response_body_bytes(&body);
                    let parsed = match adapter.parse_response(body, &actual_model) {
                        Ok(parsed) => parsed,
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
                                format!("failed to parse upstream response: {error}"),
                            );
                            route_plan.exclude_selected_account(&channel);
                            request_svc
                                .try_update_execution_status(
                                    tracking.execution_id,
                                    ExecutionStatusUpdate {
                                        status: ExecutionStatus::Failed,
                                        error_message: Some(format!(
                                            "failed to parse upstream response: {error}"
                                        )),
                                        duration_ms: Some(elapsed as i32),
                                        first_token_ms: Some(0),
                                        response_status_code: Some(0),
                                        response_body: Some(response_body_snapshot.clone()),
                                        upstream_request_id: Some(upstream_request_id.clone()),
                                    },
                                )
                                .await;
                            if attempt == max_retries - 1 {
                                let message = format!("failed to parse upstream response: {error}");
                                record_terminal_failure(
                                    &log_svc,
                                    &token_info,
                                    &channel,
                                    "chat/completions",
                                    "openai/chat_completions",
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
                                        response_body: Some(response_body_snapshot),
                                    },
                                )
                                .await;
                                if let Err(e) =
                                    rate_limiter.finalize_failure_with_retry(&request_id).await
                                {
                                    tracing::warn!(error = %e, "failed to finalize rate limit failure");
                                }
                                return Err(OpenAiErrorResponse::internal_with(
                                    "failed to parse upstream response",
                                    error,
                                ));
                            }
                            continue;
                        }
                    };

                    let usage = parsed.usage.clone();
                    let upstream_model = parsed.model.clone();
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
                        "chat/completions",
                        "openai/chat_completions",
                        elapsed,
                        0,
                        false,
                    );

                    let mut response = Json(parsed).into_response();
                    insert_request_id_header(&mut response, &request_id);
                    insert_upstream_request_id_header(&mut response, &upstream_request_id);
                    return Ok(response);
                }
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
                        "chat/completions",
                        "openai/chat_completions",
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
                        "chat/completions",
                        "openai/chat_completions",
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
                        "failed to send upstream request",
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

pub(crate) use crate::service::openai_responses_relay::{
    build_json_bytes_response, settle_usage_accounting, spawn_usage_accounting_task,
};

/// POST /v1/responses
#[post_api("/v1/responses")]
pub async fn responses(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiResponsesRelayService>,
    ClientIp(client_ip): ClientIp,
    headers: HeaderMap,
    Json(req): Json<ResponsesRequest>,
) -> OpenAiApiResult<Response> {
    relay_svc.relay(token_info, client_ip, headers, req).await
}

/// POST /v1/embeddings
#[post_api("/v1/embeddings")]
pub async fn embeddings(
    AiToken(token_info): AiToken,
    Component(relay_svc): Component<OpenAiEmbeddingsRelayService>,
    ClientIp(client_ip): ClientIp,
    headers: HeaderMap,
    Json(req): Json<EmbeddingRequest>,
) -> OpenAiApiResult<Response> {
    relay_svc.relay(token_info, client_ip, headers, req).await
}
pub(crate) fn classify_upstream_provider_failure(
    channel_type: i16,
    status: StatusCode,
    headers: &HeaderMap,
    body: &[u8],
) -> UpstreamProviderFailure {
    let info = get_adapter(channel_type).parse_error(status.as_u16(), headers, body);
    let scope = match info.kind {
        ProviderErrorKind::InvalidRequest => UpstreamFailureScope::Channel,
        ProviderErrorKind::Authentication
        | ProviderErrorKind::RateLimit
        | ProviderErrorKind::Server
        | ProviderErrorKind::Api => UpstreamFailureScope::Account,
    };
    let message = if info.message.is_empty() {
        String::from_utf8_lossy(body).trim().to_string()
    } else {
        info.message.clone()
    };

    UpstreamProviderFailure {
        scope,
        error: provider_error_to_openai_response(status, &info),
        message,
    }
}

pub(crate) fn apply_upstream_failure_scope<T: RouteSelectionState>(
    exclusions: &mut T,
    channel: &crate::relay::channel_router::SelectedChannel,
    scope: UpstreamFailureScope,
) {
    match scope {
        UpstreamFailureScope::Account => exclusions.exclude_selected_account(channel),
        UpstreamFailureScope::Channel => exclusions.exclude_selected_channel(channel),
    }
}

fn provider_error_to_openai_response(
    status: StatusCode,
    info: &ProviderErrorInfo,
) -> OpenAiErrorResponse {
    let error_type = match info.kind {
        ProviderErrorKind::InvalidRequest => "invalid_request_error",
        ProviderErrorKind::Authentication => "authentication_error",
        ProviderErrorKind::RateLimit => "rate_limit_error",
        ProviderErrorKind::Server => "server_error",
        ProviderErrorKind::Api => "api_error",
    };
    let normalized_status = match info.kind {
        ProviderErrorKind::InvalidRequest => match status.as_u16() {
            404 => StatusCode::NOT_FOUND,
            400 | 413 | 422 => status,
            _ => StatusCode::BAD_REQUEST,
        },
        ProviderErrorKind::Authentication => match status.as_u16() {
            403 => StatusCode::FORBIDDEN,
            _ => StatusCode::UNAUTHORIZED,
        },
        ProviderErrorKind::RateLimit => StatusCode::TOO_MANY_REQUESTS,
        ProviderErrorKind::Server => {
            if status.is_server_error() {
                status
            } else {
                StatusCode::BAD_GATEWAY
            }
        }
        ProviderErrorKind::Api => {
            if status.is_success() {
                StatusCode::BAD_GATEWAY
            } else {
                status
            }
        }
    };

    OpenAiErrorResponse {
        status: normalized_status.into(),
        error: OpenAiError {
            error: OpenAiErrorBody {
                message: info.message.clone(),
                r#type: error_type.into(),
                param: None,
                code: Some(info.code.to_lowercase()),
            },
        },
    }
}

/// GET /v1/models
#[get_api("/v1/models")]
pub async fn list_models(
    AiToken(token_info): AiToken,
    Component(model_svc): Component<ModelService>,
    Component(token_svc): Component<TokenService>,
    ClientIp(client_ip): ClientIp,
) -> OpenAiApiResult<Json<ModelListResponse>> {
    token_svc.update_last_used_ip_async(token_info.token_id, client_ip.to_string());

    let models = model_svc
        .list_available(&token_info.group)
        .await
        .map_err(|e| OpenAiErrorResponse::internal_with("failed to list available models", e))?;

    Ok(Json(models))
}

/// GET /v1/models/{model}
#[get_api("/v1/models/{model}")]
pub async fn retrieve_model(
    AiToken(token_info): AiToken,
    Component(model_svc): Component<ModelService>,
    Component(token_svc): Component<TokenService>,
    ClientIp(client_ip): ClientIp,
    summer_common::extractor::Path(model): summer_common::extractor::Path<String>,
) -> OpenAiApiResult<Json<summer_ai_core::types::model::ModelObject>> {
    token_svc.update_last_used_ip_async(token_info.token_id, client_ip.to_string());

    let model = model_svc
        .get_available(&token_info.group, &model)
        .await
        .map_err(|e| OpenAiErrorResponse::internal_with("failed to query available model", e))?
        .ok_or_else(|| OpenAiErrorResponse::not_found("model not found"))?;

    Ok(Json(model))
}
