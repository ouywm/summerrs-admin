use bytes::Bytes;
use futures::StreamExt;
use futures::stream::BoxStream;
use summer_ai_core::provider::get_adapter;
use summer_ai_core::types::chat::{
    ChatCompletionChunk, ChatCompletionRequest, ChatCompletionResponse,
};
use summer_ai_core::types::common::{Message, StreamOptions};
use summer_ai_core::types::completion::{
    CompletionChoice, CompletionChunk, CompletionChunkChoice, CompletionRequest, CompletionResponse,
};
use summer_ai_core::types::error::{OpenAiApiResult, OpenAiErrorResponse};
use summer_common::extractor::ClientIp;
use summer_common::response::Json;
use summer_common::user_agent::UserAgentInfo;
use summer_web::axum::body::Body;
use summer_web::axum::http::{
    HeaderMap, HeaderValue, StatusCode,
    header::{CACHE_CONTROL, CONTENT_TYPE},
};
use summer_web::axum::response::{IntoResponse, Response};
use summer_web::extractor::Component;
use summer_web::post_api;

use crate::auth::extractor::AiToken;
use crate::relay::billing::{
    BillingEngine, estimate_prompt_tokens, estimate_total_tokens_for_rate_limit,
};
use crate::relay::channel_router::{ChannelRouter, RouteSelectionExclusions};
use crate::relay::http_client::UpstreamHttpClient;
use crate::relay::rate_limit::RateLimitEngine;
use crate::service::channel::ChannelService;
use crate::service::log::LogService;
use crate::service::runtime_ops::RuntimeOpsService;
use crate::service::token::TokenService;

use super::{
    apply_upstream_failure_scope, classify_upstream_provider_failure, extract_request_id,
    extract_upstream_request_id, fallback_usage, insert_request_id_header,
    insert_upstream_request_id_header, map_adapter_build_error, record_terminal_failure,
    spawn_usage_accounting_task, unusable_success_response_message,
};

