use bytes::Bytes;
use futures::StreamExt;
use summer_web::axum::body::Body;
use summer_web::axum::http::{
    HeaderMap, HeaderValue, StatusCode,
    header::{CACHE_CONTROL, CONTENT_TYPE},
};
use summer_web::axum::response::{IntoResponse, Response};
use summer_web::extractor::Component;
use summer_web::{get_api, post_api};
use uuid::Uuid;

use summer_ai_core::provider::{ProviderErrorInfo, ProviderErrorKind, get_adapter};
use summer_ai_core::types::chat::ChatCompletionRequest;
use summer_ai_core::types::common::Usage;
use summer_ai_core::types::embedding::{EmbeddingRequest, EmbeddingResponse};
use summer_ai_core::types::error::{
    OpenAiApiResult, OpenAiError, OpenAiErrorBody, OpenAiErrorResponse,
};
use summer_ai_core::types::model::ModelListResponse;
use summer_ai_core::types::responses::{
    ResponsesRequest, ResponsesResponse, estimate_input_tokens as estimate_response_input_tokens,
    estimate_total_tokens_for_rate_limit as estimate_response_total_tokens_for_rate_limit,
    extract_response_model, extract_response_usage, is_output_text_delta_event,
};

use crate::auth::extractor::AiToken;
use crate::relay::billing::{
    BillingEngine, ModelConfigInfo, estimate_prompt_tokens, estimate_total_tokens_for_rate_limit,
};
use crate::relay::channel_router::{ChannelRouter, RouteSelectionExclusions, RouteSelectionState};
use crate::relay::http_client::UpstreamHttpClient;
use crate::relay::rate_limit::RateLimitEngine;
use crate::relay::stream::build_sse_response;
use crate::router::openai_passthrough::unusable_success_response_message;
use crate::service::channel::ChannelService;
use crate::service::log::{AiUsageLogRecord, ChatCompletionLogRecord, LogService};
use crate::service::model::ModelService;
use crate::service::resource_affinity::ResourceAffinityService;
use crate::service::token::TokenService;
use summer_common::extractor::ClientIp;
use summer_common::response::Json;

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

