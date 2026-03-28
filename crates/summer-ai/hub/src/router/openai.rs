use summer_web::axum::body::Body;
use summer_web::axum::extract::{Path, Query};
use summer_web::axum::http;
use summer_web::axum::http::header::CONTENT_TYPE;
use summer_web::axum::response::{IntoResponse, Response};
use summer_web::extractor::Component;
use summer_web::{delete_api, get_api, post_api};

use summer_ai_core::provider::get_adapter;
use summer_ai_core::types::audio::AudioSpeechRequest;
use summer_ai_core::types::chat::ChatCompletionRequest;
use summer_ai_core::types::common::Usage;
use summer_ai_core::types::embedding::{EmbeddingsRequest, EmbeddingsResponse};
use summer_ai_core::types::error::{OpenAiApiResult, OpenAiErrorResponse};
use summer_ai_core::types::file::{FileDeleteResponse, FileListResponse, FileObject};
use summer_ai_core::types::image::{ImageGenerationRequest, ImageGenerationResponse};
use summer_ai_core::types::model::ModelListResponse;
use summer_ai_core::types::moderation::{ModerationRequest, ModerationResponse};
use summer_ai_core::types::responses::{ResponsesRequest, ResponsesResponse};

use crate::auth::extractor::AiToken;
use crate::relay::billing::{BillingEngine, estimate_prompt_tokens};
use crate::relay::channel_router::ChannelRouter;
use crate::relay::http_client::UpstreamHttpClient;
use crate::relay::stream::{build_responses_sse_response, build_sse_response};
use crate::service::log::{
    ChatCompletionLogRecord, EmbeddingLogRecord, EndpointUsageLogRecord, LogService,
};
use crate::service::model::ModelService;
use crate::service::token::TokenService;
use summer_common::extractor::ClientIp;
use summer_common::extractor::Multipart;
use summer_common::response::Json;

/// POST /v1/chat/completions
#[post_api("/v1/chat/completions")]
#[allow(clippy::too_many_arguments)]
pub async fn chat_completions(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(billing): Component<BillingEngine>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(log_svc): Component<LogService>,
    Component(token_svc): Component<TokenService>,
    ClientIp(client_ip): ClientIp,
    Json(req): Json<ChatCompletionRequest>,
) -> OpenAiApiResult<Response> {
    token_info
        .ensure_model_allowed(&req.model)
        .map_err(|e| OpenAiErrorResponse::from_api_error(&e))?;
    let client_ip = client_ip.to_string();
    token_svc.update_last_used_ip_async(token_info.token_id, client_ip.clone());

    let model_config = billing
        .get_model_config(&req.model)
        .await
        .map_err(|e| OpenAiErrorResponse::internal_with("failed to load model pricing", e))?;
    let group_ratio = billing
        .get_group_ratio(&token_info.group)
        .await
        .map_err(|e| OpenAiErrorResponse::internal_with("failed to load group pricing", e))?;

    let mut exclude: Vec<i64> = Vec::new();
    let max_retries = 3;
    let start = std::time::Instant::now();
    let is_stream = req.stream;
    let requested_model = req.model.clone();

    for attempt in 0..max_retries {
        let channel = router_svc
            .select_channel(&token_info.group, &req.model, "chat", &exclude)
            .await
            .map_err(|e| OpenAiErrorResponse::internal_with("failed to select channel", e))?
            .ok_or_else(|| OpenAiErrorResponse::no_available_channel("no available channel"))?;

        let actual_model = channel
            .model_mapping
            .get(&req.model)
            .and_then(|v| v.as_str())
            .unwrap_or(&req.model)
            .to_string();

        let estimated_tokens = estimate_prompt_tokens(&req.messages);
        let pre_consumed = billing
            .pre_consume(
                token_info.token_id,
                estimated_tokens,
                model_config.input_ratio,
                group_ratio,
            )
            .await
            .map_err(|e| OpenAiErrorResponse::from_quota_error(&e))?;

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
                let _ = billing.refund(token_info.token_id, pre_consumed).await;
                router_svc.record_failure_async(
                    &channel,
                    "build_request_error",
                    format!("failed to build upstream request: {e}"),
                );
                exclude.push(channel.channel_id);
                tracing::warn!(
                    "failed to build upstream request: {e}, channel_id={}",
                    channel.channel_id
                );
                if attempt == max_retries - 1 {
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
                let elapsed = start.elapsed().as_millis() as i64;

                if is_stream {
                    let stream = match adapter.parse_stream(resp, &actual_model) {
                        Ok(stream) => stream,
                        Err(error) => {
                            let _ = billing.refund(token_info.token_id, pre_consumed).await;
                            router_svc.record_failure_async(
                                &channel,
                                "parse_stream_error",
                                format!("failed to parse upstream stream: {error}"),
                            );
                            exclude.push(channel.channel_id);
                            if attempt == max_retries - 1 {
                                return Err(OpenAiErrorResponse::internal_with(
                                    "failed to parse upstream stream",
                                    error,
                                ));
                            }
                            continue;
                        }
                    };
                    router_svc.record_success_async(&channel, elapsed as i32);
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
                        billing,
                    ));
                } else {
                    let body = match resp.bytes().await {
                        Ok(body) => body,
                        Err(error) => {
                            let _ = billing.refund(token_info.token_id, pre_consumed).await;
                            router_svc.record_failure_async(
                                &channel,
                                "read_response_error",
                                format!("failed to read upstream response: {error}"),
                            );
                            exclude.push(channel.channel_id);
                            if attempt == max_retries - 1 {
                                return Err(OpenAiErrorResponse::internal_with(
                                    "failed to read upstream response",
                                    error,
                                ));
                            }
                            continue;
                        }
                    };
                    let parsed = match adapter.parse_response(body, &actual_model) {
                        Ok(parsed) => parsed,
                        Err(error) => {
                            let _ = billing.refund(token_info.token_id, pre_consumed).await;
                            router_svc.record_failure_async(
                                &channel,
                                "parse_response_error",
                                format!("failed to parse upstream response: {error}"),
                            );
                            exclude.push(channel.channel_id);
                            if attempt == max_retries - 1 {
                                return Err(OpenAiErrorResponse::internal_with(
                                    "failed to parse upstream response",
                                    error,
                                ));
                            }
                            continue;
                        }
                    };
                    router_svc.record_success_async(&channel, elapsed as i32);

                    let usage = parsed.usage.clone();
                    let upstream_model = parsed.model.clone();
                    let ti = token_info.clone();
                    let mc = model_config.clone();
                    let ch = channel.clone();
                    let rm = requested_model.clone();
                    let ip = client_ip.clone();
                    let bl = billing.clone();
                    let ls = log_svc.clone();
                    tokio::spawn(async move {
                        let logged_quota =
                            BillingEngine::calculate_actual_quota(&usage, &mc, group_ratio);
                        let actual_quota = match bl
                            .post_consume(&ti, pre_consumed, &usage, &mc, group_ratio)
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
                                endpoint: "chat/completions".into(),
                                requested_model: rm,
                                upstream_model,
                                model_name: mc.model_name,
                                quota: actual_quota,
                                elapsed_time: elapsed as i32,
                                first_token_time: 0,
                                is_stream: false,
                                client_ip: ip,
                            },
                        );
                    });

                    return Ok(Json(parsed).into_response());
                }
            }
            Ok(resp) => {
                let _ = billing.refund(token_info.token_id, pre_consumed).await;
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                router_svc.record_failure_async(
                    &channel,
                    status.as_u16().to_string(),
                    format!("upstream HTTP {} {}", status.as_u16(), body),
                );
                exclude.push(channel.channel_id);
                if attempt == max_retries - 1 {
                    return Err(OpenAiErrorResponse::no_available_channel(
                        "all channels failed",
                    ));
                }
            }
            Err(error) => {
                let _ = billing.refund(token_info.token_id, pre_consumed).await;
                router_svc.record_failure_async(
                    &channel,
                    "request_error",
                    format!("failed to send upstream request: {error}"),
                );
                exclude.push(channel.channel_id);
                if attempt == max_retries - 1 {
                    return Err(OpenAiErrorResponse::no_available_channel(
                        "all channels failed",
                    ));
                }
            }
        }
    }

    Err(OpenAiErrorResponse::no_available_channel(
        "all channels failed",
    ))
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