/// POST /v1/completions
#[post_api("/v1/completions")]
#[allow(clippy::too_many_arguments)]
pub async fn completions(
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
    Json(req): Json<CompletionRequest>,
) -> OpenAiApiResult<Response> {
    let request_id = extract_request_id(&headers);
    let user_agent = UserAgentInfo::from_headers(&headers).raw;
    token_info
        .ensure_endpoint_allowed("completions")
        .map_err(|e| OpenAiErrorResponse::from_api_error(&e))?;
    token_info
        .ensure_model_allowed(&req.model)
        .map_err(|e| OpenAiErrorResponse::from_api_error(&e))?;
    let client_ip = client_ip.to_string();
    token_svc.update_last_used_ip_async(token_info.token_id, client_ip.clone());

    let chat_req = completion_request_to_chat_request(&req).map_err(|e| {
        OpenAiErrorResponse::internal_with("failed to bridge completion request", e)
    })?;

    let model_config = billing
        .get_model_config_for_endpoint(&req.model, "completions")
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
            "completions",
            &route_exclusions,
        )
        .await
        .map_err(|e| OpenAiErrorResponse::internal_with("failed to build channel plan", e))?;
    let max_retries = 3;
    let start = std::time::Instant::now();
    let is_stream = req.stream;
    let requested_model = req.model.clone();
    let estimated_tokens = estimate_prompt_tokens(&chat_req.messages);
    let estimated_total_tokens =
        estimate_total_tokens_for_rate_limit(&chat_req.messages, chat_req.max_tokens);

    rate_limiter
        .reserve(&token_info, &request_id, estimated_total_tokens)
        .await
        .map_err(|e| OpenAiErrorResponse::from_quota_error(&e))?;

    for attempt in 0..max_retries {
        if attempt > 0 {
            runtime_ops.record_fallback_async();
        }
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
        let request_builder = match adapter.build_request(
            http_client.client(),
            &channel.base_url,
            &channel.api_key,
            &chat_req,
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
                    format!("failed to build upstream completions request: {error}"),
                );
                route_plan.exclude_selected_channel(&channel);
                if attempt == max_retries - 1 {
                    record_terminal_failure(
                        &log_svc,
                        &token_info,
                        &channel,
                        "completions",
                        "openai/completions",
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
                        format!("failed to build upstream completions request: {error}"),
                    );
                    let _ = rate_limiter.finalize_failure_with_retry(&request_id).await;
                    return Err(map_adapter_build_error(
                        "failed to build upstream completions request",
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
                                format!("failed to parse upstream completions stream: {error}"),
                            );
                            route_plan.exclude_selected_channel(&channel);
                            if attempt == max_retries - 1 {
                                record_terminal_failure(
                                    &log_svc,
                                    &token_info,
                                    &channel,
                                    "completions",
                                    "openai/completions",
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
                                    format!("failed to parse upstream completions stream: {error}"),
                                );
                                let _ = rate_limiter.finalize_failure_with_retry(&request_id).await;
                                return Err(OpenAiErrorResponse::internal_with(
                                    "failed to parse upstream completions stream",
                                    error,
                                ));
                            }
                            continue;
                        }
                    };
                    return Ok(build_completion_stream_response(
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
                    ));
                }

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
                            format!("failed to read upstream completions response: {error}"),
                        );
                        route_plan.exclude_selected_account(&channel);
                        if attempt == max_retries - 1 {
                            record_terminal_failure(
                                &log_svc,
                                &token_info,
                                &channel,
                                "completions",
                                "openai/completions",
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
                                format!("failed to read upstream completions response: {error}"),
                            );
                            let _ = rate_limiter.finalize_failure_with_retry(&request_id).await;
                            return Err(OpenAiErrorResponse::internal_with(
                                "failed to read upstream completions response",
                                error,
                            ));
                        }
                        continue;
                    }
                };
                if let Some(message) =
                    unusable_success_response_message(status, &body, "completions", false)
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
                        record_terminal_failure(
                            &log_svc,
                            &token_info,
                            &channel,
                            "completions",
                            "openai/completions",
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
                        let _ = rate_limiter.finalize_failure_with_retry(&request_id).await;
                        return Err(OpenAiErrorResponse::unsupported_endpoint(message));
                    }
                    continue;
                }
                let parsed = match adapter.parse_response(body, &actual_model) {
                    Ok(parsed) => bridge_chat_completion_to_completion(parsed),
                    Err(error) => {
                        let _ = billing
                            .refund_with_retry(&request_id, token_info.token_id, pre_consumed)
                            .await;
                        channel_svc.record_relay_failure_async(
                            channel.channel_id,
                            channel.account_id,
                            elapsed,
                            0,
                            format!("failed to parse upstream completions response: {error}"),
                        );
                        route_plan.exclude_selected_account(&channel);
                        if attempt == max_retries - 1 {
                            record_terminal_failure(
                                &log_svc,
                                &token_info,
                                &channel,
                                "completions",
                                "openai/completions",
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
                                format!("failed to parse upstream completions response: {error}"),
                            );
                            let _ = rate_limiter.finalize_failure_with_retry(&request_id).await;
                            return Err(OpenAiErrorResponse::internal_with(
                                "failed to parse upstream completions response",
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
                    "completions",
                    "openai/completions",
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
                    record_terminal_failure(
                        &log_svc,
                        &token_info,
                        &channel,
                        "completions",
                        "openai/completions",
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
                    record_terminal_failure(
                        &log_svc,
                        &token_info,
                        &channel,
                        "completions",
                        "openai/completions",
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
                        format!("failed to call upstream completions endpoint: {error}"),
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

pub(crate) fn completion_request_to_chat_request(
    req: &CompletionRequest,
) -> anyhow::Result<ChatCompletionRequest> {
    Ok(ChatCompletionRequest {
        model: req.model.clone(),
        messages: vec![Message {
            role: "user".into(),
            content: normalize_completion_prompt(&req.prompt),
            name: None,
            tool_calls: None,
            tool_call_id: None,
        }],
        stream: req.stream,
        temperature: req.temperature,
        max_tokens: req.max_tokens,
        top_p: req.top_p,
        frequency_penalty: req.frequency_penalty,
        presence_penalty: req.presence_penalty,
        stop: req.stop.clone(),
        tools: None,
        tool_choice: None,
        response_format: None,
        stream_options: completion_stream_options(req),
        extra: req.extra.clone(),
    })
}

pub(crate) fn bridge_chat_completion_to_completion(
    response: ChatCompletionResponse,
) -> CompletionResponse {
    CompletionResponse {
        id: response.id,
        object: "text_completion".into(),
        created: response.created,
        model: response.model,
        choices: response
            .choices
            .into_iter()
            .map(|choice| CompletionChoice {
                text: completion_text_from_message(&choice.message),
                index: choice.index,
                finish_reason: choice.finish_reason,
            })
            .collect(),
        usage: response.usage,
    }
}

fn bridge_chat_completion_chunk_to_completion(chunk: ChatCompletionChunk) -> CompletionChunk {
    CompletionChunk {
        id: chunk.id,
        object: "text_completion".into(),
        created: chunk.created,
        model: chunk.model,
        choices: chunk
            .choices
            .into_iter()
            .map(|choice| CompletionChunkChoice {
                text: choice.delta.content.unwrap_or_default(),
                index: choice.index,
                finish_reason: choice.finish_reason,
            })
            .collect(),
        usage: chunk.usage,
    }
}

fn normalize_completion_prompt(prompt: &serde_json::Value) -> serde_json::Value {
    match prompt {
        serde_json::Value::Null => serde_json::Value::String(String::new()),
        serde_json::Value::String(text) => serde_json::Value::String(text.clone()),
        serde_json::Value::Array(items) if items.iter().all(serde_json::Value::is_string) => {
            serde_json::Value::String(
                items
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .collect::<Vec<_>>()
                    .join("\n"),
            )
        }
        other => serde_json::Value::String(other.to_string()),
    }
}

fn completion_stream_options(req: &CompletionRequest) -> Option<StreamOptions> {
    if !req.stream {
        return None;
    }

    let mut options = req.stream_options.clone().unwrap_or(StreamOptions {
        include_usage: None,
    });
    if options.include_usage.is_none() {
        options.include_usage = Some(true);
    }
    Some(options)
}

fn completion_text_from_message(message: &Message) -> String {
    match &message.content {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Array(items) => items
            .iter()
            .filter_map(|item| item.get("text").and_then(serde_json::Value::as_str))
            .collect::<Vec<_>>()
            .join(""),
        other => other.to_string(),
    }
}

fn completion_sse_bytes(payload: &CompletionChunk) -> Option<Bytes> {
    serde_json::to_string(payload)
        .ok()
        .map(|json| Bytes::from(format!("data: {json}\n\n")))
}

#[allow(clippy::too_many_arguments)]
fn build_completion_stream_response(
    upstream: BoxStream<'static, anyhow::Result<ChatCompletionChunk>>,
    token_info: crate::service::token::TokenInfo,
    pre_consumed: i64,
    model_config: crate::relay::billing::ModelConfigInfo,
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
) -> Response {
    let response_request_id = request_id.clone();
    let response_upstream_request_id = upstream_request_id.clone();

    let stream = async_stream::stream! {
        let start = std::time::Instant::now();
        let mut upstream = upstream;
        let mut first_token_time = None;
        let mut usage = None;
        let mut saw_terminal_finish_reason = false;
        let mut upstream_model = String::new();
        let mut stream_error = None;

        while let Some(item) = upstream.next().await {
            match item {
                Ok(chunk) => {
                    if first_token_time.is_none()
                        && chunk
                            .choices
                            .iter()
                            .any(|choice| choice.delta.content.as_ref().is_some_and(|text| !text.is_empty()))
                    {
                        first_token_time = Some(start.elapsed().as_millis() as i64);
                    }
                    if chunk
                        .choices
                        .iter()
                        .any(|choice| choice.finish_reason.is_some())
                    {
                        saw_terminal_finish_reason = true;
                    }
                    if let Some(chunk_usage) = chunk.usage.clone() {
                        usage = Some(chunk_usage);
                    }
                    if upstream_model.is_empty() && !chunk.model.is_empty() {
                        upstream_model = chunk.model.clone();
                    }
                    if let Some(bytes) =
                        completion_sse_bytes(&bridge_chat_completion_chunk_to_completion(chunk))
                    {
                        yield Ok::<Bytes, std::convert::Infallible>(bytes);
                    }
                }
                Err(error) => {
                    tracing::error!("completions stream read error: {error}");
                    stream_error = Some(error.to_string());
                    break;
                }
            }
        }

        yield Ok(Bytes::from_static(b"data: [DONE]\n\n"));

        let total_elapsed = start_elapsed + start.elapsed().as_millis() as i64;
        if let Some(usage) = usage.filter(|_| saw_terminal_finish_reason && stream_error.is_none()) {
            let completed_model = if upstream_model.is_empty() {
                requested_model.clone()
            } else {
                upstream_model
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
                completed_model,
                client_ip,
                user_agent,
                "completions",
                "openai/completions",
                total_elapsed,
                first_token_time.unwrap_or(0) as i32,
                true,
            );
        } else {
            let fallback_reason = stream_error.unwrap_or_else(|| {
                if saw_terminal_finish_reason {
                    "completion stream ended without usage".into()
                } else {
                    "completion stream ended before terminal finish_reason".into()
                }
            });
            billing.refund_later(request_id.clone(), token_info.token_id, pre_consumed);
            let rl = rate_limiter.clone();
            let request_id_for_task = request_id.clone();
            tokio::spawn(async move {
                if let Err(error) = rl.finalize_failure_with_retry(&request_id_for_task).await {
                    tracing::warn!("failed to finalize completions rate limit failure: {error}");
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
        .expect("completion stream response");
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
