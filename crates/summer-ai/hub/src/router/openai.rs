use bytes::Bytes;
use futures::StreamExt;
use futures::stream::BoxStream;
use summer_web::axum::body::Body;
use summer_web::axum::http::{
    HeaderMap, HeaderValue, StatusCode,
    header::{CACHE_CONTROL, CONTENT_TYPE},
};
use summer_web::axum::response::{IntoResponse, Response};
use summer_web::extractor::Component;
use summer_web::{get_api, post_api};
use uuid::Uuid;

use summer_ai_core::provider::{
    ProviderErrorInfo, ProviderErrorKind, ResponsesRuntimeMode, get_adapter,
};
use summer_ai_core::types::chat::{
    ChatCompletionChunk, ChatCompletionRequest, ChatCompletionResponse,
};
use summer_ai_core::types::common::{Message, Usage};
use summer_ai_core::types::embedding::{EmbeddingRequest, EmbeddingResponse};
use summer_ai_core::types::error::{
    OpenAiApiResult, OpenAiError, OpenAiErrorBody, OpenAiErrorResponse,
};
use summer_ai_core::types::model::ModelListResponse;
use summer_ai_core::types::responses::{
    ResponseInputTokensDetails, ResponseOutputTokensDetails, ResponseUsage, ResponsesRequest,
    ResponsesResponse, estimate_input_tokens as estimate_response_input_tokens,
    estimate_total_tokens_for_rate_limit as estimate_response_total_tokens_for_rate_limit,
    extract_response_model, extract_response_usage, is_output_text_delta_event,
};
use summer_ai_model::entity::log::LogStatus;
use summer_ai_model::entity::request::RequestStatus;
use summer_ai_model::entity::request_execution::ExecutionStatus;

use crate::auth::extractor::AiToken;
use crate::relay::billing::{
    BillingEngine, ModelConfigInfo, estimate_prompt_tokens, estimate_total_tokens_for_rate_limit,
};
use crate::relay::channel_router::{
    ChannelRouter, RouteSelectionExclusions, RouteSelectionState, SelectedChannel,
};
use crate::relay::http_client::UpstreamHttpClient;
use crate::relay::rate_limit::RateLimitEngine;
use crate::relay::stream::build_sse_response;
use crate::router::openai_passthrough::unusable_success_response_message;
use crate::service::channel::ChannelService;
use crate::service::log::{AiFailureLogRecord, AiUsageLogRecord, LogService};
use crate::service::model::ModelService;
use crate::service::request::{
    ExecutionSnapshotInput, ExecutionStatusUpdate, RequestService, RequestSnapshotInput,
    RequestStatusUpdate, build_execution_active_model, build_request_active_model,
    snapshot_response_body_bytes,
};
use crate::service::resource_affinity::ResourceAffinityService;
use crate::service::response_bridge::ResponseBridgeService;
use crate::service::runtime_ops::RuntimeOpsService;
use crate::service::token::{TokenInfo, TokenService};
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

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct RequestTrackingIds {
    pub(crate) request_id: Option<i64>,
    pub(crate) execution_id: Option<i64>,
}

struct FailureTrackingUpdate {
    status_code: i32,
    message: String,
    elapsed_ms: i64,
    upstream_model: Option<String>,
    upstream_request_id: Option<String>,
    response_body: Option<serde_json::Value>,
}

fn map_adapter_build_error(context: &str, error: anyhow::Error) -> OpenAiErrorResponse {
    let message = error.to_string();
    if message.contains("is not supported") {
        return OpenAiErrorResponse::unsupported_endpoint(message);
    }
    OpenAiErrorResponse::internal_with(context, error)
}