/// POST /v1/responses
#[post_api("/v1/responses")]
#[allow(clippy::too_many_arguments)]
pub async fn responses(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(billing): Component<BillingEngine>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(log_svc): Component<LogService>,
    Component(token_svc): Component<TokenService>,
    ClientIp(client_ip): ClientIp,
    Json(req): Json<ResponsesRequest>,
) -> OpenAiApiResult<Response> {
    token_info
        .ensure_model_allowed(&req.model)
        .map_err(|e| OpenAiErrorResponse::from_api_error(&e))?;

    let chat_req = req.to_chat_completion_request().map_err(|e| {
        OpenAiErrorResponse::invalid_request(format!("invalid responses request: {e}"))
    })?;

    let client_ip = client_ip.to_string();
    token_svc.update_last_used_ip_async(token_info.token_id, client_ip.clone());

    let model_config = billing
        .get_model_config(&req.model)
        .await
        .map_err(|e| OpenAiErrorResponse::internal_with("failed to load model pricing", e))?;
    let group_ratio = billing
        .get_group_ratio(&token_info.group)
        .await
        .map_err(|e| OpenAiErrorResponse::internal_with("failed to load group pricing", e))?;

    let mut exclude: Vec<i64> = Vec::new();
    let max_retries = 3;
    let start = std::time::Instant::now();
    let requested_model = req.model.clone();

    for attempt in 0..max_retries {
        let channel = select_channel_by_scopes(
            &router_svc,
            &token_info.group,
            &req.model,
            &["responses", "chat"],
            &exclude,
        )
        .await?
        .ok_or_else(|| OpenAiErrorResponse::no_available_channel("no available channel"))?;

        let actual_model = channel
            .model_mapping
            .get(&req.model)
            .and_then(|value| value.as_str())
            .unwrap_or(&req.model)
            .to_string();

        let estimated_tokens = estimate_prompt_tokens(&chat_req.messages);
        let pre_consumed = billing
            .pre_consume(
                token_info.token_id,
                estimated_tokens,
                model_config.input_ratio,
                group_ratio,
            )
            .await
            .map_err(|e| OpenAiErrorResponse::from_quota_error(&e))?;

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
                let _ = billing.refund(token_info.token_id, pre_consumed).await;
                router_svc.record_failure_async(
                    &channel,
                    "build_request_error",
                    format!("failed to build responses upstream request: {error}"),
                );
                exclude.push(channel.channel_id);
                if attempt == max_retries - 1 {
                    return Err(OpenAiErrorResponse::internal_with(
                        "failed to build responses upstream request",
                        error,
                    ));
                }
                continue;
            }
        };

        match request_builder.send().await {
            Ok(resp) if resp.status().is_success() => {
                let elapsed = start.elapsed().as_millis() as i64;
                if chat_req.stream {
                    let stream = match adapter.parse_stream(resp, &actual_model) {
                        Ok(stream) => stream,
                        Err(error) => {
                            let _ = billing.refund(token_info.token_id, pre_consumed).await;
                            router_svc.record_failure_async(
                                &channel,
                                "parse_stream_error",
                                format!("failed to parse responses upstream stream: {error}"),
                            );
                            exclude.push(channel.channel_id);
                            if attempt == max_retries - 1 {
                                return Err(OpenAiErrorResponse::internal_with(
                                    "failed to parse responses upstream stream",
                                    error,
                                ));
                            }
                            continue;
                        }
                    };
                    router_svc.record_success_async(&channel, elapsed as i32);
                    return Ok(build_responses_sse_response(
                        stream,
                        req.clone(),
                        token_info,
                        pre_consumed,
                        model_config,
                        group_ratio,
                        channel,
                        requested_model,
                        elapsed,
                        client_ip,
                        log_svc,
                        billing,
                    ));
                } else {
                    let body = match resp.bytes().await {
                        Ok(body) => body,
                        Err(error) => {
                            let _ = billing.refund(token_info.token_id, pre_consumed).await;
                            router_svc.record_failure_async(
                                &channel,
                                "read_response_error",
                                format!("failed to read responses upstream response: {error}"),
                            );
                            exclude.push(channel.channel_id);
                            if attempt == max_retries - 1 {
                                return Err(OpenAiErrorResponse::internal_with(
                                    "failed to read responses upstream response",
                                    error,
                                ));
                            }
                            continue;
                        }
                    };
                    let parsed = match adapter.parse_response(body, &actual_model) {
                        Ok(parsed) => parsed,
                        Err(error) => {
                            let _ = billing.refund(token_info.token_id, pre_consumed).await;
                            router_svc.record_failure_async(
                                &channel,
                                "parse_response_error",
                                format!("failed to parse responses upstream response: {error}"),
                            );
                            exclude.push(channel.channel_id);
                            if attempt == max_retries - 1 {
                                return Err(OpenAiErrorResponse::internal_with(
                                    "failed to parse responses upstream response",
                                    error,
                                ));
                            }
                            continue;
                        }
                    };
                    router_svc.record_success_async(&channel, elapsed as i32);

                    let usage = parsed.usage.clone();
                    let upstream_model = parsed.model.clone();
                    let response_payload = ResponsesResponse::from_chat_completion(&req, &parsed);
                    let ti = token_info.clone();
                    let mc = model_config.clone();
                    let ch = channel.clone();
                    let rm = requested_model.clone();
                    let ip = client_ip.clone();
                    let bl = billing.clone();
                    let ls = log_svc.clone();
                    tokio::spawn(async move {
                        let logged_quota =
                            BillingEngine::calculate_actual_quota(&usage, &mc, group_ratio);
                        let actual_quota = match bl
                            .post_consume(&ti, pre_consumed, &usage, &mc, group_ratio)
                            .await
                        {
                            Ok(quota) => quota,
                            Err(error) => {
                                tracing::error!(
                                    "failed to settle responses usage asynchronously: {error}"
                                );
                                logged_quota
                            }
                        };

                        ls.record_chat_completion_async(
                            &ti,
                            &ch,
                            &usage,
                            ChatCompletionLogRecord {
                                endpoint: "responses".into(),
                                requested_model: rm,
                                upstream_model,
                                model_name: mc.model_name,
                                quota: actual_quota,
                                elapsed_time: elapsed as i32,
                                first_token_time: 0,
                                is_stream: false,
                                client_ip: ip,
                            },
                        );
                    });

                    return Ok(Json(response_payload).into_response());
                }
            }
            Ok(resp) => {
                let _ = billing.refund(token_info.token_id, pre_consumed).await;
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                router_svc.record_failure_async(
                    &channel,
                    status.as_u16().to_string(),
                    format!("upstream HTTP {} {}", status.as_u16(), body),
                );
                exclude.push(channel.channel_id);
                if attempt == max_retries - 1 {
                    return Err(OpenAiErrorResponse::no_available_channel(
                        "all channels failed",
                    ));
                }
            }
            Err(error) => {
                let _ = billing.refund(token_info.token_id, pre_consumed).await;
                router_svc.record_failure_async(
                    &channel,
                    "request_error",
                    format!("failed to send responses upstream request: {error}"),
                );
                exclude.push(channel.channel_id);
                if attempt == max_retries - 1 {
                    return Err(OpenAiErrorResponse::no_available_channel(
                        "all channels failed",
                    ));
                }
            }
        }
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
    Component(http_client): Component<UpstreamHttpClient>,
    Component(log_svc): Component<LogService>,
    Component(token_svc): Component<TokenService>,
    ClientIp(client_ip): ClientIp,
    Json(req): Json<EmbeddingsRequest>,
) -> OpenAiApiResult<Json<EmbeddingsResponse>> {
    token_info
        .ensure_model_allowed(&req.model)
        .map_err(|e| OpenAiErrorResponse::from_api_error(&e))?;
    let client_ip = client_ip.to_string();
    token_svc.update_last_used_ip_async(token_info.token_id, client_ip.clone());

    let model_config = billing
        .get_model_config(&req.model)
        .await
        .map_err(|e| OpenAiErrorResponse::internal_with("failed to load model pricing", e))?;
    let group_ratio = billing
        .get_group_ratio(&token_info.group)
        .await
        .map_err(|e| OpenAiErrorResponse::internal_with("failed to load group pricing", e))?;

    let mut exclude: Vec<i64> = Vec::new();
    let max_retries = 3;
    let start = std::time::Instant::now();
    let requested_model = req.model.clone();

    for attempt in 0..max_retries {
        let channel = router_svc
            .select_channel(&token_info.group, &req.model, "embeddings", &exclude)
            .await
            .map_err(|e| OpenAiErrorResponse::internal_with("failed to select channel", e))?
            .ok_or_else(|| OpenAiErrorResponse::no_available_channel("no available channel"))?;

        let actual_model = channel
            .model_mapping
            .get(&req.model)
            .and_then(|value| value.as_str())
            .unwrap_or(&req.model)
            .to_string();

        let estimated_tokens = req.estimate_input_tokens();
        let pre_consumed = billing
            .pre_consume(
                token_info.token_id,
                estimated_tokens,
                model_config.input_ratio,
                group_ratio,
            )
            .await
            .map_err(|e| OpenAiErrorResponse::from_quota_error(&e))?;

        let request_builder = match build_embeddings_request(
            http_client.client(),
            &channel.base_url,
            &channel.api_key,
            &req,
            &actual_model,
        ) {
            Ok(builder) => builder,
            Err(error) => {
                let _ = billing.refund(token_info.token_id, pre_consumed).await;
                router_svc.record_failure_async(
                    &channel,
                    "build_request_error",
                    format!("failed to build embeddings request: {error}"),
                );
                exclude.push(channel.channel_id);
                if attempt == max_retries - 1 {
                    return Err(OpenAiErrorResponse::internal_with(
                        "failed to build embeddings request",
                        error,
                    ));
                }
                continue;
            }
        };

        match request_builder.send().await {
            Ok(resp) if resp.status().is_success() => {
                let elapsed = start.elapsed().as_millis() as i64;
                let body = match resp.bytes().await {
                    Ok(body) => body,
                    Err(error) => {
                        let _ = billing.refund(token_info.token_id, pre_consumed).await;
                        router_svc.record_failure_async(
                            &channel,
                            "read_response_error",
                            format!("failed to read embeddings response: {error}"),
                        );
                        exclude.push(channel.channel_id);
                        if attempt == max_retries - 1 {
                            return Err(OpenAiErrorResponse::internal_with(
                                "failed to read embeddings response",
                                error,
                            ));
                        }
                        continue;
                    }
                };
                let parsed = match serde_json::from_slice::<EmbeddingsResponse>(&body) {
                    Ok(parsed) => parsed,
                    Err(error) => {
                        let _ = billing.refund(token_info.token_id, pre_consumed).await;
                        router_svc.record_failure_async(
                            &channel,
                            "parse_response_error",
                            format!("failed to parse embeddings response: {error}"),
                        );
                        exclude.push(channel.channel_id);
                        if attempt == max_retries - 1 {
                            return Err(OpenAiErrorResponse::internal_with(
                                "failed to parse embeddings response",
                                error,
                            ));
                        }
                        continue;
                    }
                };
                router_svc.record_success_async(&channel, elapsed as i32);

                let usage = Usage {
                    prompt_tokens: parsed.usage.prompt_tokens,
                    completion_tokens: 0,
                    total_tokens: parsed.usage.total_tokens,
                    cached_tokens: 0,
                    reasoning_tokens: 0,
                };
                let upstream_model = parsed.model.clone();
                let ti = token_info.clone();
                let mc = model_config.clone();
                let ch = channel.clone();
                let rm = requested_model.clone();
                let ip = client_ip.clone();
                let bl = billing.clone();
                let ls = log_svc.clone();
                tokio::spawn(async move {
                    let logged_quota =
                        BillingEngine::calculate_actual_quota(&usage, &mc, group_ratio);
                    let actual_quota = match bl
                        .post_consume(&ti, pre_consumed, &usage, &mc, group_ratio)
                        .await
                    {
                        Ok(quota) => quota,
                        Err(error) => {
                            tracing::error!(
                                "failed to settle embeddings usage asynchronously: {error}"
                            );
                            logged_quota
                        }
                    };

                    ls.record_embedding_async(
                        &ti,
                        &ch,
                        &usage,
                        EmbeddingLogRecord {
                            requested_model: rm,
                            upstream_model,
                            model_name: mc.model_name,
                            quota: actual_quota,
                            elapsed_time: elapsed as i32,
                            client_ip: ip,
                        },
                    );
                });

                return Ok(Json(parsed));
            }
            Ok(resp) => {
                let _ = billing.refund(token_info.token_id, pre_consumed).await;
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                router_svc.record_failure_async(
                    &channel,
                    status.as_u16().to_string(),
                    format!("upstream HTTP {} {}", status.as_u16(), body),
                );
                exclude.push(channel.channel_id);
                if attempt == max_retries - 1 {
                    return Err(OpenAiErrorResponse::no_available_channel(
                        "all channels failed",
                    ));
                }
            }
            Err(error) => {
                let _ = billing.refund(token_info.token_id, pre_consumed).await;
                router_svc.record_failure_async(
                    &channel,
                    "request_error",
                    format!("failed to send embeddings request: {error}"),
                );
                exclude.push(channel.channel_id);
                if attempt == max_retries - 1 {
                    return Err(OpenAiErrorResponse::no_available_channel(
                        "all channels failed",
                    ));
                }
            }
        }
    }

    Err(OpenAiErrorResponse::no_available_channel(
        "all channels failed",
    ))
}

