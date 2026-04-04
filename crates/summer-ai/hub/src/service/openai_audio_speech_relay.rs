use anyhow::Context;
use summer::plugin::Service;
use summer_ai_core::types::audio::AudioSpeechRequest;
use summer_ai_core::types::error::{OpenAiApiResult, OpenAiErrorResponse};
use summer_ai_model::entity::channel::ChannelType;
use summer_common::extractor::ClientIp;
use summer_common::response::Json;
use summer_common::user_agent::UserAgentInfo;
use summer_web::axum::http::{HeaderMap, header::CONTENT_TYPE};
use summer_web::axum::response::Response;
use summer_web::extractor::Component;

use crate::auth::extractor::AiToken;
use crate::relay::billing::BillingEngine;
use crate::relay::channel_router::{ChannelRouter, RouteSelectionExclusions};
use crate::relay::http_client::UpstreamHttpClient;
use crate::relay::rate_limit::RateLimitEngine;
use crate::router::openai_passthrough::{apply_upstream_auth, build_upstream_url};
use crate::service::channel::ChannelService;
use crate::service::log::LogService;
use crate::service::openai_responses_relay::{
    build_json_bytes_response, spawn_usage_accounting_task,
};
use crate::service::openai_tracking::{map_adapter_build_error, record_terminal_failure};
use crate::service::runtime_ops::RuntimeOpsService;
use crate::service::token::TokenService;

use crate::router::openai::{
    apply_upstream_failure_scope, classify_upstream_provider_failure, extract_request_id,
    extract_upstream_request_id, insert_upstream_request_id_header,
};
use crate::router::openai_passthrough::unusable_success_response_message;

#[derive(Clone, Service)]
pub struct OpenAiAudioSpeechRelayService {
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
}

impl OpenAiAudioSpeechRelayService {
    pub async fn relay(
        &self,
        token_info: crate::service::token::TokenInfo,
        client_ip: std::net::IpAddr,
        headers: HeaderMap,
        body: serde_json::Value,
    ) -> OpenAiApiResult<Response> {
        relay_impl(
            AiToken(token_info),
            Component(self.router_svc.clone()),
            Component(self.billing.clone()),
            Component(self.rate_limiter.clone()),
            Component(self.http_client.clone()),
            Component(self.log_svc.clone()),
            Component(self.channel_svc.clone()),
            Component(self.token_svc.clone()),
            Component(self.runtime_ops.clone()),
            ClientIp(client_ip),
            headers,
            Json(body),
        )
        .await
    }
}

#[allow(clippy::too_many_arguments)]
async fn relay_impl(
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
    Json(body): Json<serde_json::Value>,
) -> OpenAiApiResult<Response> {
    let request_id = extract_request_id(&headers);
    let user_agent = UserAgentInfo::from_headers(&headers).raw;
    let req: AudioSpeechRequest = serde_json::from_value(body).map_err(|error| {
        OpenAiErrorResponse::internal_with("failed to deserialize audio speech request", error)
    })?;
    let requested_model = req.model.clone();

    token_info
        .ensure_endpoint_allowed("audio")
        .map_err(|e| OpenAiErrorResponse::from_api_error(&e))?;
    token_info
        .ensure_model_allowed(&requested_model)
        .map_err(|e| OpenAiErrorResponse::from_api_error(&e))?;
    let client_ip = client_ip.to_string();
    token_svc.update_last_used_ip_async(token_info.token_id, client_ip.clone());

    let model_config = billing
        .get_model_config_for_endpoint(&requested_model, "audio")
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
            &requested_model,
            "audio",
            &route_exclusions,
        )
        .await
        .map_err(|e| OpenAiErrorResponse::internal_with("failed to build channel plan", e))?;
    let max_retries = 3;
    let start = std::time::Instant::now();
    let estimated_tokens = req.estimate_input_tokens();

    rate_limiter
        .reserve(&token_info, &request_id, i64::from(estimated_tokens))
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
            .get(&requested_model)
            .and_then(|value| value.as_str())
            .unwrap_or(&requested_model)
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

        let request_builder = match build_audio_speech_request_for_channel(
            http_client.client(),
            channel.channel_type,
            &channel.base_url,
            &channel.api_key,
            &req,
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
                    format!("failed to build upstream audio speech request: {error}"),
                );
                route_plan.exclude_selected_channel(&channel);
                let exhausted_after_exclusion = route_plan.clone().next().is_none();
                if attempt == max_retries - 1 || exhausted_after_exclusion {
                    record_terminal_failure(
                        &log_svc,
                        &token_info,
                        &channel,
                        "audio/speech",
                        "openai/audio_speech",
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
                        format!("failed to build upstream audio speech request: {error}"),
                    );
                    let _ = rate_limiter.finalize_failure_with_retry(&request_id).await;
                    return Err(map_adapter_build_error(
                        "failed to build upstream audio speech request",
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
                            format!("failed to read upstream audio speech response: {error}"),
                        );
                        route_plan.exclude_selected_account(&channel);
                        if attempt == max_retries - 1 {
                            record_terminal_failure(
                                &log_svc,
                                &token_info,
                                &channel,
                                "audio/speech",
                                "openai/audio_speech",
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
                                format!("failed to read upstream audio speech response: {error}"),
                            );
                            let _ = rate_limiter.finalize_failure_with_retry(&request_id).await;
                            return Err(OpenAiErrorResponse::internal_with(
                                "failed to read upstream audio speech response",
                                error,
                            ));
                        }
                        continue;
                    }
                };
                if let Some(message) =
                    unusable_success_response_message(status, &body_bytes, "audio/speech", false)
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
                            "audio/speech",
                            "openai/audio_speech",
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

                let usage = summer_ai_core::types::common::Usage {
                    prompt_tokens: estimated_tokens,
                    completion_tokens: 0,
                    total_tokens: estimated_tokens,
                    cached_tokens: 0,
                    reasoning_tokens: 0,
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
                    "audio/speech",
                    "openai/audio_speech",
                    elapsed,
                    0,
                    false,
                );

                let mut response = build_json_bytes_response(body_bytes, content_type, &request_id);
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
                        "audio/speech",
                        "openai/audio_speech",
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
                        "audio/speech",
                        "openai/audio_speech",
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
                        format!("failed to call upstream audio speech endpoint: {error}"),
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

fn build_audio_speech_request_for_channel(
    client: &reqwest::Client,
    channel_type: i16,
    base_url: &str,
    api_key: &str,
    req: &AudioSpeechRequest,
    actual_model: &str,
) -> anyhow::Result<reqwest::RequestBuilder> {
    match channel_type {
        value if value == ChannelType::OpenAi as i16 || value == ChannelType::Azure as i16 => {
            let mut payload =
                serde_json::to_value(req).context("failed to serialize audio speech request")?;
            payload["model"] = serde_json::Value::String(actual_model.to_string());
            let builder = client.post(build_upstream_url(base_url, "/v1/audio/speech", None));
            Ok(apply_upstream_auth(builder, channel_type, api_key).json(&payload))
        }
        _ => Err(anyhow::anyhow!("audio speech endpoint is not supported")),
    }
}