#[allow(clippy::too_many_arguments)]
fn record_terminal_failure(
    log_svc: &LogService,
    token_info: &TokenInfo,
    channel: &SelectedChannel,
    endpoint: &str,
    request_format: &str,
    requested_model: &str,
    upstream_model: &str,
    model_name: &str,
    request_id: &str,
    upstream_request_id: &str,
    elapsed_ms: i64,
    is_stream: bool,
    client_ip: &str,
    user_agent: &str,
    status_code: i32,
    message: impl Into<String>,
) {
    log_svc.record_failure_async(
        token_info,
        channel,
        AiFailureLogRecord {
            endpoint: endpoint.to_string(),
            request_format: request_format.to_string(),
            request_id: request_id.to_string(),
            upstream_request_id: upstream_request_id.to_string(),
            requested_model: requested_model.to_string(),
            upstream_model: upstream_model.to_string(),
            model_name: model_name.to_string(),
            elapsed_time: elapsed_ms as i32,
            is_stream,
            client_ip: client_ip.to_string(),
            user_agent: user_agent.to_string(),
            status_code,
            content: message.into(),
        },
    );
}

async fn update_request_failure_tracking(
    request_svc: &RequestService,
    tracking: RequestTrackingIds,
    failure: FailureTrackingUpdate,
) {
    request_svc
        .try_update_execution_status(
            tracking.execution_id,
            ExecutionStatusUpdate {
                status: ExecutionStatus::Failed,
                error_message: Some(failure.message.clone()),
                duration_ms: Some(failure.elapsed_ms as i32),
                first_token_ms: Some(0),
                response_status_code: Some(failure.status_code),
                response_body: failure.response_body.clone(),
                upstream_request_id: failure.upstream_request_id,
            },
        )
        .await;
    request_svc
        .try_update_request_status(
            tracking.request_id,
            RequestStatusUpdate {
                status: RequestStatus::Failed,
                error_message: Some(failure.message),
                duration_ms: Some(failure.elapsed_ms as i32),
                first_token_ms: Some(0),
                response_status_code: Some(failure.status_code),
                response_body: failure.response_body,
                upstream_model: failure.upstream_model,
            },
        )
        .await;
}

async fn update_request_success_tracking(
    request_svc: &RequestService,
    tracking: RequestTrackingIds,
    elapsed_ms: i64,
    first_token_ms: i32,
    upstream_model: String,
    upstream_request_id: String,
    response_body: Option<serde_json::Value>,
) {
    request_svc
        .try_update_execution_status(
            tracking.execution_id,
            ExecutionStatusUpdate {
                status: ExecutionStatus::Success,
                error_message: None,
                duration_ms: Some(elapsed_ms as i32),
                first_token_ms: Some(first_token_ms),
                response_status_code: Some(200),
                response_body: response_body.clone(),
                upstream_request_id: Some(upstream_request_id),
            },
        )
        .await;
    request_svc
        .try_update_request_status(
            tracking.request_id,
            RequestStatusUpdate {
                status: RequestStatus::Success,
                error_message: None,
                duration_ms: Some(elapsed_ms as i32),
                first_token_ms: Some(first_token_ms),
                response_status_code: Some(200),
                response_body,
                upstream_model: Some(upstream_model),
            },
        )
        .await;
}