/// POST /v1/moderations
#[post_api("/v1/moderations")]
#[allow(clippy::too_many_arguments)]
pub async fn moderations(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(billing): Component<BillingEngine>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(log_svc): Component<LogService>,
    Component(token_svc): Component<TokenService>,
    ClientIp(client_ip): ClientIp,
    Json(req): Json<ModerationRequest>,
) -> OpenAiApiResult<Json<ModerationResponse>> {
    let requested_model = req.effective_model().to_string();
    token_info
        .ensure_model_allowed(&requested_model)
        .map_err(|e| OpenAiErrorResponse::from_api_error(&e))?;
    let client_ip = client_ip.to_string();
    token_svc.update_last_used_ip_async(token_info.token_id, client_ip.clone());

    let model_config = billing
        .get_model_config(&requested_model)
        .await
        .map_err(|e| OpenAiErrorResponse::internal_with("failed to load model pricing", e))?;
    let group_ratio = billing
        .get_group_ratio(&token_info.group)
        .await
        .map_err(|e| OpenAiErrorResponse::internal_with("failed to load group pricing", e))?;

    let estimated_tokens = req.estimate_prompt_tokens();
    let mut exclude: Vec<i64> = Vec::new();
    let max_retries = 3;
    let start = std::time::Instant::now();

    for attempt in 0..max_retries {
        let channel = select_channel_by_scopes(
            &router_svc,
            &token_info.group,
            &requested_model,
            &["moderation", "moderations"],
            &exclude,
        )
        .await?
        .ok_or_else(|| OpenAiErrorResponse::no_available_channel("no available channel"))?;

        let actual_model = channel
            .model_mapping
            .get(&requested_model)
            .and_then(|value| value.as_str())
            .unwrap_or(&requested_model)
            .to_string();

        let pre_consumed = billing
            .pre_consume(
                token_info.token_id,
                estimated_tokens,
                model_config.input_ratio,
                group_ratio,
            )
            .await
            .map_err(|e| OpenAiErrorResponse::from_quota_error(&e))?;

        let request_builder = match build_moderation_request(
            http_client.client(),
            &channel.base_url,
            &channel.api_key,
            &req,
            &actual_model,
        ) {
            Ok(builder) => builder,
            Err(error) => {
                let _ = billing.refund(token_info.token_id, pre_consumed).await;
                router_svc.record_failure_async(
                    &channel,
                    "build_request_error",
                    format!("failed to build moderation request: {error}"),
                );
                exclude.push(channel.channel_id);
                if attempt == max_retries - 1 {
                    return Err(OpenAiErrorResponse::internal_with(
                        "failed to build moderation request",
                        error,
                    ));
                }
                continue;
            }
        };

        match request_builder.send().await {
            Ok(resp) if resp.status().is_success() => {
                let elapsed = start.elapsed().as_millis() as i64;
                let body = match resp.bytes().await {
                    Ok(body) => body,
                    Err(error) => {
                        let _ = billing.refund(token_info.token_id, pre_consumed).await;
                        router_svc.record_failure_async(
                            &channel,
                            "read_response_error",
                            format!("failed to read moderation response: {error}"),
                        );
                        exclude.push(channel.channel_id);
                        if attempt == max_retries - 1 {
                            return Err(OpenAiErrorResponse::internal_with(
                                "failed to read moderation response",
                                error,
                            ));
                        }
                        continue;
                    }
                };
                let parsed = match serde_json::from_slice::<ModerationResponse>(&body) {
                    Ok(parsed) => parsed,
                    Err(error) => {
                        let _ = billing.refund(token_info.token_id, pre_consumed).await;
                        router_svc.record_failure_async(
                            &channel,
                            "parse_response_error",
                            format!("failed to parse moderation response: {error}"),
                        );
                        exclude.push(channel.channel_id);
                        if attempt == max_retries - 1 {
                            return Err(OpenAiErrorResponse::internal_with(
                                "failed to parse moderation response",
                                error,
                            ));
                        }
                        continue;
                    }
                };
                router_svc.record_success_async(&channel, elapsed as i32);

                let usage = Usage {
                    prompt_tokens: estimated_tokens,
                    completion_tokens: 0,
                    total_tokens: estimated_tokens,
                    cached_tokens: 0,
                    reasoning_tokens: 0,
                };
                let actual_quota = billing
                    .commit_reserved_quota(&token_info, pre_consumed)
                    .await
                    .map_err(|e| {
                        OpenAiErrorResponse::internal_with("failed to commit reserved quota", e)
                    })?;

                log_svc.record_endpoint_usage_async(
                    &token_info,
                    &channel,
                    &usage,
                    EndpointUsageLogRecord {
                        endpoint: "moderations".into(),
                        requested_model: requested_model.clone(),
                        upstream_model: actual_model,
                        model_name: model_config.model_name,
                        quota: actual_quota,
                        elapsed_time: elapsed as i32,
                        first_token_time: 0,
                        is_stream: false,
                        client_ip: client_ip.clone(),
                    },
                );

                return Ok(Json(parsed));
            }
            Ok(resp) => {
                let _ = billing.refund(token_info.token_id, pre_consumed).await;
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                router_svc.record_failure_async(
                    &channel,
                    status.as_u16().to_string(),
                    format!("upstream HTTP {} {}", status.as_u16(), body),
                );
                exclude.push(channel.channel_id);
                if attempt == max_retries - 1 {
                    return Err(OpenAiErrorResponse::no_available_channel(
                        "all channels failed",
                    ));
                }
            }
            Err(error) => {
                let _ = billing.refund(token_info.token_id, pre_consumed).await;
                router_svc.record_failure_async(
                    &channel,
                    "request_error",
                    format!("failed to send moderation request: {error}"),
                );
                exclude.push(channel.channel_id);
                if attempt == max_retries - 1 {
                    return Err(OpenAiErrorResponse::no_available_channel(
                        "all channels failed",
                    ));
                }
            }
        }
    }

    Err(OpenAiErrorResponse::no_available_channel(
        "all channels failed",
    ))
}

/// POST /v1/files
#[post_api("/v1/files")]
#[allow(clippy::too_many_arguments)]
pub async fn files_upload(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(token_svc): Component<TokenService>,
    ClientIp(client_ip): ClientIp,
    Multipart(mut multipart): Multipart,
) -> OpenAiApiResult<Json<FileObject>> {
    let fields = buffer_multipart_fields(&mut multipart).await?;
    let client_ip = client_ip.to_string();
    token_svc.update_last_used_ip_async(token_info.token_id, client_ip);

    let mut exclude: Vec<i64> = Vec::new();
    let max_retries = 3;
    let start = std::time::Instant::now();

    for attempt in 0..max_retries {
        let channel =
            select_channel_by_scopes(&router_svc, &token_info.group, "", &["files"], &exclude)
                .await?
                .ok_or_else(|| OpenAiErrorResponse::no_available_channel("no available channel"))?;

        let request_builder = match build_file_upload_request(
            http_client.client(),
            &channel.base_url,
            &channel.api_key,
            &fields,
        ) {
            Ok(builder) => builder,
            Err(error) => {
                router_svc.record_failure_async(
                    &channel,
                    "build_request_error",
                    format!("failed to build file upload request: {error}"),
                );
                exclude.push(channel.channel_id);
                if attempt == max_retries - 1 {
                    return Err(OpenAiErrorResponse::internal_with(
                        "failed to build file upload request",
                        error,
                    ));
                }
                continue;
            }
        };

        match request_builder.send().await {
            Ok(resp) if resp.status().is_success() => {
                let elapsed = start.elapsed().as_millis() as i64;
                let body = resp.bytes().await.map_err(|e| {
                    OpenAiErrorResponse::internal_with("failed to read file upload response", e)
                })?;
                let parsed = serde_json::from_slice::<FileObject>(&body).map_err(|e| {
                    OpenAiErrorResponse::internal_with("failed to parse file upload response", e)
                })?;
                router_svc.record_success_async(&channel, elapsed as i32);
                return Ok(Json(parsed));
            }
            Ok(resp) => {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                router_svc.record_failure_async(
                    &channel,
                    status.as_u16().to_string(),
                    format!("upstream HTTP {} {}", status.as_u16(), body),
                );
                exclude.push(channel.channel_id);
                if attempt == max_retries - 1 {
                    return Err(OpenAiErrorResponse::no_available_channel(
                        "all channels failed",
                    ));
                }
            }
            Err(error) => {
                router_svc.record_failure_async(
                    &channel,
                    "request_error",
                    format!("failed to send file upload request: {error}"),
                );
                exclude.push(channel.channel_id);
                if attempt == max_retries - 1 {
                    return Err(OpenAiErrorResponse::no_available_channel(
                        "all channels failed",
                    ));
                }
            }
        }
    }

    Err(OpenAiErrorResponse::no_available_channel(
        "all channels failed",
    ))
}

/// GET /v1/files
#[get_api("/v1/files")]
#[allow(clippy::too_many_arguments)]
pub async fn files_list(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(token_svc): Component<TokenService>,
    ClientIp(client_ip): ClientIp,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> OpenAiApiResult<Json<FileListResponse>> {
    let client_ip = client_ip.to_string();
    token_svc.update_last_used_ip_async(token_info.token_id, client_ip);

    forward_file_json_request(
        &router_svc,
        http_client.client(),
        &token_info.group,
        &query,
        http::Method::GET,
        "/v1/files",
        "files/list",
    )
    .await
}

/// GET /v1/files/{file_id}
#[get_api("/v1/files/{file_id}")]
#[allow(clippy::too_many_arguments)]
pub async fn files_get(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(token_svc): Component<TokenService>,
    ClientIp(client_ip): ClientIp,
    Path(file_id): Path<String>,
) -> OpenAiApiResult<Json<FileObject>> {
    let client_ip = client_ip.to_string();
    token_svc.update_last_used_ip_async(token_info.token_id, client_ip);

    let path = format!("/v1/files/{file_id}");
    forward_file_json_request(
        &router_svc,
        http_client.client(),
        &token_info.group,
        &std::collections::HashMap::new(),
        http::Method::GET,
        &path,
        "files/get",
    )
    .await
}

/// DELETE /v1/files/{file_id}
#[delete_api("/v1/files/{file_id}")]
#[allow(clippy::too_many_arguments)]
pub async fn files_delete(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(token_svc): Component<TokenService>,
    ClientIp(client_ip): ClientIp,
    Path(file_id): Path<String>,
) -> OpenAiApiResult<Json<FileDeleteResponse>> {
    let client_ip = client_ip.to_string();
    token_svc.update_last_used_ip_async(token_info.token_id, client_ip);

    let path = format!("/v1/files/{file_id}");
    forward_file_json_request(
        &router_svc,
        http_client.client(),
        &token_info.group,
        &std::collections::HashMap::new(),
        http::Method::DELETE,
        &path,
        "files/delete",
    )
    .await
}

/// GET /v1/files/{file_id}/content
#[get_api("/v1/files/{file_id}/content")]
#[allow(clippy::too_many_arguments)]
pub async fn files_content(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(token_svc): Component<TokenService>,
    ClientIp(client_ip): ClientIp,
    Path(file_id): Path<String>,
) -> OpenAiApiResult<Response> {
    let client_ip = client_ip.to_string();
    token_svc.update_last_used_ip_async(token_info.token_id, client_ip);

    let path = format!("/v1/files/{file_id}/content");
    let mut exclude: Vec<i64> = Vec::new();
    let max_retries = 3;
    let start = std::time::Instant::now();

    for attempt in 0..max_retries {
        let channel =
            select_channel_by_scopes(&router_svc, &token_info.group, "", &["files"], &exclude)
                .await?
                .ok_or_else(|| OpenAiErrorResponse::no_available_channel("no available channel"))?;
        let url = format!("{}{}", channel.base_url.trim_end_matches('/'), path);

        match http_client
            .client()
            .get(url)
            .bearer_auth(&channel.api_key)
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                let elapsed = start.elapsed().as_millis() as i64;
                let content_type = resp
                    .headers()
                    .get(CONTENT_TYPE)
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or("application/octet-stream")
                    .to_string();
                let body = resp.bytes().await.map_err(|e| {
                    OpenAiErrorResponse::internal_with("failed to read file content response", e)
                })?;
                router_svc.record_success_async(&channel, elapsed as i32);
                return Ok(binary_response(body, &content_type));
            }
            Ok(resp) => {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                router_svc.record_failure_async(
                    &channel,
                    status.as_u16().to_string(),
                    format!("upstream HTTP {} {}", status.as_u16(), body),
                );
                exclude.push(channel.channel_id);
                if attempt == max_retries - 1 {
                    return Err(OpenAiErrorResponse::no_available_channel(
                        "all channels failed",
                    ));
                }
            }
            Err(error) => {
                router_svc.record_failure_async(
                    &channel,
                    "request_error",
                    format!("failed to send file content request: {error}"),
                );
                exclude.push(channel.channel_id);
                if attempt == max_retries - 1 {
                    return Err(OpenAiErrorResponse::no_available_channel(
                        "all channels failed",
                    ));
                }
            }
        }
    }

    Err(OpenAiErrorResponse::no_available_channel(
        "all channels failed",
    ))
}