fn map_adapter_build_error(context: &str, error: anyhow::Error) -> OpenAiErrorResponse {
    let message = error.to_string();
    if message.contains("is not supported") {
        return OpenAiErrorResponse::unsupported_endpoint(message);
    }
    OpenAiErrorResponse::internal_with(context, error)
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
    let estimated_tokens = estimate_prompt_tokens(&req.messages);
    let estimated_total_tokens =
        estimate_total_tokens_for_rate_limit(&req.messages, req.max_tokens);

    rate_limiter
        .reserve(&token_info, &request_id, estimated_total_tokens)
        .await
        .map_err(|e| OpenAiErrorResponse::from_quota_error(&e))?;

    for attempt in 0..max_retries {
        let Some(channel) = route_plan.next() else {
            let _ = rate_limiter.finalize_failure_with_retry(&request_id).await;
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
                let _ = rate_limiter.finalize_failure_with_retry(&request_id).await;
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
                let _ = billing
                    .refund_with_retry(&request_id, token_info.token_id, pre_consumed)
                    .await;
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
                    let _ = rate_limiter.finalize_failure_with_retry(&request_id).await;
                    return Err(OpenAiErrorResponse::internal_with(
                        "failed to build upstream request",
                        e,
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
                    let stream = match adapter.parse_stream(resp, &actual_model) {
                        Ok(stream) => stream,
                        Err(error) => {
                            let _ = billing
                                .refund_with_retry(&request_id, token_info.token_id, pre_consumed)
                                .await;
                            channel_svc.record_relay_failure_async(
                                channel.channel_id,
                                channel.account_id,
                                elapsed,
                                0,
                                format!("failed to parse upstream stream: {error}"),
                            );
                            route_plan.exclude_selected_channel(&channel);
                            if attempt == max_retries - 1 {
                                let _ = rate_limiter.finalize_failure_with_retry(&request_id).await;
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
                    ));
                } else {
                    let body = match resp.bytes().await {
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
                            route_plan.exclude_selected_channel(&channel);
                            if attempt == max_retries - 1 {
                                let _ = rate_limiter.finalize_failure_with_retry(&request_id).await;
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
                            let _ = rate_limiter.finalize_failure_with_retry(&request_id).await;
                            return Err(OpenAiErrorResponse::unsupported_endpoint(message));
                        }
                        continue;
                    }
                    let parsed = match adapter.parse_response(body, &actual_model) {
                        Ok(parsed) => parsed,
                        Err(error) => {
                            let _ = billing
                                .refund_with_retry(&request_id, token_info.token_id, pre_consumed)
                                .await;
                            channel_svc.record_relay_failure_async(
                                channel.channel_id,
                                channel.account_id,
                                elapsed,
                                0,
                                format!("failed to parse upstream response: {error}"),
                            );
                            route_plan.exclude_selected_account(&channel);
                            if attempt == max_retries - 1 {
                                let _ = rate_limiter.finalize_failure_with_retry(&request_id).await;
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
                    let ti = token_info.clone();
                    let mc = model_config.clone();
                    let ch = channel.clone();
                    let rm = requested_model.clone();
                    let ip = client_ip.clone();
                    let bl = billing.clone();
                    let ls = log_svc.clone();
                    let cs = channel_svc.clone();
                    let rl = rate_limiter.clone();
                    let request_id_for_task = request_id.clone();
                    let upstream_request_id_for_task = upstream_request_id.clone();
                    let user_agent_for_task = user_agent.clone();
                    tokio::spawn(async move {
                        let logged_quota =
                            BillingEngine::calculate_actual_quota(&usage, &mc, group_ratio);
                        let actual_quota = match bl
                            .post_consume_with_retry(
                                &request_id_for_task,
                                &ti,
                                pre_consumed,
                                &usage,
                                &mc,
                                group_ratio,
                            )
                            .await
                        {
                            Ok(quota) => quota,
                            Err(error) => {
                                tracing::error!(
                                    "failed to settle non-stream usage asynchronously: {error}"
                                );
                                logged_quota
                            }
                        };

                        ls.record_chat_completion_async(
                            &ti,
                            &ch,
                            &usage,
                            ChatCompletionLogRecord {
                                request_id: request_id_for_task.clone(),
                                upstream_request_id: upstream_request_id_for_task,
                                requested_model: rm,
                                upstream_model,
                                model_name: mc.model_name,
                                quota: actual_quota,
                                elapsed_time: elapsed as i32,
                                first_token_time: 0,
                                is_stream: false,
                                client_ip: ip,
                                user_agent: user_agent_for_task,
                            },
                        );

                        if let Err(error) = rl
                            .finalize_success_with_retry(
                                &request_id_for_task,
                                i64::from(usage.total_tokens),
                            )
                            .await
                        {
                            tracing::warn!("failed to finalize rate limit success: {error}");
                        }

                        if let Err(error) = cs
                            .record_relay_success(ch.channel_id, ch.account_id, elapsed)
                            .await
                        {
                            tracing::warn!("failed to update relay success health state: {error}");
                        }
                    });

                    let mut response = Json(parsed).into_response();
                    insert_request_id_header(&mut response, &request_id);
                    return Ok(response);
                }
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

#[derive(Default)]
struct ResponsesStreamTracker {
    buffer: String,
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
        self.buffer.push_str(&String::from_utf8_lossy(chunk));

        while let Some(pos) = self.buffer.find("\n\n") {
            let event_block = self.buffer[..pos].to_string();
            self.buffer = self.buffer[pos + 2..].to_string();

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
        let logged_quota =
            BillingEngine::calculate_actual_quota(&usage, &model_config, group_ratio);
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
        .unwrap();
    response.headers_mut().insert(
        CONTENT_TYPE,
        content_type.unwrap_or_else(|| HeaderValue::from_static("application/json")),
    );
    insert_request_id_header(&mut response, request_id);
    response
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
        .unwrap();
    response.headers_mut().insert(
        CONTENT_TYPE,
        content_type.unwrap_or_else(|| HeaderValue::from_static("text/event-stream")),
    );
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    insert_request_id_header(&mut response, &response_request_id);
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
    Component(resource_affinity): Component<ResourceAffinityService>,
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
        let Some(channel) = route_plan.next() else {
            let _ = rate_limiter.finalize_failure_with_retry(&request_id).await;
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
                let _ = rate_limiter.finalize_failure_with_retry(&request_id).await;
                return Err(OpenAiErrorResponse::from_quota_error(&error));
            }
        };

        let adapter = get_adapter(channel.channel_type);
        let request_builder = match adapter.build_responses_request(
            http_client.client(),
            &channel.base_url,
            &channel.api_key,
            &raw_request,
            &actual_model,
        ) {
            Ok(builder) => builder,
            Err(error) => {
                let _ = billing
                    .refund_with_retry(&request_id, token_info.token_id, pre_consumed)
                    .await;
                channel_svc.record_relay_failure_async(
                    channel.channel_id,
                    channel.account_id,
                    start.elapsed().as_millis() as i64,
                    0,
                    format!("failed to build upstream responses request: {error}"),
                );
                route_plan.exclude_selected_channel(&channel);
                if attempt == max_retries - 1 {
                    let _ = rate_limiter.finalize_failure_with_retry(&request_id).await;
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
                        let _ = billing
                            .refund_with_retry(&request_id, token_info.token_id, pre_consumed)
                            .await;
                        channel_svc.record_relay_failure_async(
                            channel.channel_id,
                            channel.account_id,
                            elapsed,
                            0,
                            format!("failed to read upstream responses response: {error}"),
                        );
                        route_plan.exclude_selected_account(&channel);
                        if attempt == max_retries - 1 {
                            let _ = rate_limiter.finalize_failure_with_retry(&request_id).await;
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
                        let _ = rate_limiter.finalize_failure_with_retry(&request_id).await;
                        return Err(OpenAiErrorResponse::unsupported_endpoint(message));
                    }
                    continue;
                }
                let parsed: ResponsesResponse = match serde_json::from_slice(&body) {
                    Ok(parsed) => parsed,
                    Err(error) => {
                        let _ = billing
                            .refund_with_retry(&request_id, token_info.token_id, pre_consumed)
                            .await;
                        channel_svc.record_relay_failure_async(
                            channel.channel_id,
                            channel.account_id,
                            elapsed,
                            0,
                            format!("failed to parse upstream responses response: {error}"),
                        );
                        route_plan.exclude_selected_account(&channel);
                        if attempt == max_retries - 1 {
                            let _ = rate_limiter.finalize_failure_with_retry(&request_id).await;
                            return Err(OpenAiErrorResponse::internal_with(
                                "failed to parse upstream responses response",
                                error,
                            ));
                        }
                        continue;
                    }
                };

                let usage = parsed
                    .usage
                    .as_ref()
                    .map(|usage| usage.to_usage())
                    .unwrap_or_else(|| fallback_usage(estimated_tokens));
                if let Err(error) = resource_affinity
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
                    upstream_request_id,
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

                return Ok(build_json_bytes_response(body, content_type, &request_id));
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
                    let _ = rate_limiter.finalize_failure_with_retry(&request_id).await;
                    return Err(OpenAiErrorResponse::internal_with(
                        "failed to send upstream responses request",
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
        let Some(channel) = route_plan.next() else {
            let _ = rate_limiter.finalize_failure_with_retry(&request_id).await;
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
                let _ = rate_limiter.finalize_failure_with_retry(&request_id).await;
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
                let _ = billing
                    .refund_with_retry(&request_id, token_info.token_id, pre_consumed)
                    .await;
                channel_svc.record_relay_failure_async(
                    channel.channel_id,
                    channel.account_id,
                    start.elapsed().as_millis() as i64,
                    0,
                    format!("failed to build upstream embeddings request: {error}"),
                );
                route_plan.exclude_selected_channel(&channel);
                if attempt == max_retries - 1 {
                    let _ = rate_limiter.finalize_failure_with_retry(&request_id).await;
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
                let content_type = resp.headers().get(CONTENT_TYPE).cloned();
                let body = match resp.bytes().await {
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
                            format!("failed to read upstream embeddings response: {error}"),
                        );
                        route_plan.exclude_selected_account(&channel);
                        if attempt == max_retries - 1 {
                            let _ = rate_limiter.finalize_failure_with_retry(&request_id).await;
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
                        let _ = rate_limiter.finalize_failure_with_retry(&request_id).await;
                        return Err(OpenAiErrorResponse::unsupported_endpoint(message));
                    }
                    continue;
                }
                let parsed: EmbeddingResponse = match serde_json::from_slice(&body) {
                    Ok(parsed) => parsed,
                    Err(error) => {
                        let _ = billing
                            .refund_with_retry(&request_id, token_info.token_id, pre_consumed)
                            .await;
                        channel_svc.record_relay_failure_async(
                            channel.channel_id,
                            channel.account_id,
                            elapsed,
                            0,
                            format!("failed to parse upstream embeddings response: {error}"),
                        );
                        route_plan.exclude_selected_account(&channel);
                        if attempt == max_retries - 1 {
                            let _ = rate_limiter.finalize_failure_with_retry(&request_id).await;
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
                    upstream_request_id,
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

                return Ok(build_json_bytes_response(body, content_type, &request_id));
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
                    let _ = rate_limiter.finalize_failure_with_retry(&request_id).await;
                    return Err(OpenAiErrorResponse::internal_with(
                        "failed to send upstream embeddings request",
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

pub(crate) fn classify_upstream_provider_failure(
    channel_type: i16,
    status: StatusCode,
    headers: &HeaderMap,
    body: &[u8],
) -> UpstreamProviderFailure {
    let info = get_adapter(channel_type).parse_error(status, headers, body);
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
        status: normalized_status,
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::router::test_support::TestHarness;
    use summer_web::axum::{
        Router,
        body::{Body, to_bytes},
        extract::{Request, State},
        http::Method,
        http::header::CONTENT_TYPE,
        response::IntoResponse,
    };
    use tokio::sync::oneshot;

    #[derive(Clone)]
    struct MockUpstreamSpec {
        expected_path_and_query: String,
        expected_header_name: String,
        expected_header_value: String,
        expected_body_substring: Option<String>,
        response_status: StatusCode,
        response_content_type: String,
        response_body: String,
    }

    struct MockUpstreamServer {
        base_url: String,
        shutdown_tx: Option<oneshot::Sender<()>>,
        _task: tokio::task::JoinHandle<()>,
    }

    impl Drop for MockUpstreamServer {
        fn drop(&mut self) {
            if let Some(shutdown_tx) = self.shutdown_tx.take() {
                let _ = shutdown_tx.send(());
            }
        }
    }

    async fn mock_upstream_handler(
        State(spec): State<Arc<MockUpstreamSpec>>,
        req: Request,
    ) -> summer_web::axum::response::Response {
        let path_and_query = req
            .uri()
            .path_and_query()
            .map(|value| value.as_str().to_string())
            .unwrap_or_else(|| req.uri().path().to_string());
        if path_and_query != spec.expected_path_and_query {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("unexpected path: {path_and_query}"),
            )
                .into_response();
        }

        let header_value = req
            .headers()
            .get(&spec.expected_header_name)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();
        if header_value != spec.expected_header_value {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!(
                    "unexpected header {}: {}",
                    spec.expected_header_name, header_value
                ),
            )
                .into_response();
        }

        if let Some(expected_body_substring) = spec.expected_body_substring.as_ref() {
            let body = to_bytes(req.into_body(), usize::MAX)
                .await
                .expect("request body");
            let body = String::from_utf8_lossy(&body);
            if !body.contains(expected_body_substring) {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("unexpected body: {body}"),
                )
                    .into_response();
            }
        }

        summer_web::axum::http::Response::builder()
            .status(spec.response_status)
            .header(CONTENT_TYPE, spec.response_content_type.as_str())
            .body(Body::from(spec.response_body.clone()))
            .expect("mock upstream response")
    }

    async fn spawn_mock_upstream(spec: MockUpstreamSpec) -> MockUpstreamServer {
        let spec = Arc::new(spec);
        let router = Router::new()
            .fallback(mock_upstream_handler)
            .with_state(spec);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock upstream");
        let addr = listener.local_addr().expect("local addr");
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let task = tokio::spawn(async move {
            let _ = summer_web::axum::serve(listener, router)
                .with_graceful_shutdown(async move {
                    let _ = shutdown_rx.await;
                })
                .await;
        });

        MockUpstreamServer {
            base_url: format!("http://{addr}"),
            shutdown_tx: Some(shutdown_tx),
            _task: task,
        }
    }

    fn sample_mock_chat_request(stream: bool) -> ChatCompletionRequest {
        serde_json::from_value(serde_json::json!({
            "model": "gpt-5.4 xhigh",
            "messages": [{"role": "user", "content": "Hello"}],
            "stream": stream
        }))
        .expect("sample chat request")
    }

    fn sample_mock_responses_request(stream: bool) -> ResponsesRequest {
        serde_json::from_value(serde_json::json!({
            "model": "gpt-5.4 xhigh",
            "input": "Hello",
            "stream": stream
        }))
        .expect("sample responses request")
    }

    fn sample_mock_embeddings_request() -> EmbeddingRequest {
        serde_json::from_value(serde_json::json!({
            "model": "text-embedding-3-large",
            "input": "hello"
        }))
        .expect("sample embeddings request")
    }

    async fn send_mock_chat_request(
        channel_type: i16,
        api_key: &str,
        req: &ChatCompletionRequest,
        actual_model: &str,
        spec: MockUpstreamSpec,
    ) -> (MockUpstreamServer, reqwest::Response) {
        let server = spawn_mock_upstream(spec).await;
        let client = reqwest::Client::new();
        let request_builder = get_adapter(channel_type)
            .build_request(&client, &server.base_url, api_key, req, actual_model)
            .expect("build request");
        let response = request_builder.send().await.expect("send request");
        (server, response)
    }

    async fn send_mock_responses_request(
        channel_type: i16,
        api_key: &str,
        req: &ResponsesRequest,
        actual_model: &str,
        spec: MockUpstreamSpec,
    ) -> (MockUpstreamServer, reqwest::Response) {
        let server = spawn_mock_upstream(spec).await;
        let client = reqwest::Client::new();
        let raw_request = serde_json::to_value(req).expect("responses request json");
        let request_builder = get_adapter(channel_type)
            .build_responses_request(
                &client,
                &server.base_url,
                api_key,
                &raw_request,
                actual_model,
            )
            .expect("build responses request");
        let response = request_builder
            .send()
            .await
            .expect("send responses request");
        (server, response)
    }

    async fn send_mock_embeddings_request(
        channel_type: i16,
        api_key: &str,
        req: &EmbeddingRequest,
        actual_model: &str,
        spec: MockUpstreamSpec,
    ) -> (MockUpstreamServer, reqwest::Response) {
        let server = spawn_mock_upstream(spec).await;
        let client = reqwest::Client::new();
        let raw_request = serde_json::to_value(req).expect("embeddings request json");
        let request_builder = get_adapter(channel_type)
            .build_embeddings_request(
                &client,
                &server.base_url,
                api_key,
                &raw_request,
                actual_model,
            )
            .expect("build embeddings request");
        let response = request_builder
            .send()
            .await
            .expect("send embeddings request");
        (server, response)
    }

    #[tokio::test]
    async fn anthropic_chat_non_stream_mock_upstream_success() {
        let req = sample_mock_chat_request(false);
        let actual_model = "claude-3-5-sonnet-20241022";
        let (_server, response) = send_mock_chat_request(
            3,
            "sk-ant-test",
            &req,
            actual_model,
            MockUpstreamSpec {
                expected_path_and_query: "/v1/messages".into(),
                expected_header_name: "x-api-key".into(),
                expected_header_value: "sk-ant-test".into(),
                expected_body_substring: Some(format!("\"model\":\"{actual_model}\"")),
                response_status: StatusCode::OK,
                response_content_type: "application/json".into(),
                response_body: serde_json::json!({
                    "id": "msg_123",
                    "model": actual_model,
                    "content": [{"type": "text", "text": "Hello from Claude"}],
                    "stop_reason": "end_turn",
                    "usage": {
                        "input_tokens": 12,
                        "output_tokens": 7
                    }
                })
                .to_string(),
            },
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let parsed = get_adapter(3)
            .parse_response(response.bytes().await.expect("body"), actual_model)
            .expect("parse anthropic response");
        assert_eq!(parsed.model, actual_model);
        assert_eq!(
            parsed.choices[0].message.content,
            serde_json::json!("Hello from Claude")
        );
        assert_eq!(parsed.usage.total_tokens, 19);
    }

    #[tokio::test]
    async fn anthropic_chat_stream_mock_upstream_success() {
        let req = sample_mock_chat_request(true);
        let actual_model = "claude-3-5-sonnet-20241022";
        let (_server, response) = send_mock_chat_request(
            3,
            "sk-ant-test",
            &req,
            actual_model,
            MockUpstreamSpec {
                expected_path_and_query: "/v1/messages".into(),
                expected_header_name: "x-api-key".into(),
                expected_header_value: "sk-ant-test".into(),
                expected_body_substring: Some(format!("\"model\":\"{actual_model}\"")),
                response_status: StatusCode::OK,
                response_content_type: "text/event-stream".into(),
                response_body: concat!(
                    "event: message_start\n",
                    "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_123\",\"model\":\"claude-3-5-sonnet-20241022\",\"usage\":{\"input_tokens\":12,\"output_tokens\":0}}}\n\n",
                    "event: content_block_delta\n",
                    "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}\n\n",
                    "event: message_delta\n",
                    "data: {\"type\":\"message_delta\",\"usage\":{\"input_tokens\":12,\"output_tokens\":7},\"delta\":{\"stop_reason\":\"end_turn\"}}\n\n",
                    "event: message_stop\n",
                    "data: {\"type\":\"message_stop\"}\n\n"
                )
                .into(),
            },
        )
        .await;

        let chunks: Vec<_> = get_adapter(3)
            .parse_stream(response, actual_model)
            .expect("parse stream")
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .filter_map(Result::ok)
            .collect();

        assert!(
            chunks
                .iter()
                .any(|chunk| { chunk.choices[0].delta.content.as_deref() == Some("Hello") })
        );
        let final_chunk = chunks
            .iter()
            .rfind(|chunk| chunk.choices[0].finish_reason.is_some())
            .expect("final chunk");
        assert!(matches!(
            final_chunk.choices[0].finish_reason,
            Some(summer_ai_core::types::common::FinishReason::Stop)
        ));
    }

    #[tokio::test]
    async fn anthropic_chat_mock_upstream_provider_failure() {
        let req = sample_mock_chat_request(false);
        let actual_model = "claude-3-5-sonnet-20241022";
        let (_server, response) = send_mock_chat_request(
            3,
            "sk-ant-test",
            &req,
            actual_model,
            MockUpstreamSpec {
                expected_path_and_query: "/v1/messages".into(),
                expected_header_name: "x-api-key".into(),
                expected_header_value: "sk-ant-test".into(),
                expected_body_substring: Some(format!("\"model\":\"{actual_model}\"")),
                response_status: StatusCode::TOO_MANY_REQUESTS,
                response_content_type: "application/json".into(),
                response_body:
                    r#"{"type":"error","error":{"type":"rate_limit_error","message":"slow down"}}"#
                        .to_string(),
            },
        )
        .await;

        let status = response.status();
        let headers = response.headers().clone();
        let body = response.bytes().await.expect("body");
        let failure = classify_upstream_provider_failure(3, status, &headers, &body);
        assert_eq!(failure.scope, UpstreamFailureScope::Account);
        assert_eq!(failure.error.status, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(failure.error.error.error.r#type, "rate_limit_error");
        assert_eq!(failure.error.error.error.message, "slow down");
    }

    #[tokio::test]
    async fn anthropic_chat_stream_mock_upstream_provider_failure_event() {
        let req = sample_mock_chat_request(true);
        let actual_model = "claude-3-5-sonnet-20241022";
        let (_server, response) = send_mock_chat_request(
            3,
            "sk-ant-test",
            &req,
            actual_model,
            MockUpstreamSpec {
                expected_path_and_query: "/v1/messages".into(),
                expected_header_name: "x-api-key".into(),
                expected_header_value: "sk-ant-test".into(),
                expected_body_substring: Some(format!("\"model\":\"{actual_model}\"")),
                response_status: StatusCode::OK,
                response_content_type: "text/event-stream".into(),
                response_body: concat!(
                    "event: error\n",
                    "data: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"upstream overloaded\"}}\n\n"
                )
                .into(),
            },
        )
        .await;

        let results = get_adapter(3)
            .parse_stream(response, actual_model)
            .expect("parse stream")
            .collect::<Vec<_>>()
            .await;

        let error = results
            .into_iter()
            .find_map(Result::err)
            .expect("expected anthropic stream error");
        let stream_error = error
            .downcast_ref::<summer_ai_core::provider::ProviderStreamError>()
            .expect("expected provider stream error");
        assert_eq!(stream_error.info.kind, ProviderErrorKind::Server);
        assert_eq!(stream_error.info.code, "overloaded_error");
        assert_eq!(stream_error.info.message, "upstream overloaded");
    }

    #[tokio::test]
    async fn gemini_chat_non_stream_mock_upstream_success() {
        let req = sample_mock_chat_request(false);
        let actual_model = "gemini-2.5-pro";
        let (_server, response) = send_mock_chat_request(
            24,
            "gem-key",
            &req,
            actual_model,
            MockUpstreamSpec {
                expected_path_and_query: format!("/v1beta/models/{actual_model}:generateContent"),
                expected_header_name: "x-goog-api-key".into(),
                expected_header_value: "gem-key".into(),
                expected_body_substring: Some("\"contents\"".into()),
                response_status: StatusCode::OK,
                response_content_type: "application/json".into(),
                response_body: serde_json::json!({
                    "candidates": [{
                        "content": {
                            "parts": [{"text": "Hello from Gemini"}]
                        },
                        "finishReason": "STOP"
                    }],
                    "usageMetadata": {
                        "promptTokenCount": 4,
                        "candidatesTokenCount": 6,
                        "totalTokenCount": 10
                    }
                })
                .to_string(),
            },
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let parsed = get_adapter(24)
            .parse_response(response.bytes().await.expect("body"), actual_model)
            .expect("parse gemini response");
        assert_eq!(parsed.model, actual_model);
        assert_eq!(
            parsed.choices[0].message.content,
            serde_json::json!("Hello from Gemini")
        );
        assert_eq!(parsed.usage.total_tokens, 10);
    }

    #[tokio::test]
    async fn gemini_chat_stream_mock_upstream_success() {
        let req = sample_mock_chat_request(true);
        let actual_model = "gemini-2.5-pro";
        let (_server, response) = send_mock_chat_request(
            24,
            "gem-key",
            &req,
            actual_model,
            MockUpstreamSpec {
                expected_path_and_query: format!(
                    "/v1beta/models/{actual_model}:streamGenerateContent?alt=sse"
                ),
                expected_header_name: "x-goog-api-key".into(),
                expected_header_value: "gem-key".into(),
                expected_body_substring: Some("\"contents\"".into()),
                response_status: StatusCode::OK,
                response_content_type: "text/event-stream".into(),
                response_body:
                    "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"Hello\"}]}}]}\n\n\
                     data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\" Gemini\"}]},\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":4,\"candidatesTokenCount\":6,\"totalTokenCount\":10}}\n\n"
                        .into(),
            },
        )
        .await;

        let chunks: Vec<_> = get_adapter(24)
            .parse_stream(response, actual_model)
            .expect("parse stream")
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .filter_map(Result::ok)
            .collect();

        assert!(
            chunks
                .iter()
                .any(|chunk| { chunk.choices[0].delta.content.as_deref() == Some("Hello") })
        );
        let final_chunk = chunks
            .iter()
            .rfind(|chunk| chunk.choices[0].finish_reason.is_some())
            .expect("final chunk");
        assert!(matches!(
            final_chunk.choices[0].finish_reason,
            Some(summer_ai_core::types::common::FinishReason::Stop)
        ));
        assert_eq!(
            final_chunk.usage.as_ref().map(|usage| usage.total_tokens),
            Some(10)
        );
    }

    #[tokio::test]
    async fn gemini_chat_mock_upstream_provider_failure() {
        let req = sample_mock_chat_request(false);
        let actual_model = "gemini-2.5-pro";
        let (_server, response) = send_mock_chat_request(
            24,
            "gem-key",
            &req,
            actual_model,
            MockUpstreamSpec {
                expected_path_and_query: format!("/v1beta/models/{actual_model}:generateContent"),
                expected_header_name: "x-goog-api-key".into(),
                expected_header_value: "gem-key".into(),
                expected_body_substring: Some("\"contents\"".into()),
                response_status: StatusCode::BAD_REQUEST,
                response_content_type: "application/json".into(),
                response_body:
                    r#"{"error":{"status":"INVALID_ARGUMENT","message":"bad tool schema"}}"#
                        .to_string(),
            },
        )
        .await;

        let status = response.status();
        let headers = response.headers().clone();
        let body = response.bytes().await.expect("body");
        let failure = classify_upstream_provider_failure(24, status, &headers, &body);
        assert_eq!(failure.scope, UpstreamFailureScope::Channel);
        assert_eq!(failure.error.status, StatusCode::BAD_REQUEST);
        assert_eq!(failure.error.error.error.r#type, "invalid_request_error");
        assert_eq!(failure.error.error.error.message, "bad tool schema");
    }

    #[tokio::test]
    async fn gemini_chat_stream_mock_upstream_provider_failure_event() {
        let req = sample_mock_chat_request(true);
        let actual_model = "gemini-2.5-pro";
        let (_server, response) = send_mock_chat_request(
            24,
            "gem-key",
            &req,
            actual_model,
            MockUpstreamSpec {
                expected_path_and_query: format!(
                    "/v1beta/models/{actual_model}:streamGenerateContent?alt=sse"
                ),
                expected_header_name: "x-goog-api-key".into(),
                expected_header_value: "gem-key".into(),
                expected_body_substring: Some("\"contents\"".into()),
                response_status: StatusCode::OK,
                response_content_type: "text/event-stream".into(),
                response_body: concat!(
                    "event: error\n",
                    "data: {\"error\":{\"status\":\"INVALID_ARGUMENT\",\"message\":\"bad tool schema\"}}\n\n"
                )
                .into(),
            },
        )
        .await;

        let results = get_adapter(24)
            .parse_stream(response, actual_model)
            .expect("parse stream")
            .collect::<Vec<_>>()
            .await;

        let error = results
            .into_iter()
            .find_map(Result::err)
            .expect("expected gemini stream error");
        let stream_error = error
            .downcast_ref::<summer_ai_core::provider::ProviderStreamError>()
            .expect("expected provider stream error");
        assert_eq!(stream_error.info.kind, ProviderErrorKind::InvalidRequest);
        assert_eq!(stream_error.info.code, "INVALID_ARGUMENT");
        assert_eq!(stream_error.info.message, "bad tool schema");
    }

    #[tokio::test]
    async fn responses_non_stream_mock_upstream_success() {
        let req = sample_mock_responses_request(false);
        let actual_model = "gpt-5.4-mini";
        let (_server, response) = send_mock_responses_request(
            1,
            "sk-openai-test",
            &req,
            actual_model,
            MockUpstreamSpec {
                expected_path_and_query: "/v1/responses".into(),
                expected_header_name: "authorization".into(),
                expected_header_value: "Bearer sk-openai-test".into(),
                expected_body_substring: Some(format!("\"model\":\"{actual_model}\"")),
                response_status: StatusCode::OK,
                response_content_type: "application/json".into(),
                response_body: serde_json::json!({
                    "id": "resp_123",
                    "object": "response",
                    "model": actual_model,
                    "status": "completed",
                    "usage": {
                        "input_tokens": 12,
                        "output_tokens": 7,
                        "total_tokens": 19
                    },
                    "output_text": "hello"
                })
                .to_string(),
            },
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let parsed: ResponsesResponse =
            serde_json::from_slice(&response.bytes().await.expect("body")).expect("responses json");
        assert_eq!(parsed.id, "resp_123");
        assert_eq!(parsed.model, actual_model);
        assert_eq!(
            parsed.usage.as_ref().map(|usage| usage.total_tokens),
            Some(19)
        );
    }

    #[tokio::test]
    async fn responses_stream_tracker_parses_completed_event_from_mock_upstream() {
        let req = sample_mock_responses_request(true);
        let actual_model = "gpt-5.4-mini";
        let (_server, response) = send_mock_responses_request(
            1,
            "sk-openai-test",
            &req,
            actual_model,
            MockUpstreamSpec {
                expected_path_and_query: "/v1/responses".into(),
                expected_header_name: "authorization".into(),
                expected_header_value: "Bearer sk-openai-test".into(),
                expected_body_substring: Some(format!("\"model\":\"{actual_model}\"")),
                response_status: StatusCode::OK,
                response_content_type: "text/event-stream".into(),
                response_body: concat!(
                    "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_123\",\"model\":\"gpt-5.4-mini\"}}\n\n",
                    "data: {\"type\":\"response.output_text.delta\",\"delta\":\"hel\"}\n\n",
                    "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_123\",\"model\":\"gpt-5.4-mini\",\"usage\":{\"input_tokens\":12,\"output_tokens\":7,\"total_tokens\":19}}}\n\n",
                    "data: [DONE]\n\n"
                )
                .into(),
            },
        )
        .await;

        let body = response.bytes().await.expect("body");
        let mut tracker = ResponsesStreamTracker::default();
        let start = std::time::Instant::now();
        let mut first_token_time = None;
        tracker.ingest(&body, &start, &mut first_token_time);

        assert_eq!(tracker.response_id, "resp_123");
        assert_eq!(tracker.upstream_model, actual_model);
        assert_eq!(
            tracker.usage.as_ref().map(|usage| usage.total_tokens),
            Some(19)
        );
        assert!(first_token_time.is_some());
    }

    #[tokio::test]
    async fn responses_mock_upstream_provider_failure() {
        let req = sample_mock_responses_request(false);
        let actual_model = "gpt-5.4-mini";
        let (_server, response) = send_mock_responses_request(
            1,
            "sk-openai-test",
            &req,
            actual_model,
            MockUpstreamSpec {
                expected_path_and_query: "/v1/responses".into(),
                expected_header_name: "authorization".into(),
                expected_header_value: "Bearer sk-openai-test".into(),
                expected_body_substring: Some(format!("\"model\":\"{actual_model}\"")),
                response_status: StatusCode::TOO_MANY_REQUESTS,
                response_content_type: "application/json".into(),
                response_body: r#"{"error":{"message":"slow down","type":"rate_limit_error","code":"rate_limit_error"}}"#
                    .to_string(),
            },
        )
        .await;

        let status = response.status();
        let headers = response.headers().clone();
        let body = response.bytes().await.expect("body");
        let failure = classify_upstream_provider_failure(1, status, &headers, &body);
        assert_eq!(failure.scope, UpstreamFailureScope::Account);
        assert_eq!(failure.error.status, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(failure.error.error.error.r#type, "rate_limit_error");
    }

    #[tokio::test]
    async fn embeddings_non_stream_mock_upstream_success() {
        let req = sample_mock_embeddings_request();
        let actual_model = "text-embedding-3-small";
        let (_server, response) = send_mock_embeddings_request(
            1,
            "sk-openai-test",
            &req,
            actual_model,
            MockUpstreamSpec {
                expected_path_and_query: "/v1/embeddings".into(),
                expected_header_name: "authorization".into(),
                expected_header_value: "Bearer sk-openai-test".into(),
                expected_body_substring: Some(format!("\"model\":\"{actual_model}\"")),
                response_status: StatusCode::OK,
                response_content_type: "application/json".into(),
                response_body: serde_json::json!({
                    "object": "list",
                    "data": [{
                        "object": "embedding",
                        "index": 0,
                        "embedding": [0.1, 0.2]
                    }],
                    "usage": {
                        "prompt_tokens": 8,
                        "completion_tokens": 0,
                        "total_tokens": 8
                    }
                })
                .to_string(),
            },
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let parsed: EmbeddingResponse =
            serde_json::from_slice(&response.bytes().await.expect("body"))
                .expect("embeddings json");
        assert_eq!(parsed.data.len(), 1);
        assert_eq!(parsed.usage.total_tokens, 8);
    }

    #[tokio::test]
    async fn embeddings_mock_upstream_provider_failure() {
        let req = sample_mock_embeddings_request();
        let actual_model = "text-embedding-3-small";
        let (_server, response) = send_mock_embeddings_request(
            1,
            "sk-openai-test",
            &req,
            actual_model,
            MockUpstreamSpec {
                expected_path_and_query: "/v1/embeddings".into(),
                expected_header_name: "authorization".into(),
                expected_header_value: "Bearer sk-openai-test".into(),
                expected_body_substring: Some(format!("\"model\":\"{actual_model}\"")),
                response_status: StatusCode::BAD_REQUEST,
                response_content_type: "application/json".into(),
                response_body: r#"{"error":{"message":"bad embedding input","type":"invalid_request_error","code":"invalid_request_error"}}"#
                    .to_string(),
            },
        )
        .await;

        let status = response.status();
        let headers = response.headers().clone();
        let body = response.bytes().await.expect("body");
        let failure = classify_upstream_provider_failure(1, status, &headers, &body);
        assert_eq!(failure.scope, UpstreamFailureScope::Channel);
        assert_eq!(failure.error.status, StatusCode::BAD_REQUEST);
        assert_eq!(failure.error.error.error.r#type, "invalid_request_error");
        assert_eq!(failure.error.error.error.message, "bad embedding input");
    }

    #[test]
    fn classify_anthropic_rate_limit_as_account_failure() {
        let failure = classify_upstream_provider_failure(
            3,
            StatusCode::TOO_MANY_REQUESTS,
            &HeaderMap::new(),
            br#"{"type":"error","error":{"type":"rate_limit_error","message":"slow down"}}"#,
        );

        assert_eq!(failure.scope, UpstreamFailureScope::Account);
        assert_eq!(failure.error.status, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(failure.error.error.error.r#type, "rate_limit_error");
        assert_eq!(
            failure.error.error.error.code.as_deref(),
            Some("rate_limit_error")
        );
        assert_eq!(failure.error.error.error.message, "slow down");
    }

    #[test]
    fn classify_anthropic_new_api_error_as_account_failure() {
        let failure = classify_upstream_provider_failure(
            3,
            StatusCode::INTERNAL_SERVER_ERROR,
            &HeaderMap::new(),
            br#"{"error":{"type":"new_api_error","message":"invalid claude code request"},"type":"error"}"#,
        );

        assert_eq!(failure.scope, UpstreamFailureScope::Account);
        assert_eq!(failure.error.status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(failure.error.error.error.r#type, "server_error");
        assert_eq!(
            failure.error.error.error.code.as_deref(),
            Some("new_api_error")
        );
        assert_eq!(
            failure.error.error.error.message,
            "invalid claude code request"
        );
    }

    #[test]
    fn classify_gemini_invalid_argument_as_channel_failure() {
        let failure = classify_upstream_provider_failure(
            24,
            StatusCode::BAD_REQUEST,
            &HeaderMap::new(),
            br#"{"error":{"status":"INVALID_ARGUMENT","message":"bad tool schema"}}"#,
        );

        assert_eq!(failure.scope, UpstreamFailureScope::Channel);
        assert_eq!(failure.error.status, StatusCode::BAD_REQUEST);
        assert_eq!(failure.error.error.error.r#type, "invalid_request_error");
        assert_eq!(
            failure.error.error.error.code.as_deref(),
            Some("invalid_argument")
        );
        assert_eq!(failure.error.error.error.message, "bad tool schema");
    }

    #[test]
    fn map_adapter_build_error_uses_unsupported_endpoint_contract() {
        let error = map_adapter_build_error(
            "failed to build upstream responses request",
            anyhow::anyhow!("responses endpoint is not supported"),
        );

        assert_eq!(error.status, StatusCode::BAD_GATEWAY);
        assert_eq!(error.error.error.r#type, "upstream_error");
        assert_eq!(
            error.error.error.code.as_deref(),
            Some("unsupported_endpoint")
        );
        assert_eq!(
            error.error.error.message,
            "responses endpoint is not supported"
        );
    }

    #[test]
    fn map_adapter_build_error_keeps_internal_errors_internal() {
        let error = map_adapter_build_error(
            "failed to build upstream embeddings request",
            anyhow::anyhow!("failed to sign request"),
        );

        assert_eq!(error.status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(error.error.error.r#type, "server_error");
        assert!(
            error
                .error
                .error
                .message
                .contains("failed to build upstream embeddings request")
        );
    }

    #[test]
    fn extract_upstream_request_id_supports_oneapi_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-oneapi-request-id",
            HeaderValue::from_static("2026032622051868099140Z3FLl6h8"),
        );

        assert_eq!(
            extract_upstream_request_id(&headers),
            "2026032622051868099140Z3FLl6h8"
        );
    }

    #[tokio::test]
    #[ignore = "requires local postgres and redis"]
    async fn list_models_returns_fixture_models_for_token_group() {
        let harness =
            TestHarness::responses_affinity_fixture("http://127.0.0.1:9", "http://127.0.0.1:10")
                .await;

        let response = harness
            .empty_request(Method::GET, "/v1/models", "list-models")
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        let payload = crate::router::test_support::response_json(response).await;

        assert_eq!(payload["object"], "list");
        assert_eq!(payload["data"].as_array().map(Vec::len), Some(1));
        assert_eq!(payload["data"][0]["id"], harness.model_name);

        harness.cleanup().await;
    }

    #[tokio::test]
    #[ignore = "requires local postgres and redis"]
    async fn retrieve_model_returns_not_found_for_unknown_fixture_model() {
        let harness =
            TestHarness::responses_affinity_fixture("http://127.0.0.1:9", "http://127.0.0.1:10")
                .await;

        let response = harness
            .empty_request(
                Method::GET,
                "/v1/models/missing-test-model",
                "retrieve-model-missing",
            )
            .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let payload = crate::router::test_support::response_json(response).await;
        assert_eq!(payload["error"]["code"], "not_found");

        harness.cleanup().await;
    }
}