/// POST /v1/chat/completions
#[post_api("/v1/chat/completions")]
#[allow(clippy::too_many_arguments)]
pub async fn chat_completions(
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

#[derive(Default)]
struct ResponsesStreamTracker {
    buffer: Vec<u8>,
    usage: Option<Usage>,
    upstream_model: String,
    response_id: String,
}

impl ResponsesStreamTracker {
    fn ingest(
        &mut self,
        chunk: &Bytes,
        start: &std::time::Instant,
        first_token_time: &mut Option<i64>,
    ) {
        self.buffer.extend_from_slice(chunk);

        while let Some(pos) = find_double_newline(&self.buffer) {
            let event_bytes = self.buffer[..pos].to_vec();
            self.buffer = self.buffer[pos + 2..].to_vec();

            let event_block = match std::str::from_utf8(&event_bytes) {
                Ok(s) => s.to_string(),
                Err(_) => String::from_utf8_lossy(&event_bytes).into_owned(),
            };

            let mut data = String::new();
            for line in event_block.lines() {
                if let Some(value) = line.strip_prefix("data:") {
                    if !data.is_empty() {
                        data.push('\n');
                    }
                    data.push_str(value.trim_start());
                }
            }

            if data.is_empty() || data == "[DONE]" {
                continue;
            }

            let Ok(payload) = serde_json::from_str::<serde_json::Value>(&data) else {
                continue;
            };

            if first_token_time.is_none() && is_output_text_delta_event(&payload) {
                *first_token_time = Some(start.elapsed().as_millis() as i64);
            }

            if self.upstream_model.is_empty()
                && let Some(model) = extract_response_model(&payload)
            {
                self.upstream_model = model;
            }

            if self.response_id.is_empty()
                && let Some(response_id) = extract_response_id(&payload)
            {
                self.response_id = response_id;
            }

            if let Some(usage) = extract_response_usage(&payload) {
                self.usage = Some(usage);
            }
        }
    }
}

/// Find the position of `\n\n` in a byte slice.
fn find_double_newline(buf: &[u8]) -> Option<usize> {
    buf.windows(2).position(|w| w == b"\n\n")
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

pub(crate) fn fallback_usage(prompt_tokens: i32) -> Usage {
    Usage {
        prompt_tokens,
        completion_tokens: 0,
        total_tokens: prompt_tokens,
        cached_tokens: 0,
        reasoning_tokens: 0,
    }
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

fn response_usage_from_usage(usage: &Usage) -> ResponseUsage {
    ResponseUsage {
        input_tokens: usage.prompt_tokens,
        output_tokens: usage.completion_tokens,
        total_tokens: usage.total_tokens,
        input_tokens_details: (usage.cached_tokens > 0).then_some(ResponseInputTokensDetails {
            cached_tokens: usage.cached_tokens,
        }),
        output_tokens_details: (usage.reasoning_tokens > 0).then_some(
            ResponseOutputTokensDetails {
                reasoning_tokens: usage.reasoning_tokens,
            },
        ),
    }
}

fn response_output_text_from_message(message: &Message) -> Option<String> {
    match &message.content {
        serde_json::Value::String(text) if !text.is_empty() => Some(text.clone()),
        serde_json::Value::Array(items) => {
            let text = items
                .iter()
                .filter_map(|item| item.get("text").and_then(serde_json::Value::as_str))
                .collect::<Vec<_>>()
                .join("");
            (!text.is_empty()).then_some(text)
        }
        _ => None,
    }
}

fn bridge_chat_completion_to_response(response: ChatCompletionResponse) -> ResponsesResponse {
    let choice = response.choices.into_iter().next();
    let output_text = choice
        .as_ref()
        .and_then(|choice| response_output_text_from_message(&choice.message));

    ResponsesResponse {
        id: response.id,
        object: "response".into(),
        created_at: response.created,
        model: response.model,
        status: "completed".into(),
        usage: Some(response_usage_from_usage(&response.usage)),
        output_text,
        extra: serde_json::Map::new(),
    }
}

fn responses_sse_bytes(payload: &serde_json::Value) -> Bytes {
    Bytes::from(format!("data: {payload}\n\n"))
}

#[allow(clippy::too_many_arguments)]
fn build_responses_stream_response(
    upstream: reqwest::Response,
    token_info: crate::service::token::TokenInfo,
    pre_consumed: i64,
    model_config: ModelConfigInfo,
    group_ratio: f64,
    channel: crate::relay::channel_router::SelectedChannel,
    requested_model: String,
    estimated_prompt_tokens: i32,
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
) -> Response {
    let status = upstream.status();
    let content_type = upstream.headers().get(CONTENT_TYPE).cloned();
    let response_request_id = request_id.clone();
    let response_upstream_request_id = upstream_request_id.clone();

    let stream = async_stream::stream! {
        let start = std::time::Instant::now();
        let mut tracker = ResponsesStreamTracker::default();
        let mut first_token_time = None;
        let mut stream_error = None;
        let mut byte_stream = upstream.bytes_stream();

        while let Some(result) = byte_stream.next().await {
            match result {
                Ok(chunk) => {
                    tracker.ingest(&chunk, &start, &mut first_token_time);
                    yield Ok::<Bytes, std::convert::Infallible>(chunk);
                }
                Err(error) => {
                    tracing::error!("responses stream read error: {error}");
                    stream_error = Some(error.to_string());
                    break;
                }
            }
        }

        let total_elapsed = start_elapsed + start.elapsed().as_millis() as i64;
        if !tracker.response_id.is_empty()
            && let Err(error) = resource_affinity
                .bind(&token_info, "response", &tracker.response_id, &channel)
                .await
        {
            tracing::warn!("failed to bind streamed response affinity: {error}");
        }

        if let Some(usage) = tracker.usage {
            let upstream_model = if tracker.upstream_model.is_empty() {
                requested_model.clone()
            } else {
                tracker.upstream_model
            };

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
                request_id,
                upstream_request_id,
                requested_model,
                upstream_model,
                client_ip,
                user_agent,
                "responses",
                "openai/responses",
                total_elapsed,
                first_token_time.unwrap_or(0) as i32,
                true,
            );
        } else {
            let fallback_reason = stream_error.unwrap_or_else(|| "response stream ended without usage".into());
            billing.refund_later(request_id.clone(), token_info.token_id, pre_consumed);
            let rl = rate_limiter.clone();
            let request_id_for_task = request_id.clone();
            tokio::spawn(async move {
                if let Err(error) = rl.finalize_failure_with_retry(&request_id_for_task).await {
                    tracing::warn!("failed to finalize responses rate limit failure: {error}");
                }
            });
            channel_svc.record_relay_failure_async(
                channel.channel_id,
                channel.account_id,
                total_elapsed,
                0,
                if estimated_prompt_tokens > 0 {
                    format!("{fallback_reason}; estimated_prompt_tokens={estimated_prompt_tokens}")
                } else {
                    fallback_reason
                },
            );
        }
    };

    let mut response = Response::builder()
        .status(status)
        .body(Body::from_stream(stream))
        .unwrap_or_else(|_| Response::new(Body::empty()));
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

#[allow(clippy::too_many_arguments)]
fn build_chat_bridged_responses_stream_response(
    upstream: BoxStream<'static, anyhow::Result<ChatCompletionChunk>>,
    token_info: crate::service::token::TokenInfo,
    pre_consumed: i64,
    model_config: ModelConfigInfo,
    group_ratio: f64,
    channel: crate::relay::channel_router::SelectedChannel,
    requested_model: String,
    estimated_prompt_tokens: i32,
    start_elapsed: i64,
    client_ip: String,
    log_svc: LogService,
    channel_svc: ChannelService,
    rate_limiter: RateLimitEngine,
    billing: BillingEngine,
    request_id: String,
    upstream_request_id: String,
    user_agent: String,
    response_bridge: ResponseBridgeService,
    input_snapshot: serde_json::Value,
) -> Response {
    let response_request_id = request_id.clone();
    let response_upstream_request_id = upstream_request_id.clone();

    let stream = async_stream::stream! {
        let start = std::time::Instant::now();
        let mut upstream = upstream;
        let mut first_token_time = None;
        let mut usage = None;
        let mut response_id = String::new();
        let mut upstream_model = String::new();
        let mut created_at = 0_i64;
        let mut output_text = String::new();
        let mut stream_error = None;
        let mut emitted_created = false;

        while let Some(item) = upstream.next().await {
            match item {
                Ok(chunk) => {
                    if response_id.is_empty() {
                        response_id = chunk.id.clone();
                        created_at = chunk.created;
                    }
                    if upstream_model.is_empty() && !chunk.model.is_empty() {
                        upstream_model = chunk.model.clone();
                    }

                    if !emitted_created {
                        emitted_created = true;
                        yield Ok::<Bytes, std::convert::Infallible>(responses_sse_bytes(&serde_json::json!({
                            "type": "response.created",
                            "response": {
                                "id": response_id,
                                "object": "response",
                                "created_at": created_at,
                                "model": if upstream_model.is_empty() { requested_model.clone() } else { upstream_model.clone() },
                                "status": "in_progress"
                            }
                        })));
                    }

                    for choice in &chunk.choices {
                        if let Some(text) = choice.delta.content.as_ref()
                            && !text.is_empty()
                        {
                            if first_token_time.is_none() {
                                first_token_time = Some(start.elapsed().as_millis() as i64);
                            }
                            output_text.push_str(text);
                            yield Ok(responses_sse_bytes(&serde_json::json!({
                                "type": "response.output_text.delta",
                                "delta": text,
                            })));
                        }
                    }

                    if let Some(chunk_usage) = chunk.usage {
                        usage = Some(chunk_usage);
                    }
                }
                Err(error) => {
                    tracing::error!("responses bridge stream read error: {error}");
                    stream_error = Some(error.to_string());
                    break;
                }
            }
        }

        let total_elapsed = start_elapsed + start.elapsed().as_millis() as i64;
        let completed_model = if upstream_model.is_empty() {
            requested_model.clone()
        } else {
            upstream_model.clone()
        };

        if let Some(usage) = usage {
            let bridged_response = ResponsesResponse {
                id: response_id.clone(),
                object: "response".into(),
                created_at,
                model: completed_model.clone(),
                status: "completed".into(),
                usage: Some(response_usage_from_usage(&usage)),
                output_text: (!output_text.is_empty()).then_some(output_text.clone()),
                extra: serde_json::Map::new(),
            };
            if let Err(error) = response_bridge
                .store(
                    &token_info,
                    bridged_response.clone(),
                    &input_snapshot,
                    &upstream_request_id,
                )
                .await
            {
                tracing::warn!("failed to store bridged response snapshot: {error}");
            }
            yield Ok(responses_sse_bytes(&serde_json::json!({
                "type": "response.completed",
                "response": bridged_response
            })));
            yield Ok(Bytes::from_static(b"data: [DONE]\n\n"));

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
                request_id,
                upstream_request_id,
                requested_model,
                completed_model,
                client_ip,
                user_agent,
                "responses",
                "openai/responses",
                total_elapsed,
                first_token_time.unwrap_or(0) as i32,
                true,
            );
        } else {
            let fallback_reason = stream_error.unwrap_or_else(|| "response bridge stream ended without usage".into());
            billing.refund_later(request_id.clone(), token_info.token_id, pre_consumed);
            let rl = rate_limiter.clone();
            let request_id_for_task = request_id.clone();
            tokio::spawn(async move {
                if let Err(error) = rl.finalize_failure_with_retry(&request_id_for_task).await {
                    tracing::warn!("failed to finalize bridged responses rate limit failure: {error}");
                }
            });
            channel_svc.record_relay_failure_async(
                channel.channel_id,
                channel.account_id,
                total_elapsed,
                0,
                if estimated_prompt_tokens > 0 {
                    format!("{fallback_reason}; estimated_prompt_tokens={estimated_prompt_tokens}")
                } else {
                    fallback_reason
                },
            );
        }
    };

    let mut response = Response::builder()
        .status(StatusCode::OK)
        .body(Body::from_stream(stream))
        .unwrap_or_else(|_| Response::new(Body::empty()));
    response
        .headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static("text/event-stream"));
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    insert_request_id_header(&mut response, &response_request_id);
    insert_upstream_request_id_header(&mut response, &response_upstream_request_id);
    response
}

/// POST /v1/responses
#[post_api("/v1/responses")]
#[allow(clippy::too_many_arguments)]
pub async fn responses(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(billing): Component<BillingEngine>,
    Component(rate_limiter): Component<RateLimitEngine>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(log_svc): Component<LogService>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(response_bridge): Component<ResponseBridgeService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    Component(runtime_ops): Component<RuntimeOpsService>,
    ClientIp(client_ip): ClientIp,
    headers: HeaderMap,
    Json(req): Json<ResponsesRequest>,
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

    rate_limiter
        .reserve(&token_info, &request_id, estimated_total_tokens)
        .await
        .map_err(|e| OpenAiErrorResponse::from_quota_error(&e))?;

    for attempt in 0..max_retries {
        if attempt > 0 {
            runtime_ops.record_fallback_async();
        }
        let Some(channel) = route_plan.next() else {
            if let Err(e) = rate_limiter.finalize_failure_with_retry(&request_id).await {
                tracing::warn!(error = %e, "failed to finalize rate limit failure (no channel)");
            }
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
                        format!("failed to build upstream responses request: {error}"),
                    );
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

        match request_builder.send().await {
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
                                        true,
                                        &client_ip,
                                        &user_agent,
                                        0,
                                        format!(
                                            "failed to parse upstream bridged responses stream: {error}"
                                        ),
                                    );
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
                                0,
                                format!("failed to read upstream responses response: {error}"),
                            );
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
                                    0,
                                    format!("failed to parse upstream responses response: {error}"),
                                );
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
                                    0,
                                    format!("failed to parse bridged responses response: {error}"),
                                );
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
                        &extract_upstream_request_id(&headers),
                        elapsed,
                        is_stream,
                        &client_ip,
                        &user_agent,
                        status_code,
                        failure.message.clone(),
                    );
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
                        "",
                        elapsed,
                        is_stream,
                        &client_ip,
                        &user_agent,
                        0,
                        error.to_string(),
                    );
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
    Err(OpenAiErrorResponse::no_available_channel(
        "all channels failed",
    ))
}