/// POST /v1/images/generations
#[post_api("/v1/images/generations")]
#[allow(clippy::too_many_arguments)]
pub async fn image_generations(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(billing): Component<BillingEngine>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(log_svc): Component<LogService>,
    Component(token_svc): Component<TokenService>,
    ClientIp(client_ip): ClientIp,
    Json(req): Json<ImageGenerationRequest>,
) -> OpenAiApiResult<Json<ImageGenerationResponse>> {
    token_info
        .ensure_model_allowed(&req.model)
        .map_err(|e| OpenAiErrorResponse::from_api_error(&e))?;
    let client_ip = client_ip.to_string();
    token_svc.update_last_used_ip_async(token_info.token_id, client_ip.clone());

    let model_config = billing
        .get_model_config(&req.model)
        .await
        .map_err(|e| OpenAiErrorResponse::internal_with("failed to load model pricing", e))?;
    let group_ratio = billing
        .get_group_ratio(&token_info.group)
        .await
        .map_err(|e| OpenAiErrorResponse::internal_with("failed to load group pricing", e))?;

    let mut exclude: Vec<i64> = Vec::new();
    let max_retries = 3;
    let start = std::time::Instant::now();
    let requested_model = req.model.clone();
    let estimated_tokens = req.estimate_prompt_tokens();

    for attempt in 0..max_retries {
        let channel = select_channel_by_scopes(
            &router_svc,
            &token_info.group,
            &req.model,
            &["image_generation", "images"],
            &exclude,
        )
        .await?
        .ok_or_else(|| OpenAiErrorResponse::no_available_channel("no available channel"))?;

        let actual_model = channel
            .model_mapping
            .get(&req.model)
            .and_then(|value| value.as_str())
            .unwrap_or(&req.model)
            .to_string();

        let pre_consumed = billing
            .pre_consume(
                token_info.token_id,
                estimated_tokens,
                model_config.input_ratio,
                group_ratio,
            )
            .await
            .map_err(|e| OpenAiErrorResponse::from_quota_error(&e))?;

        let request_builder = match build_image_generation_request(
            http_client.client(),
            &channel.base_url,
            &channel.api_key,
            &req,
            &actual_model,
        ) {
            Ok(builder) => builder,
            Err(error) => {
                let _ = billing.refund(token_info.token_id, pre_consumed).await;
                router_svc.record_failure_async(
                    &channel,
                    "build_request_error",
                    format!("failed to build image generation request: {error}"),
                );
                exclude.push(channel.channel_id);
                if attempt == max_retries - 1 {
                    return Err(OpenAiErrorResponse::internal_with(
                        "failed to build image generation request",
                        error,
                    ));
                }
                continue;
            }
        };

        match request_builder.send().await {
            Ok(resp) if resp.status().is_success() => {
                let elapsed = start.elapsed().as_millis() as i64;
                let body = match resp.bytes().await {
                    Ok(body) => body,
                    Err(error) => {
                        let _ = billing.refund(token_info.token_id, pre_consumed).await;
                        router_svc.record_failure_async(
                            &channel,
                            "read_response_error",
                            format!("failed to read image generation response: {error}"),
                        );
                        exclude.push(channel.channel_id);
                        if attempt == max_retries - 1 {
                            return Err(OpenAiErrorResponse::internal_with(
                                "failed to read image generation response",
                                error,
                            ));
                        }
                        continue;
                    }
                };
                let parsed = match serde_json::from_slice::<ImageGenerationResponse>(&body) {
                    Ok(parsed) => parsed,
                    Err(error) => {
                        let _ = billing.refund(token_info.token_id, pre_consumed).await;
                        router_svc.record_failure_async(
                            &channel,
                            "parse_response_error",
                            format!("failed to parse image generation response: {error}"),
                        );
                        exclude.push(channel.channel_id);
                        if attempt == max_retries - 1 {
                            return Err(OpenAiErrorResponse::internal_with(
                                "failed to parse image generation response",
                                error,
                            ));
                        }
                        continue;
                    }
                };
                router_svc.record_success_async(&channel, elapsed as i32);

                let usage = Usage {
                    prompt_tokens: estimated_tokens,
                    completion_tokens: 0,
                    total_tokens: estimated_tokens,
                    cached_tokens: 0,
                    reasoning_tokens: 0,
                };
                let actual_quota = billing
                    .commit_reserved_quota(&token_info, pre_consumed)
                    .await
                    .map_err(|e| {
                        OpenAiErrorResponse::internal_with("failed to commit reserved quota", e)
                    })?;

                log_svc.record_endpoint_usage_async(
                    &token_info,
                    &channel,
                    &usage,
                    EndpointUsageLogRecord {
                        endpoint: "images/generations".into(),
                        requested_model,
                        upstream_model: actual_model,
                        model_name: model_config.model_name,
                        quota: actual_quota,
                        elapsed_time: elapsed as i32,
                        first_token_time: 0,
                        is_stream: false,
                        client_ip: client_ip.clone(),
                    },
                );

                return Ok(Json(parsed));
            }
            Ok(resp) => {
                let _ = billing.refund(token_info.token_id, pre_consumed).await;
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                router_svc.record_failure_async(
                    &channel,
                    status.as_u16().to_string(),
                    format!("upstream HTTP {} {}", status.as_u16(), body),
                );
                exclude.push(channel.channel_id);
                if attempt == max_retries - 1 {
                    return Err(OpenAiErrorResponse::no_available_channel(
                        "all channels failed",
                    ));
                }
            }
            Err(error) => {
                let _ = billing.refund(token_info.token_id, pre_consumed).await;
                router_svc.record_failure_async(
                    &channel,
                    "request_error",
                    format!("failed to send image generation request: {error}"),
                );
                exclude.push(channel.channel_id);
                if attempt == max_retries - 1 {
                    return Err(OpenAiErrorResponse::no_available_channel(
                        "all channels failed",
                    ));
                }
            }
        }
    }

    Err(OpenAiErrorResponse::no_available_channel(
        "all channels failed",
    ))
}

/// POST /v1/images/edits
#[post_api("/v1/images/edits")]
#[allow(clippy::too_many_arguments)]
pub async fn image_edits(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(billing): Component<BillingEngine>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(log_svc): Component<LogService>,
    Component(token_svc): Component<TokenService>,
    ClientIp(client_ip): ClientIp,
    Multipart(mut multipart): Multipart,
) -> OpenAiApiResult<Json<ImageGenerationResponse>> {
    let fields = buffer_multipart_fields(&mut multipart).await?;
    let meta = parse_image_edit_meta(&fields).map_err(|e| {
        OpenAiErrorResponse::invalid_request(format!("invalid image edit form: {e}"))
    })?;

    token_info
        .ensure_model_allowed(&meta.model)
        .map_err(|e| OpenAiErrorResponse::from_api_error(&e))?;
    let client_ip = client_ip.to_string();
    token_svc.update_last_used_ip_async(token_info.token_id, client_ip.clone());

    let model_config = billing
        .get_model_config(&meta.model)
        .await
        .map_err(|e| OpenAiErrorResponse::internal_with("failed to load model pricing", e))?;
    let group_ratio = billing
        .get_group_ratio(&token_info.group)
        .await
        .map_err(|e| OpenAiErrorResponse::internal_with("failed to load group pricing", e))?;

    let mut exclude: Vec<i64> = Vec::new();
    let max_retries = 3;
    let start = std::time::Instant::now();
    let requested_model = meta.model.clone();

    for attempt in 0..max_retries {
        let channel = select_channel_by_scopes(
            &router_svc,
            &token_info.group,
            &meta.model,
            &["image_edit", "images_edit", "images"],
            &exclude,
        )
        .await?
        .ok_or_else(|| OpenAiErrorResponse::no_available_channel("no available channel"))?;

        let actual_model = channel
            .model_mapping
            .get(&meta.model)
            .and_then(|value| value.as_str())
            .unwrap_or(&meta.model)
            .to_string();

        let pre_consumed = billing
            .pre_consume(
                token_info.token_id,
                meta.estimated_tokens,
                model_config.input_ratio,
                group_ratio,
            )
            .await
            .map_err(|e| OpenAiErrorResponse::from_quota_error(&e))?;

        let form = build_image_edit_form(&fields, &actual_model).map_err(|e| {
            OpenAiErrorResponse::invalid_request(format!("invalid image edit form: {e}"))
        })?;
        let url = format!("{}/v1/images/edits", channel.base_url.trim_end_matches('/'));

        match http_client
            .client()
            .post(url)
            .bearer_auth(&channel.api_key)
            .multipart(form)
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                let elapsed = start.elapsed().as_millis() as i64;
                let body = match resp.bytes().await {
                    Ok(body) => body,
                    Err(error) => {
                        let _ = billing.refund(token_info.token_id, pre_consumed).await;
                        router_svc.record_failure_async(
                            &channel,
                            "read_response_error",
                            format!("failed to read image edit response: {error}"),
                        );
                        exclude.push(channel.channel_id);
                        if attempt == max_retries - 1 {
                            return Err(OpenAiErrorResponse::internal_with(
                                "failed to read image edit response",
                                error,
                            ));
                        }
                        continue;
                    }
                };
                let parsed = match serde_json::from_slice::<ImageGenerationResponse>(&body) {
                    Ok(parsed) => parsed,
                    Err(error) => {
                        let _ = billing.refund(token_info.token_id, pre_consumed).await;
                        router_svc.record_failure_async(
                            &channel,
                            "parse_response_error",
                            format!("failed to parse image edit response: {error}"),
                        );
                        exclude.push(channel.channel_id);
                        if attempt == max_retries - 1 {
                            return Err(OpenAiErrorResponse::internal_with(
                                "failed to parse image edit response",
                                error,
                            ));
                        }
                        continue;
                    }
                };
                router_svc.record_success_async(&channel, elapsed as i32);

                let usage = Usage {
                    prompt_tokens: meta.estimated_tokens,
                    completion_tokens: 0,
                    total_tokens: meta.estimated_tokens,
                    cached_tokens: 0,
                    reasoning_tokens: 0,
                };
                let actual_quota = billing
                    .commit_reserved_quota(&token_info, pre_consumed)
                    .await
                    .map_err(|e| {
                        OpenAiErrorResponse::internal_with("failed to commit reserved quota", e)
                    })?;

                log_svc.record_endpoint_usage_async(
                    &token_info,
                    &channel,
                    &usage,
                    EndpointUsageLogRecord {
                        endpoint: "images/edits".into(),
                        requested_model,
                        upstream_model: actual_model,
                        model_name: model_config.model_name,
                        quota: actual_quota,
                        elapsed_time: elapsed as i32,
                        first_token_time: 0,
                        is_stream: false,
                        client_ip: client_ip.clone(),
                    },
                );

                return Ok(Json(parsed));
            }
            Ok(resp) => {
                let _ = billing.refund(token_info.token_id, pre_consumed).await;
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                router_svc.record_failure_async(
                    &channel,
                    status.as_u16().to_string(),
                    format!("upstream HTTP {} {}", status.as_u16(), body),
                );
                exclude.push(channel.channel_id);
                if attempt == max_retries - 1 {
                    return Err(OpenAiErrorResponse::no_available_channel(
                        "all channels failed",
                    ));
                }
            }
            Err(error) => {
                let _ = billing.refund(token_info.token_id, pre_consumed).await;
                router_svc.record_failure_async(
                    &channel,
                    "request_error",
                    format!("failed to send image edit request: {error}"),
                );
                exclude.push(channel.channel_id);
                if attempt == max_retries - 1 {
                    return Err(OpenAiErrorResponse::no_available_channel(
                        "all channels failed",
                    ));
                }
            }
        }
    }

    Err(OpenAiErrorResponse::no_available_channel(
        "all channels failed",
    ))
}

