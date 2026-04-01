use anyhow::anyhow;
use summer_ai_core::types::error::{OpenAiApiResult, OpenAiErrorResponse};
use summer_ai_model::entity::channel::ChannelType;
use summer_common::extractor::{ClientIp, Multipart};
use summer_common::user_agent::UserAgentInfo;
use summer_web::axum::http::{HeaderMap, HeaderValue, header::CONTENT_TYPE};
use summer_web::axum::response::Response;
use summer_web::extractor::Component;
use summer_web::post_api;

use crate::auth::extractor::AiToken;
use crate::relay::billing::BillingEngine;
use crate::relay::channel_router::{ChannelRouter, RouteSelectionExclusions};
use crate::relay::http_client::UpstreamHttpClient;
use crate::relay::rate_limit::RateLimitEngine;
use crate::router::openai_passthrough::{apply_upstream_auth, build_upstream_url};
use crate::service::channel::ChannelService;
use crate::service::log::LogService;
use crate::service::runtime_ops::RuntimeOpsService;
use crate::service::token::TokenService;

use super::{
    apply_upstream_failure_scope, buffer_multipart_fields, build_audio_transcription_form,
    build_audio_translation_form, build_json_bytes_response, classify_upstream_provider_failure,
    default_transcription_content_type, extract_request_id, extract_upstream_request_id,
    fallback_usage, insert_upstream_request_id_header, map_adapter_build_error,
    parse_audio_transcription_meta, record_terminal_failure, spawn_usage_accounting_task,
    unusable_success_response_message,
};

#[derive(Clone, Copy)]
struct AudioMultipartEndpointSpec {
    route_path: &'static str,
    upstream_path: &'static str,
    endpoint: &'static str,
    request_format: &'static str,
    build_form:
        fn(&[super::BufferedMultipartField], &str) -> anyhow::Result<reqwest::multipart::Form>,
}

/// POST /v1/audio/transcriptions
#[post_api("/v1/audio/transcriptions")]
#[allow(clippy::too_many_arguments)]
pub async fn audio_transcriptions(
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
    Multipart(mut multipart): Multipart,
) -> OpenAiApiResult<Response> {
    relay_audio_multipart_request(
        token_info,
        router_svc,
        billing,
        rate_limiter,
        http_client,
        log_svc,
        channel_svc,
        token_svc,
        runtime_ops,
        client_ip.to_string(),
        headers,
        &mut multipart,
        AudioMultipartEndpointSpec {
            route_path: "/v1/audio/transcriptions",
            upstream_path: "/v1/audio/transcriptions",
            endpoint: "audio/transcriptions",
            request_format: "openai/audio_transcriptions",
            build_form: build_audio_transcription_form,
        },
    )
    .await
}