/// POST /v1/embeddings
#[post_api("/v1/embeddings")]
#[allow(clippy::too_many_arguments)]
pub async fn embeddings(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(billing): Component<BillingEngine>,
    Component(rate_limiter): Component<RateLimitEngine>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(log_svc): Component<LogService>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(runtime_ops): Component<RuntimeOpsService>,
    ClientIp(client_ip): ClientIp,
    headers: HeaderMap,
    Json(req): Json<EmbeddingRequest>,
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

    rate_limiter
        .reserve(&token_info, &request_id, i64::from(estimated_tokens))
        .await
        .map_err(|e| OpenAiErrorResponse::from_quota_error(&e))?;

    for attempt in 0..max_retries {
        if attempt > 0 {
            runtime_ops.record_fallback_async();
        }
        let Some(channel) = route_plan.next() else {
            if let Err(e) = rate_limiter.finalize_failure_with_retry(&request_id).await {
                tracing::warn!(error = %e, "failed to finalize rate limit failure (no channel)");
            }
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
                        format!("failed to build upstream embeddings request: {error}"),
                    );
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

        match request_builder.send().await {
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
                                0,
                                format!("failed to read upstream embeddings response: {error}"),
                            );
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
                                0,
                                format!("failed to parse upstream embeddings response: {error}"),
                            );
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
                        &extract_upstream_request_id(&headers),
                        elapsed,
                        false,
                        &client_ip,
                        &user_agent,
                        status_code,
                        failure.message.clone(),
                    );
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
                        "",
                        elapsed,
                        false,
                        &client_ip,
                        &user_agent,
                        0,
                        error.to_string(),
                    );
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
    Err(OpenAiErrorResponse::no_available_channel(
        "all channels failed",
    ))
}