/// POST /v1/images/variations
#[post_api("/v1/images/variations")]
#[allow(clippy::too_many_arguments)]
pub async fn image_variations(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(billing): Component<BillingEngine>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(log_svc): Component<LogService>,
    Component(token_svc): Component<TokenService>,
    ClientIp(client_ip): ClientIp,
    Multipart(mut multipart): Multipart,
) -> OpenAiApiResult<Json<ImageGenerationResponse>> {
    let fields = buffer_multipart_fields(&mut multipart).await?;
    let meta = parse_image_variation_meta(&fields).map_err(|e| {
        OpenAiErrorResponse::invalid_request(format!("invalid image variation form: {e}"))
    })?;

    token_info
        .ensure_model_allowed(&meta.model)
        .map_err(|e| OpenAiErrorResponse::from_api_error(&e))?;
    let client_ip = client_ip.to_string();
    token_svc.update_last_used_ip_async(token_info.token_id, client_ip.clone());

    let model_config = billing
        .get_model_config(&meta.model)
        .await
        .map_err(|e| OpenAiErrorResponse::internal_with("failed to load model pricing", e))?;
    let group_ratio = billing
        .get_group_ratio(&token_info.group)
        .await
        .map_err(|e| OpenAiErrorResponse::internal_with("failed to load group pricing", e))?;

    let mut exclude: Vec<i64> = Vec::new();
    let max_retries = 3;
    let start = std::time::Instant::now();
    let requested_model = meta.model.clone();

    for attempt in 0..max_retries {
        let channel = select_channel_by_scopes(
            &router_svc,
            &token_info.group,
            &meta.model,
            &["image_variation", "images_variation", "images"],
            &exclude,
        )
        .await?
        .ok_or_else(|| OpenAiErrorResponse::no_available_channel("no available channel"))?;

        let actual_model = channel
            .model_mapping
            .get(&meta.model)
            .and_then(|value| value.as_str())
            .unwrap_or(&meta.model)
            .to_string();

        let pre_consumed = billing
            .pre_consume(
                token_info.token_id,
                meta.estimated_tokens,
                model_config.input_ratio,
                group_ratio,
            )
            .await
            .map_err(|e| OpenAiErrorResponse::from_quota_error(&e))?;

        let request_builder = match build_image_variation_request(
            http_client.client(),
            &channel.base_url,
            &channel.api_key,
            &fields,
            &actual_model,
        ) {
            Ok(builder) => builder,
            Err(error) => {
                let _ = billing.refund(token_info.token_id, pre_consumed).await;
                router_svc.record_failure_async(
                    &channel,
                    "build_request_error",
                    format!("failed to build image variation request: {error}"),
                );
                exclude.push(channel.channel_id);
                if attempt == max_retries - 1 {
                    return Err(OpenAiErrorResponse::internal_with(
                        "failed to build image variation request",
                        error,
                    ));
                }
                continue;
            }
        };

        match request_builder.send().await {
            Ok(resp) if resp.status().is_success() => {
                let elapsed = start.elapsed().as_millis() as i64;
                let body = match resp.bytes().await {
                    Ok(body) => body,
                    Err(error) => {
                        let _ = billing.refund(token_info.token_id, pre_consumed).await;
                        router_svc.record_failure_async(
                            &channel,
                            "read_response_error",
                            format!("failed to read image variation response: {error}"),
                        );
                        exclude.push(channel.channel_id);
                        if attempt == max_retries - 1 {
                            return Err(OpenAiErrorResponse::internal_with(
                                "failed to read image variation response",
                                error,
                            ));
                        }
                        continue;
                    }
                };
                let parsed = match serde_json::from_slice::<ImageGenerationResponse>(&body) {
                    Ok(parsed) => parsed,
                    Err(error) => {
                        let _ = billing.refund(token_info.token_id, pre_consumed).await;
                        router_svc.record_failure_async(
                            &channel,
                            "parse_response_error",
                            format!("failed to parse image variation response: {error}"),
                        );
                        exclude.push(channel.channel_id);
                        if attempt == max_retries - 1 {
                            return Err(OpenAiErrorResponse::internal_with(
                                "failed to parse image variation response",
                                error,
                            ));
                        }
                        continue;
                    }
                };
                router_svc.record_success_async(&channel, elapsed as i32);

                let usage = Usage {
                    prompt_tokens: meta.estimated_tokens,
                    completion_tokens: 0,
                    total_tokens: meta.estimated_tokens,
                    cached_tokens: 0,
                    reasoning_tokens: 0,
                };
                let actual_quota = billing
                    .commit_reserved_quota(&token_info, pre_consumed)
                    .await
                    .map_err(|e| {
                        OpenAiErrorResponse::internal_with("failed to commit reserved quota", e)
                    })?;

                log_svc.record_endpoint_usage_async(
                    &token_info,
                    &channel,
                    &usage,
                    EndpointUsageLogRecord {
                        endpoint: "images/variations".into(),
                        requested_model,
                        upstream_model: actual_model,
                        model_name: model_config.model_name,
                        quota: actual_quota,
                        elapsed_time: elapsed as i32,
                        first_token_time: 0,
                        is_stream: false,
                        client_ip: client_ip.clone(),
                    },
                );

                return Ok(Json(parsed));
            }
            Ok(resp) => {
                let _ = billing.refund(token_info.token_id, pre_consumed).await;
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                router_svc.record_failure_async(
                    &channel,
                    status.as_u16().to_string(),
                    format!("upstream HTTP {} {}", status.as_u16(), body),
                );
                exclude.push(channel.channel_id);
                if attempt == max_retries - 1 {
                    return Err(OpenAiErrorResponse::no_available_channel(
                        "all channels failed",
                    ));
                }
            }
            Err(error) => {
                let _ = billing.refund(token_info.token_id, pre_consumed).await;
                router_svc.record_failure_async(
                    &channel,
                    "request_error",
                    format!("failed to send image variation request: {error}"),
                );
                exclude.push(channel.channel_id);
                if attempt == max_retries - 1 {
                    return Err(OpenAiErrorResponse::no_available_channel(
                        "all channels failed",
                    ));
                }
            }
        }
    }

    Err(OpenAiErrorResponse::no_available_channel(
        "all channels failed",
    ))
}

/// POST /v1/audio/speech
#[post_api("/v1/audio/speech")]
#[allow(clippy::too_many_arguments)]
pub async fn audio_speech(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(billing): Component<BillingEngine>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(log_svc): Component<LogService>,
    Component(token_svc): Component<TokenService>,
    ClientIp(client_ip): ClientIp,
    Json(req): Json<AudioSpeechRequest>,
) -> OpenAiApiResult<Response> {
    token_info
        .ensure_model_allowed(&req.model)
        .map_err(|e| OpenAiErrorResponse::from_api_error(&e))?;
    let client_ip = client_ip.to_string();
    token_svc.update_last_used_ip_async(token_info.token_id, client_ip.clone());

    let model_config = billing
        .get_model_config(&req.model)
        .await
        .map_err(|e| OpenAiErrorResponse::internal_with("failed to load model pricing", e))?;
    let group_ratio = billing
        .get_group_ratio(&token_info.group)
        .await
        .map_err(|e| OpenAiErrorResponse::internal_with("failed to load group pricing", e))?;

    let mut exclude: Vec<i64> = Vec::new();
    let max_retries = 3;
    let start = std::time::Instant::now();
    let requested_model = req.model.clone();
    let estimated_tokens = req.estimate_input_tokens();

    for attempt in 0..max_retries {
        let channel = select_channel_by_scopes(
            &router_svc,
            &token_info.group,
            &req.model,
            &["audio_speech", "speech", "audio"],
            &exclude,
        )
        .await?
        .ok_or_else(|| OpenAiErrorResponse::no_available_channel("no available channel"))?;

        let actual_model = channel
            .model_mapping
            .get(&req.model)
            .and_then(|value| value.as_str())
            .unwrap_or(&req.model)
            .to_string();

        let pre_consumed = billing
            .pre_consume(
                token_info.token_id,
                estimated_tokens,
                model_config.input_ratio,
                group_ratio,
            )
            .await
            .map_err(|e| OpenAiErrorResponse::from_quota_error(&e))?;

        let request_builder = match build_audio_speech_request(
            http_client.client(),
            &channel.base_url,
            &channel.api_key,
            &req,
            &actual_model,
        ) {
            Ok(builder) => builder,
            Err(error) => {
                let _ = billing.refund(token_info.token_id, pre_consumed).await;
                router_svc.record_failure_async(
                    &channel,
                    "build_request_error",
                    format!("failed to build audio speech request: {error}"),
                );
                exclude.push(channel.channel_id);
                if attempt == max_retries - 1 {
                    return Err(OpenAiErrorResponse::internal_with(
                        "failed to build audio speech request",
                        error,
                    ));
                }
                continue;
            }
        };

        match request_builder.send().await {
            Ok(resp) if resp.status().is_success() => {
                let elapsed = start.elapsed().as_millis() as i64;
                let content_type = resp
                    .headers()
                    .get(CONTENT_TYPE)
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or("audio/mpeg")
                    .to_string();
                let body = match resp.bytes().await {
                    Ok(body) => body,
                    Err(error) => {
                        let _ = billing.refund(token_info.token_id, pre_consumed).await;
                        router_svc.record_failure_async(
                            &channel,
                            "read_response_error",
                            format!("failed to read audio speech response: {error}"),
                        );
                        exclude.push(channel.channel_id);
                        if attempt == max_retries - 1 {
                            return Err(OpenAiErrorResponse::internal_with(
                                "failed to read audio speech response",
                                error,
                            ));
                        }
                        continue;
                    }
                };
                router_svc.record_success_async(&channel, elapsed as i32);

                let usage = Usage {
                    prompt_tokens: estimated_tokens,
                    completion_tokens: 0,
                    total_tokens: estimated_tokens,
                    cached_tokens: 0,
                    reasoning_tokens: 0,
                };
                let actual_quota = billing
                    .commit_reserved_quota(&token_info, pre_consumed)
                    .await
                    .map_err(|e| {
                        OpenAiErrorResponse::internal_with("failed to commit reserved quota", e)
                    })?;

                log_svc.record_endpoint_usage_async(
                    &token_info,
                    &channel,
                    &usage,
                    EndpointUsageLogRecord {
                        endpoint: "audio/speech".into(),
                        requested_model,
                        upstream_model: actual_model,
                        model_name: model_config.model_name,
                        quota: actual_quota,
                        elapsed_time: elapsed as i32,
                        first_token_time: 0,
                        is_stream: false,
                        client_ip: client_ip.clone(),
                    },
                );

                return Ok(binary_response(body, &content_type));
            }
            Ok(resp) => {
                let _ = billing.refund(token_info.token_id, pre_consumed).await;
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                router_svc.record_failure_async(
                    &channel,
                    status.as_u16().to_string(),
                    format!("upstream HTTP {} {}", status.as_u16(), body),
                );
                exclude.push(channel.channel_id);
                if attempt == max_retries - 1 {
                    return Err(OpenAiErrorResponse::no_available_channel(
                        "all channels failed",
                    ));
                }
            }
            Err(error) => {
                let _ = billing.refund(token_info.token_id, pre_consumed).await;
                router_svc.record_failure_async(
                    &channel,
                    "request_error",
                    format!("failed to send audio speech request: {error}"),
                );
                exclude.push(channel.channel_id);
                if attempt == max_retries - 1 {
                    return Err(OpenAiErrorResponse::no_available_channel(
                        "all channels failed",
                    ));
                }
            }
        }
    }

    Err(OpenAiErrorResponse::no_available_channel(
        "all channels failed",
    ))
}