/// POST /v1/audio/translations
#[post_api("/v1/audio/translations")]
#[allow(clippy::too_many_arguments)]
pub async fn audio_translations(
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
    Multipart(mut multipart): Multipart,
) -> OpenAiApiResult<Response> {
    relay_audio_multipart_request(
        token_info,
        router_svc,
        billing,
        rate_limiter,
        http_client,
        log_svc,
        channel_svc,
        token_svc,
        runtime_ops,
        client_ip.to_string(),
        headers,
        &mut multipart,
        AudioMultipartEndpointSpec {
            route_path: "/v1/audio/translations",
            upstream_path: "/v1/audio/translations",
            endpoint: "audio/translations",
            request_format: "openai/audio_translations",
            build_form: build_audio_translation_form,
        },
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn relay_audio_multipart_request(
    token_info: crate::service::token::TokenInfo,
    router_svc: ChannelRouter,
    billing: BillingEngine,
    rate_limiter: RateLimitEngine,
    http_client: UpstreamHttpClient,
    log_svc: LogService,
    channel_svc: ChannelService,
    token_svc: TokenService,
    runtime_ops: RuntimeOpsService,
    client_ip: String,
    headers: HeaderMap,
    multipart: &mut summer_web::axum::extract::Multipart,
    spec: AudioMultipartEndpointSpec,
) -> OpenAiApiResult<Response> {
    let request_id = extract_request_id(&headers);
    let user_agent = UserAgentInfo::from_headers(&headers).raw;
    let fields = buffer_multipart_fields(multipart).await?;
    let meta = parse_audio_transcription_meta(&fields).map_err(|error| {
        OpenAiErrorResponse::internal_with("failed to parse audio multipart metadata", error)
    })?;
    let requested_model = meta.model.clone();

    token_info
        .ensure_endpoint_allowed("audio")
        .map_err(|e| OpenAiErrorResponse::from_api_error(&e))?;
    token_info
        .ensure_model_allowed(&requested_model)
        .map_err(|e| OpenAiErrorResponse::from_api_error(&e))?;
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
    let estimated_tokens = meta.estimated_tokens;

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

        let request_builder = match build_audio_multipart_request_for_channel(
            http_client.client(),
            channel.channel_type,
            &channel.base_url,
            &channel.api_key,
            &fields,
            &actual_model,
            spec,
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
                    format!(
                        "failed to build upstream {} request: {error}",
                        spec.endpoint
                    ),
                );
                route_plan.exclude_selected_channel(&channel);
                let exhausted_after_exclusion = route_plan.clone().next().is_none();
                if attempt == max_retries - 1 || exhausted_after_exclusion {
                    record_terminal_failure(
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
                        start.elapsed().as_millis() as i64,
                        false,
                        &client_ip,
                        &user_agent,
                        0,
                        format!(
                            "failed to build upstream {} request: {error}",
                            spec.endpoint
                        ),
                    );
                    let _ = rate_limiter.finalize_failure_with_retry(&request_id).await;
                    return Err(map_adapter_build_error(
                        &format!("failed to build upstream {} request", spec.endpoint),
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
                let content_type = resp.headers().get(CONTENT_TYPE).cloned().or_else(|| {
                    Some(HeaderValue::from_static(
                        default_transcription_content_type(meta.response_format.as_deref()),
                    ))
                });
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
                            format!(
                                "failed to read upstream {} response: {error}",
                                spec.endpoint
                            ),
                        );
                        route_plan.exclude_selected_account(&channel);
                        if attempt == max_retries - 1 {
                            record_terminal_failure(
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
                                false,
                                &client_ip,
                                &user_agent,
                                0,
                                format!(
                                    "failed to read upstream {} response: {error}",
                                    spec.endpoint
                                ),
                            );
                            let _ = rate_limiter.finalize_failure_with_retry(&request_id).await;
                            return Err(OpenAiErrorResponse::internal_with(
                                &format!("failed to read upstream {} response", spec.endpoint),
                                error,
                            ));
                        }
                        continue;
                    }
                };
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
                        record_terminal_failure(
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

                let usage = fallback_usage(estimated_tokens);
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
                    spec.endpoint,
                    spec.request_format,
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
                        spec.endpoint,
                        spec.request_format,
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
                        spec.endpoint,
                        spec.request_format,
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
                        format!(
                            "failed to call upstream {} endpoint: {error}",
                            spec.endpoint
                        ),
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

fn build_audio_multipart_request_for_channel(
    client: &reqwest::Client,
    channel_type: i16,
    base_url: &str,
    api_key: &str,
    fields: &[super::BufferedMultipartField],
    actual_model: &str,
    spec: AudioMultipartEndpointSpec,
) -> anyhow::Result<reqwest::RequestBuilder> {
    match channel_type {
        value if value == ChannelType::OpenAi as i16 || value == ChannelType::Azure as i16 => {
            let form = (spec.build_form)(fields, actual_model)?;
            let builder = client.post(build_upstream_url(base_url, spec.upstream_path, None));
            Ok(apply_upstream_auth(builder, channel_type, api_key).multipart(form))
        }
        _ => Err(anyhow!(
            "{} endpoint is not supported for route {}",
            spec.endpoint,
            spec.route_path,
        )),
    }
}