pub(crate) fn extract_request_id(headers: &HeaderMap) -> String {
    request_header_value(headers, &["x-request-id", "request-id"])
        .unwrap_or_else(|| Uuid::new_v4().to_string())
}

pub(crate) fn extract_upstream_request_id(headers: &HeaderMap) -> String {
    request_header_value(
        headers,
        &[
            "x-request-id",
            "request-id",
            "x-oneapi-request-id",
            "openai-request-id",
            "anthropic-request-id",
            "cf-ray",
        ],
    )
    .unwrap_or_default()
}

pub(crate) fn request_header_value(headers: &HeaderMap, names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| {
        headers
            .get(*name)
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    })
}

pub(crate) fn insert_request_id_header(response: &mut Response, request_id: &str) {
    if let Ok(value) = HeaderValue::from_str(request_id) {
        response.headers_mut().insert("x-request-id", value);
    }
}

pub(crate) fn insert_upstream_request_id_header(
    response: &mut Response,
    upstream_request_id: &str,
) {
    if upstream_request_id.is_empty() {
        return;
    }

    if let Ok(value) = HeaderValue::from_str(upstream_request_id) {
        response
            .headers_mut()
            .insert("x-upstream-request-id", value);
    }
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

fn extract_response_id(payload: &serde_json::Value) -> Option<String> {
    payload
        .get("response")
        .and_then(|response| response.get("id"))
        .or_else(|| payload.get("id"))
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned)
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