/// POST /v1/audio/transcriptions
#[post_api("/v1/audio/transcriptions")]
#[allow(clippy::too_many_arguments)]
pub async fn audio_transcriptions(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(billing): Component<BillingEngine>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(log_svc): Component<LogService>,
    Component(token_svc): Component<TokenService>,
    ClientIp(client_ip): ClientIp,
    Multipart(mut multipart): Multipart,
) -> OpenAiApiResult<Response> {
    let fields = buffer_multipart_fields(&mut multipart).await?;
    let meta = parse_audio_transcription_meta(&fields).map_err(|e| {
        OpenAiErrorResponse::invalid_request(format!("invalid transcription form: {e}"))
    })?;

    token_info
        .ensure_model_allowed(&meta.model)
        .map_err(|e| OpenAiErrorResponse::from_api_error(&e))?;
    let client_ip = client_ip.to_string();
    token_svc.update_last_used_ip_async(token_info.token_id, client_ip.clone());

    let model_config = billing
        .get_model_config(&meta.model)
        .await
        .map_err(|e| OpenAiErrorResponse::internal_with("failed to load model pricing", e))?;
    let group_ratio = billing
        .get_group_ratio(&token_info.group)
        .await
        .map_err(|e| OpenAiErrorResponse::internal_with("failed to load group pricing", e))?;

    let mut exclude: Vec<i64> = Vec::new();
    let max_retries = 3;
    let start = std::time::Instant::now();
    let requested_model = meta.model.clone();

    for attempt in 0..max_retries {
        let channel = select_channel_by_scopes(
            &router_svc,
            &token_info.group,
            &meta.model,
            &[
                "audio_transcriptions",
                "audio_transcription",
                "transcription",
                "audio",
            ],
            &exclude,
        )
        .await?
        .ok_or_else(|| OpenAiErrorResponse::no_available_channel("no available channel"))?;

        let actual_model = channel
            .model_mapping
            .get(&meta.model)
            .and_then(|value| value.as_str())
            .unwrap_or(&meta.model)
            .to_string();

        let pre_consumed = billing
            .pre_consume(
                token_info.token_id,
                meta.estimated_tokens,
                model_config.input_ratio,
                group_ratio,
            )
            .await
            .map_err(|e| OpenAiErrorResponse::from_quota_error(&e))?;

        let form = build_audio_transcription_form(&fields, &actual_model).map_err(|e| {
            OpenAiErrorResponse::invalid_request(format!("invalid transcription form: {e}"))
        })?;
        let url = format!(
            "{}/v1/audio/transcriptions",
            channel.base_url.trim_end_matches('/')
        );

        match http_client
            .client()
            .post(url)
            .bearer_auth(&channel.api_key)
            .multipart(form)
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                let elapsed = start.elapsed().as_millis() as i64;
                let content_type = resp
                    .headers()
                    .get(CONTENT_TYPE)
                    .and_then(|value| value.to_str().ok())
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(|| {
                        default_transcription_content_type(meta.response_format.as_deref())
                            .to_string()
                    });
                let body = match resp.bytes().await {
                    Ok(body) => body,
                    Err(error) => {
                        let _ = billing.refund(token_info.token_id, pre_consumed).await;
                        router_svc.record_failure_async(
                            &channel,
                            "read_response_error",
                            format!("failed to read audio transcription response: {error}"),
                        );
                        exclude.push(channel.channel_id);
                        if attempt == max_retries - 1 {
                            return Err(OpenAiErrorResponse::internal_with(
                                "failed to read audio transcription response",
                                error,
                            ));
                        }
                        continue;
                    }
                };
                router_svc.record_success_async(&channel, elapsed as i32);

                let usage = Usage {
                    prompt_tokens: meta.estimated_tokens,
                    completion_tokens: 0,
                    total_tokens: meta.estimated_tokens,
                    cached_tokens: 0,
                    reasoning_tokens: 0,
                };
                let actual_quota = billing
                    .commit_reserved_quota(&token_info, pre_consumed)
                    .await
                    .map_err(|e| {
                        OpenAiErrorResponse::internal_with("failed to commit reserved quota", e)
                    })?;

                log_svc.record_endpoint_usage_async(
                    &token_info,
                    &channel,
                    &usage,
                    EndpointUsageLogRecord {
                        endpoint: "audio/transcriptions".into(),
                        requested_model,
                        upstream_model: actual_model,
                        model_name: model_config.model_name,
                        quota: actual_quota,
                        elapsed_time: elapsed as i32,
                        first_token_time: 0,
                        is_stream: false,
                        client_ip: client_ip.clone(),
                    },
                );

                return Ok(binary_response(body, &content_type));
            }
            Ok(resp) => {
                let _ = billing.refund(token_info.token_id, pre_consumed).await;
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                router_svc.record_failure_async(
                    &channel,
                    status.as_u16().to_string(),
                    format!("upstream HTTP {} {}", status.as_u16(), body),
                );
                exclude.push(channel.channel_id);
                if attempt == max_retries - 1 {
                    return Err(OpenAiErrorResponse::no_available_channel(
                        "all channels failed",
                    ));
                }
            }
            Err(error) => {
                let _ = billing.refund(token_info.token_id, pre_consumed).await;
                router_svc.record_failure_async(
                    &channel,
                    "request_error",
                    format!("failed to send audio transcription request: {error}"),
                );
                exclude.push(channel.channel_id);
                if attempt == max_retries - 1 {
                    return Err(OpenAiErrorResponse::no_available_channel(
                        "all channels failed",
                    ));
                }
            }
        }
    }

    Err(OpenAiErrorResponse::no_available_channel(
        "all channels failed",
    ))
}

/// POST /v1/audio/translations
#[post_api("/v1/audio/translations")]
#[allow(clippy::too_many_arguments)]
pub async fn audio_translations(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(billing): Component<BillingEngine>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(log_svc): Component<LogService>,
    Component(token_svc): Component<TokenService>,
    ClientIp(client_ip): ClientIp,
    Multipart(mut multipart): Multipart,
) -> OpenAiApiResult<Response> {
    let fields = buffer_multipart_fields(&mut multipart).await?;
    let meta = parse_audio_transcription_meta(&fields).map_err(|e| {
        OpenAiErrorResponse::invalid_request(format!("invalid translation form: {e}"))
    })?;

    token_info
        .ensure_model_allowed(&meta.model)
        .map_err(|e| OpenAiErrorResponse::from_api_error(&e))?;
    let client_ip = client_ip.to_string();
    token_svc.update_last_used_ip_async(token_info.token_id, client_ip.clone());

    let model_config = billing
        .get_model_config(&meta.model)
        .await
        .map_err(|e| OpenAiErrorResponse::internal_with("failed to load model pricing", e))?;
    let group_ratio = billing
        .get_group_ratio(&token_info.group)
        .await
        .map_err(|e| OpenAiErrorResponse::internal_with("failed to load group pricing", e))?;

    let mut exclude: Vec<i64> = Vec::new();
    let max_retries = 3;
    let start = std::time::Instant::now();
    let requested_model = meta.model.clone();

    for attempt in 0..max_retries {
        let channel = select_channel_by_scopes(
            &router_svc,
            &token_info.group,
            &meta.model,
            &["audio_translations", "translation", "audio"],
            &exclude,
        )
        .await?
        .ok_or_else(|| OpenAiErrorResponse::no_available_channel("no available channel"))?;

        let actual_model = channel
            .model_mapping
            .get(&meta.model)
            .and_then(|value| value.as_str())
            .unwrap_or(&meta.model)
            .to_string();

        let pre_consumed = billing
            .pre_consume(
                token_info.token_id,
                meta.estimated_tokens,
                model_config.input_ratio,
                group_ratio,
            )
            .await
            .map_err(|e| OpenAiErrorResponse::from_quota_error(&e))?;

        let form = build_audio_translation_form(&fields, &actual_model).map_err(|e| {
            OpenAiErrorResponse::invalid_request(format!("invalid translation form: {e}"))
        })?;
        let url = format!(
            "{}/v1/audio/translations",
            channel.base_url.trim_end_matches('/')
        );

        match http_client
            .client()
            .post(url)
            .bearer_auth(&channel.api_key)
            .multipart(form)
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                let elapsed = start.elapsed().as_millis() as i64;
                let content_type = resp
                    .headers()
                    .get(CONTENT_TYPE)
                    .and_then(|value| value.to_str().ok())
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(|| {
                        default_transcription_content_type(meta.response_format.as_deref())
                            .to_string()
                    });
                let body = match resp.bytes().await {
                    Ok(body) => body,
                    Err(error) => {
                        let _ = billing.refund(token_info.token_id, pre_consumed).await;
                        router_svc.record_failure_async(
                            &channel,
                            "read_response_error",
                            format!("failed to read audio translation response: {error}"),
                        );
                        exclude.push(channel.channel_id);
                        if attempt == max_retries - 1 {
                            return Err(OpenAiErrorResponse::internal_with(
                                "failed to read audio translation response",
                                error,
                            ));
                        }
                        continue;
                    }
                };
                router_svc.record_success_async(&channel, elapsed as i32);

                let usage = Usage {
                    prompt_tokens: meta.estimated_tokens,
                    completion_tokens: 0,
                    total_tokens: meta.estimated_tokens,
                    cached_tokens: 0,
                    reasoning_tokens: 0,
                };
                let actual_quota = billing
                    .commit_reserved_quota(&token_info, pre_consumed)
                    .await
                    .map_err(|e| {
                        OpenAiErrorResponse::internal_with("failed to commit reserved quota", e)
                    })?;

                log_svc.record_endpoint_usage_async(
                    &token_info,
                    &channel,
                    &usage,
                    EndpointUsageLogRecord {
                        endpoint: "audio/translations".into(),
                        requested_model,
                        upstream_model: actual_model,
                        model_name: model_config.model_name,
                        quota: actual_quota,
                        elapsed_time: elapsed as i32,
                        first_token_time: 0,
                        is_stream: false,
                        client_ip: client_ip.clone(),
                    },
                );

                return Ok(binary_response(body, &content_type));
            }
            Ok(resp) => {
                let _ = billing.refund(token_info.token_id, pre_consumed).await;
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                router_svc.record_failure_async(
                    &channel,
                    status.as_u16().to_string(),
                    format!("upstream HTTP {} {}", status.as_u16(), body),
                );
                exclude.push(channel.channel_id);
                if attempt == max_retries - 1 {
                    return Err(OpenAiErrorResponse::no_available_channel(
                        "all channels failed",
                    ));
                }
            }
            Err(error) => {
                let _ = billing.refund(token_info.token_id, pre_consumed).await;
                router_svc.record_failure_async(
                    &channel,
                    "request_error",
                    format!("failed to send audio translation request: {error}"),
                );
                exclude.push(channel.channel_id);
                if attempt == max_retries - 1 {
                    return Err(OpenAiErrorResponse::no_available_channel(
                        "all channels failed",
                    ));
                }
            }
        }
    }

    Err(OpenAiErrorResponse::no_available_channel(
        "all channels failed",
    ))
}

