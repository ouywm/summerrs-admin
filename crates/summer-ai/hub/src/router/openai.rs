use summer_web::axum::response::{IntoResponse, Response};
use summer_web::extractor::Component;
use summer_web::{get_api, post_api};

use summer_ai_core::provider::get_adapter;
use summer_ai_core::types::chat::ChatCompletionRequest;
use summer_ai_core::types::error::{OpenAiApiResult, OpenAiErrorResponse};
use summer_ai_core::types::model::ModelListResponse;

use crate::auth::extractor::AiToken;
use crate::relay::billing::{BillingEngine, estimate_prompt_tokens};
use crate::relay::channel_router::ChannelRouter;
use crate::relay::http_client::UpstreamHttpClient;
use crate::relay::stream::build_sse_response;
use crate::service::log::{ChatCompletionLogRecord, LogService};
use crate::service::model::ModelService;
use crate::service::token::TokenService;
use summer_common::extractor::ClientIp;
use summer_common::response::Json;

/// POST /v1/chat/completions
#[post_api("/v1/chat/completions")]
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
                    let stream = adapter.parse_stream(resp, &actual_model).map_err(|e| {
                        OpenAiErrorResponse::internal_with("failed to parse upstream stream", e)
                    })?;
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
                    let body = resp.bytes().await.map_err(|e| {
                        OpenAiErrorResponse::internal_with("failed to read upstream response", e)
                    })?;
                    let parsed = adapter.parse_response(body, &actual_model).map_err(|e| {
                        OpenAiErrorResponse::internal_with("failed to parse upstream response", e)
                    })?;

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
            _ => {
                let _ = billing.refund(token_info.token_id, pre_consumed).await;
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