fn build_embeddings_request(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    req: &EmbeddingsRequest,
    actual_model: &str,
) -> anyhow::Result<reqwest::RequestBuilder> {
    let mut body = serde_json::to_value(req)?;
    body["model"] = serde_json::Value::String(actual_model.to_string());

    let url = format!("{}/v1/embeddings", base_url.trim_end_matches('/'));
    Ok(client.post(url).bearer_auth(api_key).json(&body))
}

fn build_image_generation_request(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    req: &ImageGenerationRequest,
    actual_model: &str,
) -> anyhow::Result<reqwest::RequestBuilder> {
    let mut body = serde_json::to_value(req)?;
    body["model"] = serde_json::Value::String(actual_model.to_string());

    let url = format!("{}/v1/images/generations", base_url.trim_end_matches('/'));
    Ok(client.post(url).bearer_auth(api_key).json(&body))
}

fn build_audio_speech_request(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    req: &AudioSpeechRequest,
    actual_model: &str,
) -> anyhow::Result<reqwest::RequestBuilder> {
    let mut body = serde_json::to_value(req)?;
    body["model"] = serde_json::Value::String(actual_model.to_string());

    let url = format!("{}/v1/audio/speech", base_url.trim_end_matches('/'));
    Ok(client.post(url).bearer_auth(api_key).json(&body))
}

fn build_moderation_request(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    req: &ModerationRequest,
    actual_model: &str,
) -> anyhow::Result<reqwest::RequestBuilder> {
    let mut body = serde_json::to_value(req)?;
    body["model"] = serde_json::Value::String(actual_model.to_string());

    let url = format!("{}/v1/moderations", base_url.trim_end_matches('/'));
    Ok(client.post(url).bearer_auth(api_key).json(&body))
}

fn build_file_upload_form(
    fields: &[BufferedMultipartField],
) -> anyhow::Result<reqwest::multipart::Form> {
    let mut form = reqwest::multipart::Form::new();
    let mut has_file = false;

    for field in fields {
        let mut part = if let Some(filename) = field.filename.as_ref() {
            if field.name == "file" {
                has_file = true;
            }
            let mut part =
                reqwest::multipart::Part::stream(field.bytes.clone()).file_name(filename.clone());
            if let Some(content_type) = field.content_type.as_ref() {
                part = part.mime_str(content_type)?;
            }
            part
        } else {
            let text = String::from_utf8(field.bytes.to_vec()).map_err(|e| {
                anyhow::anyhow!("multipart field '{}' is not valid UTF-8: {e}", field.name)
            })?;
            reqwest::multipart::Part::text(text)
        };

        if field.filename.is_none()
            && let Some(content_type) = field.content_type.as_ref()
        {
            part = part.mime_str(content_type)?;
        }

        form = form.part(field.name.clone(), part);
    }

    if !has_file {
        anyhow::bail!("missing file field");
    }

    Ok(form)
}

fn build_file_upload_request(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    fields: &[BufferedMultipartField],
) -> anyhow::Result<reqwest::RequestBuilder> {
    let form = build_file_upload_form(fields)?;
    let url = format!("{}/v1/files", base_url.trim_end_matches('/'));
    Ok(client.post(url).bearer_auth(api_key).multipart(form))
}

fn binary_response(bytes: bytes::Bytes, content_type: &str) -> Response {
    Response::builder()
        .header(CONTENT_TYPE, content_type)
        .body(Body::from(bytes))
        .unwrap_or_else(|_| Response::new(Body::empty()))
}

#[derive(Clone, Debug)]
struct BufferedMultipartField {
    name: String,
    filename: Option<String>,
    content_type: Option<String>,
    bytes: bytes::Bytes,
}

#[derive(Clone, Debug)]
struct AudioTranscriptionMeta {
    model: String,
    response_format: Option<String>,
    estimated_tokens: i32,
}

#[derive(Clone, Debug)]
struct ImageEditMeta {
    model: String,
    estimated_tokens: i32,
}

#[derive(Clone, Debug)]
struct ImageVariationMeta {
    model: String,
    estimated_tokens: i32,
}

async fn buffer_multipart_fields(
    multipart: &mut summer_web::axum::extract::Multipart,
) -> OpenAiApiResult<Vec<BufferedMultipartField>> {
    let mut fields = Vec::with_capacity(8);
    while let Some(field) = multipart.next_field().await.map_err(|e| {
        OpenAiErrorResponse::invalid_request(format!("failed to read multipart field: {e}"))
    })? {
        let Some(name) = field.name().map(ToOwned::to_owned) else {
            continue;
        };
        let filename = field.file_name().map(ToOwned::to_owned);
        let content_type = field.content_type().map(ToOwned::to_owned);
        let bytes = field.bytes().await.map_err(|e| {
            OpenAiErrorResponse::invalid_request(format!(
                "failed to read multipart field body: {e}"
            ))
        })?;
        fields.push(BufferedMultipartField {
            name,
            filename,
            content_type,
            bytes,
        });
    }
    Ok(fields)
}

fn parse_audio_transcription_meta(
    fields: &[BufferedMultipartField],
) -> anyhow::Result<AudioTranscriptionMeta> {
    let model = multipart_text_field(fields, "model")
        .ok_or_else(|| anyhow::anyhow!("missing 'model' field"))?;
    let response_format = multipart_text_field(fields, "response_format");
    let prompt = multipart_text_field(fields, "prompt").unwrap_or_default();

    Ok(AudioTranscriptionMeta {
        model,
        response_format,
        estimated_tokens: ((prompt.len() as f64) / 4.0).ceil() as i32,
    })
}

fn parse_image_edit_meta(fields: &[BufferedMultipartField]) -> anyhow::Result<ImageEditMeta> {
    let model = multipart_text_field(fields, "model")
        .ok_or_else(|| anyhow::anyhow!("missing 'model' field"))?;
    let prompt = multipart_text_field(fields, "prompt")
        .ok_or_else(|| anyhow::anyhow!("missing 'prompt' field"))?;

    Ok(ImageEditMeta {
        model,
        estimated_tokens: ((prompt.len() as f64) / 4.0).ceil() as i32,
    })
}

fn parse_image_variation_meta(
    fields: &[BufferedMultipartField],
) -> anyhow::Result<ImageVariationMeta> {
    let model = multipart_text_field(fields, "model")
        .ok_or_else(|| anyhow::anyhow!("missing 'model' field"))?;
    let estimated_tokens = match multipart_text_field(fields, "n") {
        Some(value) => value
            .trim()
            .parse::<i32>()
            .map_err(|e| anyhow::anyhow!("invalid 'n' field: {e}"))?
            .max(1),
        None => 1,
    };

    Ok(ImageVariationMeta {
        model,
        estimated_tokens,
    })
}

fn build_audio_transcription_form(
    fields: &[BufferedMultipartField],
    actual_model: &str,
) -> anyhow::Result<reqwest::multipart::Form> {
    let mut form = reqwest::multipart::Form::new();
    let mut has_file = false;

    for field in fields {
        let mut part = if let Some(filename) = field.filename.as_ref() {
            has_file = true;
            let mut part =
                reqwest::multipart::Part::stream(field.bytes.clone()).file_name(filename.clone());
            if let Some(content_type) = field.content_type.as_ref() {
                part = part.mime_str(content_type)?;
            }
            part
        } else {
            let text = if field.name == "model" {
                actual_model.to_string()
            } else {
                String::from_utf8(field.bytes.to_vec()).map_err(|e| {
                    anyhow::anyhow!("multipart field '{}' is not valid UTF-8: {e}", field.name)
                })?
            };
            reqwest::multipart::Part::text(text)
        };

        if field.filename.is_none()
            && let Some(content_type) = field.content_type.as_ref()
        {
            part = part.mime_str(content_type)?;
        }

        form = form.part(field.name.clone(), part);
    }

    if !has_file {
        anyhow::bail!("missing audio file field");
    }

    Ok(form)
}

fn build_audio_translation_form(
    fields: &[BufferedMultipartField],
    actual_model: &str,
) -> anyhow::Result<reqwest::multipart::Form> {
    build_audio_transcription_form(fields, actual_model)
}

fn build_image_edit_form(
    fields: &[BufferedMultipartField],
    actual_model: &str,
) -> anyhow::Result<reqwest::multipart::Form> {
    let mut form = reqwest::multipart::Form::new();
    let mut has_image = false;
    let mut has_prompt = false;

    for field in fields {
        let mut part = if let Some(filename) = field.filename.as_ref() {
            if field.name == "image" || field.name == "image[]" {
                has_image = true;
            }
            let mut part =
                reqwest::multipart::Part::stream(field.bytes.clone()).file_name(filename.clone());
            if let Some(content_type) = field.content_type.as_ref() {
                part = part.mime_str(content_type)?;
            }
            part
        } else {
            let text = if field.name == "model" {
                actual_model.to_string()
            } else {
                String::from_utf8(field.bytes.to_vec()).map_err(|e| {
                    anyhow::anyhow!("multipart field '{}' is not valid UTF-8: {e}", field.name)
                })?
            };
            if field.name == "prompt" && !text.trim().is_empty() {
                has_prompt = true;
            }
            reqwest::multipart::Part::text(text)
        };

        if field.filename.is_none()
            && let Some(content_type) = field.content_type.as_ref()
        {
            part = part.mime_str(content_type)?;
        }

        form = form.part(field.name.clone(), part);
    }

    if !has_image {
        anyhow::bail!("missing image file field");
    }
    if !has_prompt {
        anyhow::bail!("missing prompt field");
    }

    Ok(form)
}

fn build_image_variation_form(
    fields: &[BufferedMultipartField],
    actual_model: &str,
) -> anyhow::Result<reqwest::multipart::Form> {
    let mut form = reqwest::multipart::Form::new();
    let mut has_image = false;

    for field in fields {
        let mut part = if let Some(filename) = field.filename.as_ref() {
            if field.name == "image" || field.name == "image[]" {
                has_image = true;
            }
            let mut part =
                reqwest::multipart::Part::stream(field.bytes.clone()).file_name(filename.clone());
            if let Some(content_type) = field.content_type.as_ref() {
                part = part.mime_str(content_type)?;
            }
            part
        } else {
            let text = if field.name == "model" {
                actual_model.to_string()
            } else {
                String::from_utf8(field.bytes.to_vec()).map_err(|e| {
                    anyhow::anyhow!("multipart field '{}' is not valid UTF-8: {e}", field.name)
                })?
            };
            reqwest::multipart::Part::text(text)
        };

        if field.filename.is_none()
            && let Some(content_type) = field.content_type.as_ref()
        {
            part = part.mime_str(content_type)?;
        }

        form = form.part(field.name.clone(), part);
    }

    if !has_image {
        anyhow::bail!("missing image file field");
    }

    Ok(form)
}

fn build_image_variation_request(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    fields: &[BufferedMultipartField],
    actual_model: &str,
) -> anyhow::Result<reqwest::RequestBuilder> {
    let form = build_image_variation_form(fields, actual_model)?;
    let url = format!("{}/v1/images/variations", base_url.trim_end_matches('/'));
    Ok(client.post(url).bearer_auth(api_key).multipart(form))
}

fn multipart_text_field(fields: &[BufferedMultipartField], name: &str) -> Option<String> {
    fields.iter().find_map(|field| {
        if field.name == name && field.filename.is_none() {
            String::from_utf8(field.bytes.to_vec()).ok()
        } else {
            None
        }
    })
}

fn default_transcription_content_type(response_format: Option<&str>) -> &'static str {
    match response_format.unwrap_or("json") {
        "text" => "text/plain; charset=utf-8",
        "srt" => "application/x-subrip; charset=utf-8",
        "vtt" => "text/vtt; charset=utf-8",
        _ => "application/json",
    }
}

async fn select_channel_by_scopes(
    router_svc: &ChannelRouter,
    group: &str,
    model: &str,
    scopes: &[&str],
    exclude: &[i64],
) -> OpenAiApiResult<Option<crate::relay::channel_router::SelectedChannel>> {
    for scope in scopes {
        let selected = router_svc
            .select_channel(group, model, scope, exclude)
            .await
            .map_err(|e| OpenAiErrorResponse::internal_with("failed to select channel", e))?;
        if selected.is_some() {
            return Ok(selected);
        }
    }

    Ok(None)
}

async fn forward_file_json_request<T>(
    router_svc: &ChannelRouter,
    client: &reqwest::Client,
    group: &str,
    query: &std::collections::HashMap<String, String>,
    method: http::Method,
    path: &str,
    operation: &str,
) -> OpenAiApiResult<Json<T>>
where
    T: serde::de::DeserializeOwned,
{
    let mut exclude: Vec<i64> = Vec::new();
    let max_retries = 3;
    let start = std::time::Instant::now();

    for attempt in 0..max_retries {
        let channel = select_channel_by_scopes(router_svc, group, "", &["files"], &exclude)
            .await?
            .ok_or_else(|| OpenAiErrorResponse::no_available_channel("no available channel"))?;
        let mut url = reqwest::Url::parse(&format!(
            "{}{}",
            channel.base_url.trim_end_matches('/'),
            path
        ))
        .map_err(|e| OpenAiErrorResponse::internal_with("failed to build file request url", e))?;
        if !query.is_empty() {
            {
                let mut pairs = url.query_pairs_mut();
                for (key, value) in query {
                    pairs.append_pair(key, value);
                }
            }
        }
        let request = client
            .request(method.clone(), url)
            .bearer_auth(&channel.api_key);

        match request.send().await {
            Ok(resp) if resp.status().is_success() => {
                let elapsed = start.elapsed().as_millis() as i64;
                let body = resp.bytes().await.map_err(|e| {
                    OpenAiErrorResponse::internal_error(format!(
                        "failed to read {operation} response: {e}"
                    ))
                })?;
                let parsed = serde_json::from_slice::<T>(&body).map_err(|e| {
                    OpenAiErrorResponse::internal_error(format!(
                        "failed to parse {operation} response: {e}"
                    ))
                })?;
                router_svc.record_success_async(&channel, elapsed as i32);
                return Ok(Json(parsed));
            }
            Ok(resp) => {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                router_svc.record_failure_async(
                    &channel,
                    status.as_u16().to_string(),
                    format!("upstream HTTP {} {}", status.as_u16(), body),
                );
                exclude.push(channel.channel_id);
                if attempt == max_retries - 1 {
                    return Err(OpenAiErrorResponse::no_available_channel(
                        "all channels failed",
                    ));
                }
            }
            Err(error) => {
                router_svc.record_failure_async(
                    &channel,
                    "request_error",
                    format!("failed to send {operation} request: {error}"),
                );
                exclude.push(channel.channel_id);
                if attempt == max_retries - 1 {
                    return Err(OpenAiErrorResponse::no_available_channel(
                        "all channels failed",
                    ));
                }
            }
        }
    }

    Err(OpenAiErrorResponse::no_available_channel(
        "all channels failed",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use summer_ai_core::types::audio::AudioSpeechRequest;
    use summer_ai_core::types::file::FileObject;
    use summer_ai_core::types::image::ImageGenerationRequest;
    use summer_ai_core::types::moderation::ModerationRequest;

    #[test]
    fn build_image_generation_request_targets_openai_images_endpoint() {
        let client = reqwest::Client::new();
        let req: ImageGenerationRequest = serde_json::from_value(serde_json::json!({
            "model": "gpt-image-1",
            "prompt": "draw a fox"
        }))
        .unwrap();

        let built = build_image_generation_request(
            &client,
            "https://api.example.com/",
            "sk-test",
            &req,
            "mapped-image-model",
        )
        .unwrap()
        .build()
        .unwrap();

        assert_eq!(
            built.url().as_str(),
            "https://api.example.com/v1/images/generations"
        );
        let body: serde_json::Value =
            serde_json::from_slice(built.body().unwrap().as_bytes().unwrap()).unwrap();
        assert_eq!(body["model"], "mapped-image-model");
    }

    #[test]
    fn build_audio_speech_request_targets_openai_audio_endpoint() {
        let client = reqwest::Client::new();
        let req: AudioSpeechRequest = serde_json::from_value(serde_json::json!({
            "model": "gpt-4o-mini-tts",
            "input": "hello",
            "voice": "alloy"
        }))
        .unwrap();

        let built = build_audio_speech_request(
            &client,
            "https://api.example.com/",
            "sk-test",
            &req,
            "mapped-tts-model",
        )
        .unwrap()
        .build()
        .unwrap();

        assert_eq!(
            built.url().as_str(),
            "https://api.example.com/v1/audio/speech"
        );
        let body: serde_json::Value =
            serde_json::from_slice(built.body().unwrap().as_bytes().unwrap()).unwrap();
        assert_eq!(body["model"], "mapped-tts-model");
    }

    #[test]
    fn parse_audio_transcription_meta_reads_model_and_response_format() {
        let fields = vec![
            BufferedMultipartField {
                name: "model".into(),
                filename: None,
                content_type: None,
                bytes: bytes::Bytes::from("whisper-1"),
            },
            BufferedMultipartField {
                name: "response_format".into(),
                filename: None,
                content_type: None,
                bytes: bytes::Bytes::from("verbose_json"),
            },
            BufferedMultipartField {
                name: "prompt".into(),
                filename: None,
                content_type: None,
                bytes: bytes::Bytes::from("hello world"),
            },
        ];

        let meta = parse_audio_transcription_meta(&fields).unwrap();
        assert_eq!(meta.model, "whisper-1");
        assert_eq!(meta.response_format.as_deref(), Some("verbose_json"));
        assert_eq!(meta.estimated_tokens, 3);
    }

    #[test]
    fn parse_image_edit_meta_reads_model_and_prompt() {
        let fields = vec![
            BufferedMultipartField {
                name: "model".into(),
                filename: None,
                content_type: None,
                bytes: bytes::Bytes::from("gpt-image-1"),
            },
            BufferedMultipartField {
                name: "prompt".into(),
                filename: None,
                content_type: None,
                bytes: bytes::Bytes::from("make the sky blue"),
            },
        ];

        let meta = parse_image_edit_meta(&fields).unwrap();
        assert_eq!(meta.model, "gpt-image-1");
        assert_eq!(meta.estimated_tokens, 5);
    }

    #[test]
    fn parse_image_variation_meta_reads_model_and_image_count() {
        let fields = vec![
            BufferedMultipartField {
                name: "model".into(),
                filename: None,
                content_type: None,
                bytes: bytes::Bytes::from("dall-e-2"),
            },
            BufferedMultipartField {
                name: "n".into(),
                filename: None,
                content_type: None,
                bytes: bytes::Bytes::from("3"),
            },
            BufferedMultipartField {
                name: "image".into(),
                filename: Some("otter.png".into()),
                content_type: Some("image/png".into()),
                bytes: bytes::Bytes::from_static(b"png-bytes"),
            },
        ];

        let meta = parse_image_variation_meta(&fields).unwrap();
        assert_eq!(meta.model, "dall-e-2");
        assert_eq!(meta.estimated_tokens, 3);
    }

    #[test]
    fn build_image_variation_request_targets_openai_variations_endpoint() {
        let client = reqwest::Client::new();
        let fields = vec![
            BufferedMultipartField {
                name: "model".into(),
                filename: None,
                content_type: None,
                bytes: bytes::Bytes::from("dall-e-2"),
            },
            BufferedMultipartField {
                name: "image".into(),
                filename: Some("otter.png".into()),
                content_type: Some("image/png".into()),
                bytes: bytes::Bytes::from_static(b"png-bytes"),
            },
        ];

        let built = build_image_variation_request(
            &client,
            "https://api.example.com/",
            "sk-test",
            &fields,
            "mapped-image-model",
        )
        .unwrap()
        .build()
        .unwrap();

        assert_eq!(
            built.url().as_str(),
            "https://api.example.com/v1/images/variations"
        );
    }

    #[test]
    fn build_moderation_request_targets_openai_moderations_endpoint() {
        let client = reqwest::Client::new();
        let req: ModerationRequest = serde_json::from_value(serde_json::json!({
            "model": "omni-moderation-latest",
            "input": "hello"
        }))
        .unwrap();

        let built = build_moderation_request(
            &client,
            "https://api.example.com/",
            "sk-test",
            &req,
            "mapped-moderation-model",
        )
        .unwrap()
        .build()
        .unwrap();

        assert_eq!(
            built.url().as_str(),
            "https://api.example.com/v1/moderations"
        );
        let body: serde_json::Value =
            serde_json::from_slice(built.body().unwrap().as_bytes().unwrap()).unwrap();
        assert_eq!(body["model"], "mapped-moderation-model");
    }

    #[test]
    fn build_file_upload_request_targets_openai_files_endpoint() {
        let client = reqwest::Client::new();
        let fields = vec![
            BufferedMultipartField {
                name: "purpose".into(),
                filename: None,
                content_type: None,
                bytes: bytes::Bytes::from("assistants"),
            },
            BufferedMultipartField {
                name: "file".into(),
                filename: Some("notes.txt".into()),
                content_type: Some("text/plain".into()),
                bytes: bytes::Bytes::from_static(b"hello"),
            },
        ];

        let built =
            build_file_upload_request(&client, "https://api.example.com/", "sk-test", &fields)
                .unwrap()
                .build()
                .unwrap();

        assert_eq!(built.url().as_str(), "https://api.example.com/v1/files");
    }

    #[test]
    fn file_object_deserializes() {
        let file: FileObject = serde_json::from_value(serde_json::json!({
            "id": "file-123",
            "object": "file",
            "bytes": 5,
            "created_at": 1700000000,
            "filename": "notes.txt",
            "purpose": "assistants"
        }))
        .unwrap();

        assert_eq!(file.id, "file-123");
        assert_eq!(file.filename, "notes.txt");
    }
}
