#![allow(clippy::too_many_arguments)]

use std::convert::Infallible;

use bytes::Bytes;
use futures::StreamExt;
use reqwest::multipart::{Form, Part};
use serde_json::Value;
use summer_ai_core::types::common::Usage;
use summer_ai_core::types::error::{OpenAiApiResult, OpenAiErrorResponse};
use summer_ai_model::entity::log::LogStatus;
use summer_common::extractor::{ClientIp, Multipart, Path};
use summer_common::response::Json;
use summer_common::user_agent::UserAgentInfo;
use summer_web::axum::body::Body;
use summer_web::axum::extract::Request;
use summer_web::axum::http::{
    HeaderMap, HeaderValue, Method,
    header::{CACHE_CONTROL, CONTENT_TYPE},
};
use summer_web::axum::response::{IntoResponse, Response};
use summer_web::extractor::Component;
use summer_web::{delete_api, get_api, post_api};

use crate::auth::extractor::AiToken;
use crate::relay::billing::{BillingEngine, ModelConfigInfo};
use crate::relay::channel_router::{
    ChannelRouter, RouteSelectionExclusions, RouteSelectionState, SelectedChannel,
};
use crate::relay::http_client::UpstreamHttpClient;
use crate::relay::rate_limit::RateLimitEngine;
use crate::router::openai::{
    apply_upstream_failure_scope, classify_upstream_provider_failure, extract_request_id,
    extract_upstream_request_id, fallback_usage, insert_request_id_header,
    insert_upstream_request_id_header,
};
use crate::service::channel::ChannelService;
use crate::service::log::{AiFailureLogRecord, AiUsageLogRecord, LogService};
use crate::service::resource_affinity::ResourceAffinityService;
use crate::service::response_bridge::ResponseBridgeService;
use crate::service::token::{TokenInfo, TokenService};

pub(crate) mod resource;
pub(crate) mod support;
#[cfg(test)]
pub(crate) use self::resource::resource_affinity_lookup_keys;
pub(crate) use self::resource::{
    ResourceRequestSpec, ResourceRouteState, extract_generic_resource_id, referenced_resource_ids,
};
#[cfg(test)]
pub(crate) use self::support::detect_unusable_upstream_success_response;
pub(crate) use self::support::{
    allow_empty_success_body_for_upstream_path, apply_forward_headers, apply_upstream_auth,
    build_bytes_response, build_upstream_url, unusable_success_response_message,
};
#[cfg(test)]
use summer_web::axum::http::StatusCode;

#[derive(Clone, Copy)]
struct JsonModelRelaySpec {
    endpoint_scope: &'static str,
    upstream_path: &'static str,
    endpoint: &'static str,
    request_format: &'static str,
    default_model: Option<&'static str>,
}

#[derive(Clone, Copy)]
struct MultipartModelRelaySpec {
    endpoint_scope: &'static str,
    upstream_path: &'static str,
    endpoint: &'static str,
    request_format: &'static str,
    default_model: Option<&'static str>,
}

#[derive(Debug, Clone)]
enum MultipartField {
    Text {
        name: String,
        value: String,
    },
    File {
        name: String,
        file_name: String,
        content_type: Option<String>,
        bytes: Bytes,
    },
}

#[derive(Debug, Clone)]
struct ParsedMultipartPayload {
    fields: Vec<MultipartField>,
    model: Option<String>,
    estimated_tokens: i32,
}

#[derive(Default)]
struct GenericStreamTracker {
    buffer: String,
    usage: Option<Usage>,
    upstream_model: String,
    resource_id: String,
    resource_refs: Vec<(&'static str, String)>,
}

#[allow(clippy::too_many_arguments)]
fn record_passthrough_failure(
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

impl GenericStreamTracker {
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

            let Ok(payload) = serde_json::from_str::<Value>(&data) else {
                continue;
            };

            if first_token_time.is_none() && payload_has_text_delta(&payload) {
                *first_token_time = Some(start.elapsed().as_millis() as i64);
            }

            if self.upstream_model.is_empty()
                && let Some(model) = extract_model_from_response_value(&payload)
            {
                self.upstream_model = model;
            }

            if self.resource_id.is_empty()
                && let Some(id) = extract_generic_resource_id(&payload)
            {
                self.resource_id = id;
            }

            for (kind, id) in referenced_resource_ids(&payload) {
                let exists = self
                    .resource_refs
                    .iter()
                    .any(|(existing_kind, existing_id)| {
                        existing_kind == &kind && existing_id == &id
                    });
                if !exists {
                    self.resource_refs.push((kind, id));
                }
            }

            if let Some(usage) = extract_usage_from_value(&payload) {
                self.usage = Some(usage);
            }
        }
    }
}

fn map_response_bridge_error(
    action: &'static str,
    error: impl std::error::Error + Send + Sync + 'static,
) -> OpenAiErrorResponse {
    OpenAiErrorResponse::internal_with(action, error)
}

/// POST /v1/completions
#[post_api("/v1/completions")]
pub async fn completions(
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
    Json(body): Json<Value>,
) -> OpenAiApiResult<Response> {
    relay_json_model_request(
        token_info,
        router_svc,
        billing,
        rate_limiter,
        http_client,
        log_svc,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        headers,
        body,
        JsonModelRelaySpec {
            endpoint_scope: "completions",
            upstream_path: "/v1/completions",
            endpoint: "completions",
            request_format: "openai/completions",
            default_model: None,
        },
    )
    .await
}

/// POST /v1/images/generations
#[post_api("/v1/images/generations")]
pub async fn image_generations(
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
    Json(body): Json<Value>,
) -> OpenAiApiResult<Response> {
    relay_json_model_request(
        token_info,
        router_svc,
        billing,
        rate_limiter,
        http_client,
        log_svc,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        headers,
        body,
        JsonModelRelaySpec {
            endpoint_scope: "images",
            upstream_path: "/v1/images/generations",
            endpoint: "images/generations",
            request_format: "openai/images_generations",
            default_model: Some("gpt-image-1"),
        },
    )
    .await
}

/// POST /v1/images/edits
#[post_api("/v1/images/edits")]
pub async fn image_edits(
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
    Multipart(multipart): Multipart,
) -> OpenAiApiResult<Response> {
    relay_multipart_model_request(
        token_info,
        router_svc,
        billing,
        rate_limiter,
        http_client,
        log_svc,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        headers,
        multipart,
        MultipartModelRelaySpec {
            endpoint_scope: "images",
            upstream_path: "/v1/images/edits",
            endpoint: "images/edits",
            request_format: "openai/images_edits",
            default_model: Some("gpt-image-1"),
        },
    )
    .await
}

/// POST /v1/images/variations
#[post_api("/v1/images/variations")]
pub async fn image_variations(
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
    Multipart(multipart): Multipart,
) -> OpenAiApiResult<Response> {
    relay_multipart_model_request(
        token_info,
        router_svc,
        billing,
        rate_limiter,
        http_client,
        log_svc,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        headers,
        multipart,
        MultipartModelRelaySpec {
            endpoint_scope: "images",
            upstream_path: "/v1/images/variations",
            endpoint: "images/variations",
            request_format: "openai/images_variations",
            default_model: Some("gpt-image-1"),
        },
    )
    .await
}

/// POST /v1/audio/transcriptions
#[post_api("/v1/audio/transcriptions")]
pub async fn audio_transcriptions(
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
    Multipart(multipart): Multipart,
) -> OpenAiApiResult<Response> {
    relay_multipart_model_request(
        token_info,
        router_svc,
        billing,
        rate_limiter,
        http_client,
        log_svc,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        headers,
        multipart,
        MultipartModelRelaySpec {
            endpoint_scope: "audio",
            upstream_path: "/v1/audio/transcriptions",
            endpoint: "audio/transcriptions",
            request_format: "openai/audio_transcriptions",
            default_model: None,
        },
    )
    .await
}

/// POST /v1/audio/translations
#[post_api("/v1/audio/translations")]
pub async fn audio_translations(
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
    Multipart(multipart): Multipart,
) -> OpenAiApiResult<Response> {
    relay_multipart_model_request(
        token_info,
        router_svc,
        billing,
        rate_limiter,
        http_client,
        log_svc,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        headers,
        multipart,
        MultipartModelRelaySpec {
            endpoint_scope: "audio",
            upstream_path: "/v1/audio/translations",
            endpoint: "audio/translations",
            request_format: "openai/audio_translations",
            default_model: None,
        },
    )
    .await
}

/// POST /v1/audio/speech
#[post_api("/v1/audio/speech")]
pub async fn audio_speech(
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
    Json(body): Json<Value>,
) -> OpenAiApiResult<Response> {
    relay_json_model_request(
        token_info,
        router_svc,
        billing,
        rate_limiter,
        http_client,
        log_svc,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        headers,
        body,
        JsonModelRelaySpec {
            endpoint_scope: "audio",
            upstream_path: "/v1/audio/speech",
            endpoint: "audio/speech",
            request_format: "openai/audio_speech",
            default_model: None,
        },
    )
    .await
}

/// POST /v1/moderations
#[post_api("/v1/moderations")]
pub async fn moderations(
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
    Json(body): Json<Value>,
) -> OpenAiApiResult<Response> {
    relay_json_model_request(
        token_info,
        router_svc,
        billing,
        rate_limiter,
        http_client,
        log_svc,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        headers,
        body,
        JsonModelRelaySpec {
            endpoint_scope: "moderations",
            upstream_path: "/v1/moderations",
            endpoint: "moderations",
            request_format: "openai/moderations",
            default_model: Some("omni-moderation-latest"),
        },
    )
    .await
}

/// POST /v1/rerank
#[post_api("/v1/rerank")]
pub async fn rerank(
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
    Json(body): Json<Value>,
) -> OpenAiApiResult<Response> {
    relay_json_model_request(
        token_info,
        router_svc,
        billing,
        rate_limiter,
        http_client,
        log_svc,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        headers,
        body,
        JsonModelRelaySpec {
            endpoint_scope: "rerank",
            upstream_path: "/v1/rerank",
            endpoint: "rerank",
            request_format: "openai/rerank",
            default_model: None,
        },
    )
    .await
}

/// GET /v1/responses/{response_id}
#[get_api("/v1/responses/{response_id}")]
pub async fn get_response(
    AiToken(token_info): AiToken,
    Component(response_bridge): Component<ResponseBridgeService>,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    Path(response_id): Path<String>,
    req: Request,
) -> OpenAiApiResult<Response> {
    let client_ip = client_ip.to_string();
    if let Some(snapshot) = response_bridge
        .get_response(&token_info, &response_id)
        .await
        .map_err(|error| map_response_bridge_error("failed to load bridged response", error))?
    {
        token_info
            .ensure_endpoint_allowed("responses")
            .map_err(|error| OpenAiErrorResponse::from_api_error(&error))?;
        token_svc.update_last_used_ip_async(token_info.token_id, client_ip.clone());
        let request_id = extract_request_id(req.headers());
        let mut response = Json(snapshot.payload).into_response();
        insert_request_id_header(&mut response, &request_id);
        insert_upstream_request_id_header(&mut response, &snapshot.upstream_request_id);
        return Ok(response);
    }

    relay_resource_get(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip,
        req,
        format!("/v1/responses/{response_id}"),
        ResourceRequestSpec {
            endpoint_scope: "responses",
            bind_resource_kind: None,
            delete_resource_kind: None,
        },
        vec![("response", response_id)],
    )
    .await
}

/// GET /v1/responses/{response_id}/input_items
#[get_api("/v1/responses/{response_id}/input_items")]
pub async fn get_response_input_items(
    AiToken(token_info): AiToken,
    Component(response_bridge): Component<ResponseBridgeService>,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    Path(response_id): Path<String>,
    req: Request,
) -> OpenAiApiResult<Response> {
    let client_ip = client_ip.to_string();
    if let Some(snapshot) = response_bridge
        .get_input_items(&token_info, &response_id)
        .await
        .map_err(|error| {
            map_response_bridge_error("failed to load bridged response input items", error)
        })?
    {
        token_info
            .ensure_endpoint_allowed("responses")
            .map_err(|error| OpenAiErrorResponse::from_api_error(&error))?;
        token_svc.update_last_used_ip_async(token_info.token_id, client_ip.clone());
        let request_id = extract_request_id(req.headers());
        let mut response = Json(snapshot.payload).into_response();
        insert_request_id_header(&mut response, &request_id);
        insert_upstream_request_id_header(&mut response, &snapshot.upstream_request_id);
        return Ok(response);
    }

    relay_resource_get(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip,
        req,
        format!("/v1/responses/{response_id}/input_items"),
        ResourceRequestSpec {
            endpoint_scope: "responses",
            bind_resource_kind: None,
            delete_resource_kind: None,
        },
        vec![("response", response_id)],
    )
    .await
}

/// POST /v1/responses/{response_id}/cancel
#[post_api("/v1/responses/{response_id}/cancel")]
pub async fn cancel_response(
    AiToken(token_info): AiToken,
    Component(response_bridge): Component<ResponseBridgeService>,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    Path(response_id): Path<String>,
    req: Request,
) -> OpenAiApiResult<Response> {
    let client_ip = client_ip.to_string();
    if let Some(snapshot) = response_bridge
        .cancel(&token_info, &response_id)
        .await
        .map_err(|error| map_response_bridge_error("failed to cancel bridged response", error))?
    {
        token_info
            .ensure_endpoint_allowed("responses")
            .map_err(|error| OpenAiErrorResponse::from_api_error(&error))?;
        token_svc.update_last_used_ip_async(token_info.token_id, client_ip.clone());
        let request_id = extract_request_id(req.headers());
        let mut response = Json(snapshot.payload).into_response();
        insert_request_id_header(&mut response, &request_id);
        insert_upstream_request_id_header(&mut response, &snapshot.upstream_request_id);
        return Ok(response);
    }

    relay_resource_bodyless_post(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip,
        req,
        format!("/v1/responses/{response_id}/cancel"),
        ResourceRequestSpec {
            endpoint_scope: "responses",
            bind_resource_kind: None,
            delete_resource_kind: None,
        },
        vec![("response", response_id)],
    )
    .await
}

/// GET /v1/files
#[get_api("/v1/files")]
pub async fn list_files(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_resource_get(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        req,
        "/v1/files".into(),
        ResourceRequestSpec {
            endpoint_scope: "files",
            bind_resource_kind: None,
            delete_resource_kind: None,
        },
        Vec::new(),
    )
    .await
}

/// POST /v1/files
#[post_api("/v1/files")]
pub async fn create_file(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    headers: HeaderMap,
    Multipart(multipart): Multipart,
) -> OpenAiApiResult<Response> {
    relay_resource_multipart_post(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        headers,
        multipart,
        "/v1/files".into(),
        ResourceRequestSpec {
            endpoint_scope: "files",
            bind_resource_kind: Some("file"),
            delete_resource_kind: None,
        },
        Vec::new(),
        None,
    )
    .await
}

/// GET /v1/files/{file_id}
#[get_api("/v1/files/{file_id}")]
pub async fn get_file(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    Path(file_id): Path<String>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_resource_get(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        req,
        format!("/v1/files/{file_id}"),
        ResourceRequestSpec {
            endpoint_scope: "files",
            bind_resource_kind: None,
            delete_resource_kind: None,
        },
        vec![("file", file_id)],
    )
    .await
}

/// DELETE /v1/files/{file_id}
#[delete_api("/v1/files/{file_id}")]
pub async fn delete_file(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    Path(file_id): Path<String>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_resource_delete(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        req,
        format!("/v1/files/{file_id}"),
        ResourceRequestSpec {
            endpoint_scope: "files",
            bind_resource_kind: None,
            delete_resource_kind: Some("file"),
        },
        vec![("file", file_id.clone())],
        Some(("file", file_id)),
    )
    .await
}

/// GET /v1/files/{file_id}/content
#[get_api("/v1/files/{file_id}/content")]
pub async fn get_file_content(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    Path(file_id): Path<String>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_resource_get(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        req,
        format!("/v1/files/{file_id}/content"),
        ResourceRequestSpec {
            endpoint_scope: "files",
            bind_resource_kind: None,
            delete_resource_kind: None,
        },
        vec![("file", file_id)],
    )
    .await
}

/// GET /v1/batches
#[get_api("/v1/batches")]
pub async fn list_batches(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_resource_get(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        req,
        "/v1/batches".into(),
        ResourceRequestSpec {
            endpoint_scope: "batches",
            bind_resource_kind: None,
            delete_resource_kind: None,
        },
        Vec::new(),
    )
    .await
}

/// POST /v1/batches
#[post_api("/v1/batches")]
pub async fn create_batch(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> OpenAiApiResult<Response> {
    relay_resource_json_post(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        headers,
        body,
        "/v1/batches".into(),
        ResourceRequestSpec {
            endpoint_scope: "batches",
            bind_resource_kind: Some("batch"),
            delete_resource_kind: None,
        },
        Vec::new(),
        None,
    )
    .await
}

/// GET /v1/batches/{batch_id}
#[get_api("/v1/batches/{batch_id}")]
pub async fn get_batch(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    Path(batch_id): Path<String>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_resource_get(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        req,
        format!("/v1/batches/{batch_id}"),
        ResourceRequestSpec {
            endpoint_scope: "batches",
            bind_resource_kind: None,
            delete_resource_kind: None,
        },
        vec![("batch", batch_id)],
    )
    .await
}

/// POST /v1/batches/{batch_id}/cancel
#[post_api("/v1/batches/{batch_id}/cancel")]
pub async fn cancel_batch(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    Path(batch_id): Path<String>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_resource_bodyless_post(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        req,
        format!("/v1/batches/{batch_id}/cancel"),
        ResourceRequestSpec {
            endpoint_scope: "batches",
            bind_resource_kind: None,
            delete_resource_kind: None,
        },
        vec![("batch", batch_id)],
    )
    .await
}

/// GET /v1/assistants
#[get_api("/v1/assistants")]
pub async fn list_assistants(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_resource_get(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        req,
        "/v1/assistants".into(),
        ResourceRequestSpec {
            endpoint_scope: "assistants",
            bind_resource_kind: None,
            delete_resource_kind: None,
        },
        Vec::new(),
    )
    .await
}

/// POST /v1/assistants
#[post_api("/v1/assistants")]
pub async fn create_assistant(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> OpenAiApiResult<Response> {
    relay_resource_json_post(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        headers,
        body,
        "/v1/assistants".into(),
        ResourceRequestSpec {
            endpoint_scope: "assistants",
            bind_resource_kind: Some("assistant"),
            delete_resource_kind: None,
        },
        Vec::new(),
        None,
    )
    .await
}

/// GET /v1/assistants/{assistant_id}
#[get_api("/v1/assistants/{assistant_id}")]
pub async fn get_assistant(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    Path(assistant_id): Path<String>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_resource_get(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        req,
        format!("/v1/assistants/{assistant_id}"),
        ResourceRequestSpec {
            endpoint_scope: "assistants",
            bind_resource_kind: None,
            delete_resource_kind: None,
        },
        vec![("assistant", assistant_id)],
    )
    .await
}

/// POST /v1/assistants/{assistant_id}
#[post_api("/v1/assistants/{assistant_id}")]
pub async fn update_assistant(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    Path(assistant_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> OpenAiApiResult<Response> {
    relay_resource_json_post(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        headers,
        body,
        format!("/v1/assistants/{assistant_id}"),
        ResourceRequestSpec {
            endpoint_scope: "assistants",
            bind_resource_kind: Some("assistant"),
            delete_resource_kind: None,
        },
        vec![("assistant", assistant_id)],
        None,
    )
    .await
}

/// DELETE /v1/assistants/{assistant_id}
#[delete_api("/v1/assistants/{assistant_id}")]
pub async fn delete_assistant(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    Path(assistant_id): Path<String>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_resource_delete(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        req,
        format!("/v1/assistants/{assistant_id}"),
        ResourceRequestSpec {
            endpoint_scope: "assistants",
            bind_resource_kind: None,
            delete_resource_kind: Some("assistant"),
        },
        vec![("assistant", assistant_id.clone())],
        Some(("assistant", assistant_id)),
    )
    .await
}

/// POST /v1/threads
#[post_api("/v1/threads")]
pub async fn create_thread(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> OpenAiApiResult<Response> {
    relay_resource_json_post(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        headers,
        body,
        "/v1/threads".into(),
        ResourceRequestSpec {
            endpoint_scope: "threads",
            bind_resource_kind: Some("thread"),
            delete_resource_kind: None,
        },
        Vec::new(),
        None,
    )
    .await
}

/// GET /v1/threads/{thread_id}
#[get_api("/v1/threads/{thread_id}")]
pub async fn get_thread(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    Path(thread_id): Path<String>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_resource_get(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        req,
        format!("/v1/threads/{thread_id}"),
        ResourceRequestSpec {
            endpoint_scope: "threads",
            bind_resource_kind: None,
            delete_resource_kind: None,
        },
        vec![("thread", thread_id)],
    )
    .await
}

/// POST /v1/threads/{thread_id}
#[post_api("/v1/threads/{thread_id}")]
pub async fn update_thread(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    Path(thread_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> OpenAiApiResult<Response> {
    relay_resource_json_post(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        headers,
        body,
        format!("/v1/threads/{thread_id}"),
        ResourceRequestSpec {
            endpoint_scope: "threads",
            bind_resource_kind: Some("thread"),
            delete_resource_kind: None,
        },
        vec![("thread", thread_id)],
        None,
    )
    .await
}

/// DELETE /v1/threads/{thread_id}
#[delete_api("/v1/threads/{thread_id}")]
pub async fn delete_thread(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    Path(thread_id): Path<String>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_resource_delete(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        req,
        format!("/v1/threads/{thread_id}"),
        ResourceRequestSpec {
            endpoint_scope: "threads",
            bind_resource_kind: None,
            delete_resource_kind: Some("thread"),
        },
        vec![("thread", thread_id.clone())],
        Some(("thread", thread_id)),
    )
    .await
}

/// GET /v1/threads/{thread_id}/messages
#[get_api("/v1/threads/{thread_id}/messages")]
pub async fn list_thread_messages(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    Path(thread_id): Path<String>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_resource_get(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        req,
        format!("/v1/threads/{thread_id}/messages"),
        ResourceRequestSpec {
            endpoint_scope: "threads",
            bind_resource_kind: None,
            delete_resource_kind: None,
        },
        vec![("thread", thread_id)],
    )
    .await
}

/// POST /v1/threads/{thread_id}/messages
#[post_api("/v1/threads/{thread_id}/messages")]
pub async fn create_thread_message(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    Path(thread_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> OpenAiApiResult<Response> {
    relay_resource_json_post(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        headers,
        body,
        format!("/v1/threads/{thread_id}/messages"),
        ResourceRequestSpec {
            endpoint_scope: "threads",
            bind_resource_kind: Some("message"),
            delete_resource_kind: None,
        },
        vec![("thread", thread_id)],
        None,
    )
    .await
}

/// GET /v1/threads/{thread_id}/messages/{message_id}
#[get_api("/v1/threads/{thread_id}/messages/{message_id}")]
pub async fn get_thread_message(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    Path((thread_id, message_id)): Path<(String, String)>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_resource_get(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        req,
        format!("/v1/threads/{thread_id}/messages/{message_id}"),
        ResourceRequestSpec {
            endpoint_scope: "threads",
            bind_resource_kind: None,
            delete_resource_kind: None,
        },
        vec![("message", message_id), ("thread", thread_id)],
    )
    .await
}

/// POST /v1/threads/{thread_id}/messages/{message_id}
#[post_api("/v1/threads/{thread_id}/messages/{message_id}")]
pub async fn update_thread_message(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    Path((thread_id, message_id)): Path<(String, String)>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> OpenAiApiResult<Response> {
    relay_resource_json_post(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        headers,
        body,
        format!("/v1/threads/{thread_id}/messages/{message_id}"),
        ResourceRequestSpec {
            endpoint_scope: "threads",
            bind_resource_kind: Some("message"),
            delete_resource_kind: None,
        },
        vec![("message", message_id), ("thread", thread_id)],
        None,
    )
    .await
}

/// GET /v1/threads/{thread_id}/runs
#[get_api("/v1/threads/{thread_id}/runs")]
pub async fn list_thread_runs(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    Path(thread_id): Path<String>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_resource_get(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        req,
        format!("/v1/threads/{thread_id}/runs"),
        ResourceRequestSpec {
            endpoint_scope: "threads",
            bind_resource_kind: None,
            delete_resource_kind: None,
        },
        vec![("thread", thread_id)],
    )
    .await
}

/// POST /v1/threads/{thread_id}/runs
#[post_api("/v1/threads/{thread_id}/runs")]
pub async fn create_thread_run(
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
    Path(thread_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> OpenAiApiResult<Response> {
    relay_usage_resource_json_post(
        token_info,
        router_svc,
        billing,
        rate_limiter,
        http_client,
        log_svc,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        headers,
        body,
        format!("/v1/threads/{thread_id}/runs"),
        ResourceRequestSpec {
            endpoint_scope: "threads",
            bind_resource_kind: Some("run"),
            delete_resource_kind: None,
        },
        "threads/runs",
        "openai/threads_runs",
        vec![("thread", thread_id)],
        None,
    )
    .await
}

/// POST /v1/threads/runs
#[post_api("/v1/threads/runs")]
pub async fn create_thread_and_run(
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
    Json(body): Json<Value>,
) -> OpenAiApiResult<Response> {
    relay_usage_resource_json_post(
        token_info,
        router_svc,
        billing,
        rate_limiter,
        http_client,
        log_svc,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        headers,
        body,
        "/v1/threads/runs".into(),
        ResourceRequestSpec {
            endpoint_scope: "threads",
            bind_resource_kind: Some("run"),
            delete_resource_kind: None,
        },
        "threads/runs",
        "openai/threads_runs",
        Vec::new(),
        None,
    )
    .await
}

/// GET /v1/threads/{thread_id}/runs/{run_id}
#[get_api("/v1/threads/{thread_id}/runs/{run_id}")]
pub async fn get_thread_run(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    Path((thread_id, run_id)): Path<(String, String)>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_resource_get(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        req,
        format!("/v1/threads/{thread_id}/runs/{run_id}"),
        ResourceRequestSpec {
            endpoint_scope: "threads",
            bind_resource_kind: None,
            delete_resource_kind: None,
        },
        vec![("run", run_id), ("thread", thread_id)],
    )
    .await
}

/// POST /v1/threads/{thread_id}/runs/{run_id}
#[post_api("/v1/threads/{thread_id}/runs/{run_id}")]
pub async fn update_thread_run(
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
    Path((thread_id, run_id)): Path<(String, String)>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> OpenAiApiResult<Response> {
    relay_usage_resource_json_post(
        token_info,
        router_svc,
        billing,
        rate_limiter,
        http_client,
        log_svc,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        headers,
        body,
        format!("/v1/threads/{thread_id}/runs/{run_id}"),
        ResourceRequestSpec {
            endpoint_scope: "threads",
            bind_resource_kind: Some("run"),
            delete_resource_kind: None,
        },
        "threads/runs",
        "openai/threads_runs",
        vec![("run", run_id), ("thread", thread_id)],
        None,
    )
    .await
}

/// POST /v1/threads/{thread_id}/runs/{run_id}/submit_tool_outputs
#[post_api("/v1/threads/{thread_id}/runs/{run_id}/submit_tool_outputs")]
pub async fn submit_thread_run_tool_outputs(
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
    Path((thread_id, run_id)): Path<(String, String)>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> OpenAiApiResult<Response> {
    relay_usage_resource_json_post(
        token_info,
        router_svc,
        billing,
        rate_limiter,
        http_client,
        log_svc,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        headers,
        body,
        format!("/v1/threads/{thread_id}/runs/{run_id}/submit_tool_outputs"),
        ResourceRequestSpec {
            endpoint_scope: "threads",
            bind_resource_kind: Some("run"),
            delete_resource_kind: None,
        },
        "threads/runs/submit_tool_outputs",
        "openai/threads_runs_submit_tool_outputs",
        vec![("run", run_id), ("thread", thread_id)],
        None,
    )
    .await
}

/// POST /v1/threads/{thread_id}/runs/{run_id}/cancel
#[post_api("/v1/threads/{thread_id}/runs/{run_id}/cancel")]
pub async fn cancel_thread_run(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    Path((thread_id, run_id)): Path<(String, String)>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_resource_bodyless_post(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        req,
        format!("/v1/threads/{thread_id}/runs/{run_id}/cancel"),
        ResourceRequestSpec {
            endpoint_scope: "threads",
            bind_resource_kind: None,
            delete_resource_kind: None,
        },
        vec![("run", run_id), ("thread", thread_id)],
    )
    .await
}

/// GET /v1/threads/{thread_id}/runs/{run_id}/steps
#[get_api("/v1/threads/{thread_id}/runs/{run_id}/steps")]
pub async fn list_thread_run_steps(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    Path((thread_id, run_id)): Path<(String, String)>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_resource_get(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        req,
        format!("/v1/threads/{thread_id}/runs/{run_id}/steps"),
        ResourceRequestSpec {
            endpoint_scope: "threads",
            bind_resource_kind: None,
            delete_resource_kind: None,
        },
        vec![("run", run_id), ("thread", thread_id)],
    )
    .await
}

/// GET /v1/threads/{thread_id}/runs/{run_id}/steps/{step_id}
#[get_api("/v1/threads/{thread_id}/runs/{run_id}/steps/{step_id}")]
pub async fn get_thread_run_step(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    Path((thread_id, run_id, step_id)): Path<(String, String, String)>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_resource_get(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        req,
        format!("/v1/threads/{thread_id}/runs/{run_id}/steps/{step_id}"),
        ResourceRequestSpec {
            endpoint_scope: "threads",
            bind_resource_kind: None,
            delete_resource_kind: None,
        },
        vec![("run", run_id), ("thread", thread_id)],
    )
    .await
}

/// GET /v1/vector_stores
#[get_api("/v1/vector_stores")]
pub async fn list_vector_stores(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_resource_get(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        req,
        "/v1/vector_stores".into(),
        ResourceRequestSpec {
            endpoint_scope: "vector_stores",
            bind_resource_kind: None,
            delete_resource_kind: None,
        },
        Vec::new(),
    )
    .await
}

/// POST /v1/vector_stores
#[post_api("/v1/vector_stores")]
pub async fn create_vector_store(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> OpenAiApiResult<Response> {
    relay_resource_json_post(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        headers,
        body,
        "/v1/vector_stores".into(),
        ResourceRequestSpec {
            endpoint_scope: "vector_stores",
            bind_resource_kind: Some("vector_store"),
            delete_resource_kind: None,
        },
        Vec::new(),
        None,
    )
    .await
}

/// GET /v1/vector_stores/{vector_store_id}
#[get_api("/v1/vector_stores/{vector_store_id}")]
pub async fn get_vector_store(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    Path(vector_store_id): Path<String>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_resource_get(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        req,
        format!("/v1/vector_stores/{vector_store_id}"),
        ResourceRequestSpec {
            endpoint_scope: "vector_stores",
            bind_resource_kind: None,
            delete_resource_kind: None,
        },
        vec![("vector_store", vector_store_id)],
    )
    .await
}

/// POST /v1/vector_stores/{vector_store_id}
#[post_api("/v1/vector_stores/{vector_store_id}")]
pub async fn update_vector_store(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    Path(vector_store_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> OpenAiApiResult<Response> {
    relay_resource_json_post(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        headers,
        body,
        format!("/v1/vector_stores/{vector_store_id}"),
        ResourceRequestSpec {
            endpoint_scope: "vector_stores",
            bind_resource_kind: Some("vector_store"),
            delete_resource_kind: None,
        },
        vec![("vector_store", vector_store_id)],
        None,
    )
    .await
}

/// DELETE /v1/vector_stores/{vector_store_id}
#[delete_api("/v1/vector_stores/{vector_store_id}")]
pub async fn delete_vector_store(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    Path(vector_store_id): Path<String>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_resource_delete(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        req,
        format!("/v1/vector_stores/{vector_store_id}"),
        ResourceRequestSpec {
            endpoint_scope: "vector_stores",
            bind_resource_kind: None,
            delete_resource_kind: Some("vector_store"),
        },
        vec![("vector_store", vector_store_id.clone())],
        Some(("vector_store", vector_store_id)),
    )
    .await
}

/// POST /v1/vector_stores/{vector_store_id}/search
#[post_api("/v1/vector_stores/{vector_store_id}/search")]
pub async fn search_vector_store(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    Path(vector_store_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> OpenAiApiResult<Response> {
    relay_resource_json_post(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        headers,
        body,
        format!("/v1/vector_stores/{vector_store_id}/search"),
        ResourceRequestSpec {
            endpoint_scope: "vector_stores",
            bind_resource_kind: None,
            delete_resource_kind: None,
        },
        vec![("vector_store", vector_store_id)],
        None,
    )
    .await
}

/// GET /v1/vector_stores/{vector_store_id}/files
#[get_api("/v1/vector_stores/{vector_store_id}/files")]
pub async fn list_vector_store_files(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    Path(vector_store_id): Path<String>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_resource_get(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        req,
        format!("/v1/vector_stores/{vector_store_id}/files"),
        ResourceRequestSpec {
            endpoint_scope: "vector_stores",
            bind_resource_kind: None,
            delete_resource_kind: None,
        },
        vec![("vector_store", vector_store_id)],
    )
    .await
}

/// POST /v1/vector_stores/{vector_store_id}/files
#[post_api("/v1/vector_stores/{vector_store_id}/files")]
pub async fn create_vector_store_file(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    Path(vector_store_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> OpenAiApiResult<Response> {
    relay_resource_json_post(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        headers,
        body,
        format!("/v1/vector_stores/{vector_store_id}/files"),
        ResourceRequestSpec {
            endpoint_scope: "vector_stores",
            bind_resource_kind: Some("file"),
            delete_resource_kind: None,
        },
        vec![("vector_store", vector_store_id)],
        None,
    )
    .await
}

/// GET /v1/vector_stores/{vector_store_id}/files/{file_id}
#[get_api("/v1/vector_stores/{vector_store_id}/files/{file_id}")]
pub async fn get_vector_store_file(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    Path((vector_store_id, file_id)): Path<(String, String)>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_resource_get(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        req,
        format!("/v1/vector_stores/{vector_store_id}/files/{file_id}"),
        ResourceRequestSpec {
            endpoint_scope: "vector_stores",
            bind_resource_kind: None,
            delete_resource_kind: None,
        },
        vec![("file", file_id), ("vector_store", vector_store_id)],
    )
    .await
}

/// DELETE /v1/vector_stores/{vector_store_id}/files/{file_id}
#[delete_api("/v1/vector_stores/{vector_store_id}/files/{file_id}")]
pub async fn delete_vector_store_file(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    Path((vector_store_id, file_id)): Path<(String, String)>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_resource_delete(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        req,
        format!("/v1/vector_stores/{vector_store_id}/files/{file_id}"),
        ResourceRequestSpec {
            endpoint_scope: "vector_stores",
            bind_resource_kind: None,
            delete_resource_kind: None,
        },
        vec![("file", file_id), ("vector_store", vector_store_id)],
        None,
    )
    .await
}

/// GET /v1/vector_stores/{vector_store_id}/file_batches
#[get_api("/v1/vector_stores/{vector_store_id}/file_batches")]
pub async fn list_vector_store_file_batches(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    Path(vector_store_id): Path<String>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_resource_get(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        req,
        format!("/v1/vector_stores/{vector_store_id}/file_batches"),
        ResourceRequestSpec {
            endpoint_scope: "vector_stores",
            bind_resource_kind: None,
            delete_resource_kind: None,
        },
        vec![("vector_store", vector_store_id)],
    )
    .await
}

/// POST /v1/vector_stores/{vector_store_id}/file_batches
#[post_api("/v1/vector_stores/{vector_store_id}/file_batches")]
pub async fn create_vector_store_file_batch(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    Path(vector_store_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> OpenAiApiResult<Response> {
    relay_resource_json_post(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        headers,
        body,
        format!("/v1/vector_stores/{vector_store_id}/file_batches"),
        ResourceRequestSpec {
            endpoint_scope: "vector_stores",
            bind_resource_kind: Some("batch"),
            delete_resource_kind: None,
        },
        vec![("vector_store", vector_store_id)],
        None,
    )
    .await
}

/// GET /v1/vector_stores/{vector_store_id}/file_batches/{batch_id}
#[get_api("/v1/vector_stores/{vector_store_id}/file_batches/{batch_id}")]
pub async fn get_vector_store_file_batch(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    Path((vector_store_id, batch_id)): Path<(String, String)>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_resource_get(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        req,
        format!("/v1/vector_stores/{vector_store_id}/file_batches/{batch_id}"),
        ResourceRequestSpec {
            endpoint_scope: "vector_stores",
            bind_resource_kind: None,
            delete_resource_kind: None,
        },
        vec![("batch", batch_id), ("vector_store", vector_store_id)],
    )
    .await
}

/// POST /v1/vector_stores/{vector_store_id}/file_batches/{batch_id}/cancel
#[post_api("/v1/vector_stores/{vector_store_id}/file_batches/{batch_id}/cancel")]
pub async fn cancel_vector_store_file_batch(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    Path((vector_store_id, batch_id)): Path<(String, String)>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_resource_bodyless_post(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        req,
        format!("/v1/vector_stores/{vector_store_id}/file_batches/{batch_id}/cancel"),
        ResourceRequestSpec {
            endpoint_scope: "vector_stores",
            bind_resource_kind: None,
            delete_resource_kind: None,
        },
        vec![("batch", batch_id), ("vector_store", vector_store_id)],
    )
    .await
}

/// GET /v1/fine_tuning/jobs
#[get_api("/v1/fine_tuning/jobs")]
pub async fn list_fine_tuning_jobs(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_resource_get(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        req,
        "/v1/fine_tuning/jobs".into(),
        ResourceRequestSpec {
            endpoint_scope: "fine_tuning",
            bind_resource_kind: None,
            delete_resource_kind: None,
        },
        Vec::new(),
    )
    .await
}

/// POST /v1/fine_tuning/jobs
#[post_api("/v1/fine_tuning/jobs")]
pub async fn create_fine_tuning_job(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> OpenAiApiResult<Response> {
    relay_resource_json_post(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        headers,
        body,
        "/v1/fine_tuning/jobs".into(),
        ResourceRequestSpec {
            endpoint_scope: "fine_tuning",
            bind_resource_kind: Some("fine_tuning_job"),
            delete_resource_kind: None,
        },
        Vec::new(),
        None,
    )
    .await
}

/// GET /v1/fine_tuning/jobs/{job_id}
#[get_api("/v1/fine_tuning/jobs/{job_id}")]
pub async fn get_fine_tuning_job(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    Path(job_id): Path<String>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_resource_get(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        req,
        format!("/v1/fine_tuning/jobs/{job_id}"),
        ResourceRequestSpec {
            endpoint_scope: "fine_tuning",
            bind_resource_kind: None,
            delete_resource_kind: None,
        },
        vec![("fine_tuning_job", job_id)],
    )
    .await
}

/// POST /v1/fine_tuning/jobs/{job_id}/cancel
#[post_api("/v1/fine_tuning/jobs/{job_id}/cancel")]
pub async fn cancel_fine_tuning_job(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    Path(job_id): Path<String>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_resource_bodyless_post(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        req,
        format!("/v1/fine_tuning/jobs/{job_id}/cancel"),
        ResourceRequestSpec {
            endpoint_scope: "fine_tuning",
            bind_resource_kind: None,
            delete_resource_kind: None,
        },
        vec![("fine_tuning_job", job_id)],
    )
    .await
}

/// GET /v1/fine_tuning/jobs/{job_id}/events
#[get_api("/v1/fine_tuning/jobs/{job_id}/events")]
pub async fn list_fine_tuning_events(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    Path(job_id): Path<String>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_resource_get(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        req,
        format!("/v1/fine_tuning/jobs/{job_id}/events"),
        ResourceRequestSpec {
            endpoint_scope: "fine_tuning",
            bind_resource_kind: None,
            delete_resource_kind: None,
        },
        vec![("fine_tuning_job", job_id)],
    )
    .await
}

/// GET /v1/fine_tuning/jobs/{job_id}/checkpoints
#[get_api("/v1/fine_tuning/jobs/{job_id}/checkpoints")]
pub async fn list_fine_tuning_checkpoints(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    Path(job_id): Path<String>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_resource_get(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        req,
        format!("/v1/fine_tuning/jobs/{job_id}/checkpoints"),
        ResourceRequestSpec {
            endpoint_scope: "fine_tuning",
            bind_resource_kind: None,
            delete_resource_kind: None,
        },
        vec![("fine_tuning_job", job_id)],
    )
    .await
}

/// POST /v1/uploads
#[post_api("/v1/uploads")]
pub async fn create_upload(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> OpenAiApiResult<Response> {
    relay_resource_json_post(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        headers,
        body,
        "/v1/uploads".into(),
        ResourceRequestSpec {
            endpoint_scope: "uploads",
            bind_resource_kind: Some("upload"),
            delete_resource_kind: None,
        },
        Vec::new(),
        None,
    )
    .await
}

/// GET /v1/uploads/{upload_id}
#[get_api("/v1/uploads/{upload_id}")]
pub async fn get_upload(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    Path(upload_id): Path<String>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_resource_get(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        req,
        format!("/v1/uploads/{upload_id}"),
        ResourceRequestSpec {
            endpoint_scope: "uploads",
            bind_resource_kind: None,
            delete_resource_kind: None,
        },
        vec![("upload", upload_id)],
    )
    .await
}

/// POST /v1/uploads/{upload_id}/parts
#[post_api("/v1/uploads/{upload_id}/parts")]
pub async fn add_upload_part(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    Path(upload_id): Path<String>,
    headers: HeaderMap,
    Multipart(multipart): Multipart,
) -> OpenAiApiResult<Response> {
    relay_resource_multipart_post(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        headers,
        multipart,
        format!("/v1/uploads/{upload_id}/parts"),
        ResourceRequestSpec {
            endpoint_scope: "uploads",
            bind_resource_kind: None,
            delete_resource_kind: None,
        },
        vec![("upload", upload_id)],
        None,
    )
    .await
}

/// POST /v1/uploads/{upload_id}/complete
#[post_api("/v1/uploads/{upload_id}/complete")]
pub async fn complete_upload(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    Path(upload_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> OpenAiApiResult<Response> {
    relay_resource_json_post(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        headers,
        body,
        format!("/v1/uploads/{upload_id}/complete"),
        ResourceRequestSpec {
            endpoint_scope: "uploads",
            bind_resource_kind: Some("file"),
            delete_resource_kind: None,
        },
        vec![("upload", upload_id)],
        None,
    )
    .await
}

/// POST /v1/uploads/{upload_id}/cancel
#[post_api("/v1/uploads/{upload_id}/cancel")]
pub async fn cancel_upload(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    Path(upload_id): Path<String>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_resource_bodyless_post(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        req,
        format!("/v1/uploads/{upload_id}/cancel"),
        ResourceRequestSpec {
            endpoint_scope: "uploads",
            bind_resource_kind: None,
            delete_resource_kind: None,
        },
        vec![("upload", upload_id)],
    )
    .await
}

/// DELETE /v1/models/{model}
#[delete_api("/v1/models/{model}")]
pub async fn delete_model(
    AiToken(token_info): AiToken,
    Component(router_svc): Component<ChannelRouter>,
    Component(http_client): Component<UpstreamHttpClient>,
    Component(channel_svc): Component<ChannelService>,
    Component(token_svc): Component<TokenService>,
    Component(resource_affinity): Component<ResourceAffinityService>,
    ClientIp(client_ip): ClientIp,
    Path(model): Path<String>,
    req: Request,
) -> OpenAiApiResult<Response> {
    relay_resource_delete(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip.to_string(),
        req,
        format!("/v1/models/{model}"),
        ResourceRequestSpec {
            endpoint_scope: "models",
            bind_resource_kind: None,
            delete_resource_kind: None,
        },
        Vec::new(),
        None,
    )
    .await
}

async fn relay_json_model_request(
    token_info: TokenInfo,
    router_svc: ChannelRouter,
    billing: BillingEngine,
    rate_limiter: RateLimitEngine,
    http_client: UpstreamHttpClient,
    log_svc: LogService,
    channel_svc: ChannelService,
    token_svc: TokenService,
    resource_affinity: ResourceAffinityService,
    client_ip: String,
    headers: HeaderMap,
    mut body: Value,
    spec: JsonModelRelaySpec,
) -> OpenAiApiResult<Response> {
    let request_id = extract_request_id(&headers);
    let user_agent = UserAgentInfo::from_headers(&headers).raw;
    let requested_model = model_from_json_body(&body, spec.default_model)
        .ok_or_else(|| OpenAiErrorResponse::invalid_request("missing model"))?;
    token_info
        .ensure_endpoint_allowed(spec.endpoint_scope)
        .map_err(|error| OpenAiErrorResponse::from_api_error(&error))?;
    token_info
        .ensure_model_allowed(&requested_model)
        .map_err(|error| OpenAiErrorResponse::from_api_error(&error))?;
    ensure_json_model(&mut body, &requested_model)?;
    token_svc.update_last_used_ip_async(token_info.token_id, client_ip.clone());

    let model_config = billing
        .get_model_config_for_endpoint(&requested_model, spec.endpoint_scope)
        .await
        .map_err(|error| OpenAiErrorResponse::from_api_error(&error))?;
    let group_ratio = billing
        .get_group_ratio(&token_info.group)
        .await
        .map_err(|error| {
            OpenAiErrorResponse::internal_with("failed to load group pricing", error)
        })?;

    let estimated_tokens = estimate_json_tokens(&body);
    let estimated_total_tokens = estimate_total_tokens_for_rate_limit(&body);
    let is_stream = body.get("stream").and_then(Value::as_bool).unwrap_or(false);

    rate_limiter
        .reserve(&token_info, &request_id, estimated_total_tokens)
        .await
        .map_err(|error| OpenAiErrorResponse::from_quota_error(&error))?;

    let route_exclusions = RouteSelectionExclusions::default();
    let mut route_plan = router_svc
        .build_channel_plan_with_exclusions(
            &token_info.group,
            &requested_model,
            spec.endpoint_scope,
            &route_exclusions,
        )
        .await
        .map_err(|error| {
            OpenAiErrorResponse::internal_with("failed to build channel plan", error)
        })?;
    let max_retries = 3;
    let start = std::time::Instant::now();

    for attempt in 0..max_retries {
        let Some(channel) = route_plan.next() else {
            let _ = rate_limiter.finalize_failure_with_retry(&request_id).await;
            return Err(OpenAiErrorResponse::no_available_channel(
                "no available channel",
            ));
        };

        let actual_model = mapped_model(&channel, &requested_model);
        ensure_json_model(&mut body, &actual_model)?;

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

        let mut request_builder = http_client
            .client()
            .post(build_upstream_url(
                &channel.base_url,
                spec.upstream_path,
                None,
            ))
            .json(&body);
        request_builder =
            apply_upstream_auth(request_builder, channel.channel_type, &channel.api_key);
        request_builder = apply_forward_headers(request_builder, &headers, false);

        match request_builder.send().await {
            Ok(resp) if resp.status().is_success() => {
                let elapsed = start.elapsed().as_millis() as i64;
                let upstream_request_id = extract_upstream_request_id(resp.headers());

                if is_stream {
                    return Ok(build_generic_stream_response(
                        resp,
                        token_info,
                        pre_consumed,
                        Some(model_config),
                        group_ratio,
                        channel,
                        Some(requested_model),
                        estimated_tokens,
                        spec.endpoint,
                        spec.request_format,
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
                        None,
                        spec.endpoint_scope,
                    ));
                }

                let status = resp.status();
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
                            format!("failed to read upstream response: {error}"),
                        );
                        route_plan.exclude_selected_account(&channel);
                        if attempt == max_retries - 1 {
                            record_passthrough_failure(
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
                                is_stream,
                                &client_ip,
                                &user_agent,
                                0,
                                format!("failed to read upstream response: {error}"),
                            );
                            let _ = rate_limiter.finalize_failure_with_retry(&request_id).await;
                            return Err(OpenAiErrorResponse::internal_with(
                                "failed to read upstream response",
                                error,
                            ));
                        }
                        continue;
                    }
                };
                let json_value = serde_json::from_slice::<Value>(&body_bytes).ok();
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
                        record_passthrough_failure(
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
                            is_stream,
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
                let usage = json_value
                    .as_ref()
                    .and_then(extract_usage_from_value)
                    .unwrap_or_else(|| fallback_usage(estimated_tokens));
                let upstream_model = json_value
                    .as_ref()
                    .and_then(extract_model_from_response_value)
                    .unwrap_or(actual_model.clone());

                spawn_resource_usage_accounting_task(
                    billing,
                    rate_limiter,
                    log_svc,
                    channel_svc,
                    token_info,
                    channel,
                    Some(model_config),
                    group_ratio,
                    pre_consumed,
                    usage,
                    request_id.clone(),
                    upstream_request_id.clone(),
                    Some(requested_model),
                    upstream_model,
                    client_ip,
                    user_agent,
                    spec.endpoint,
                    spec.request_format,
                    elapsed,
                    0,
                    false,
                    spec.endpoint_scope,
                );

                let mut response =
                    build_bytes_response(status, body_bytes, content_type, &request_id);
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
                    record_passthrough_failure(
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
                    record_passthrough_failure(
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
                        is_stream,
                        &client_ip,
                        &user_agent,
                        0,
                        error.to_string(),
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

async fn relay_multipart_model_request(
    token_info: TokenInfo,
    router_svc: ChannelRouter,
    billing: BillingEngine,
    rate_limiter: RateLimitEngine,
    http_client: UpstreamHttpClient,
    log_svc: LogService,
    channel_svc: ChannelService,
    token_svc: TokenService,
    _resource_affinity: ResourceAffinityService,
    client_ip: String,
    headers: HeaderMap,
    multipart: summer_web::axum::extract::Multipart,
    spec: MultipartModelRelaySpec,
) -> OpenAiApiResult<Response> {
    let request_id = extract_request_id(&headers);
    let user_agent = UserAgentInfo::from_headers(&headers).raw;
    let payload = parse_multipart_payload(multipart).await.map_err(|error| {
        OpenAiErrorResponse::internal_with("failed to parse multipart body", error)
    })?;
    let requested_model = payload
        .model
        .clone()
        .or_else(|| spec.default_model.map(ToOwned::to_owned))
        .ok_or_else(|| OpenAiErrorResponse::invalid_request("missing model"))?;

    token_info
        .ensure_endpoint_allowed(spec.endpoint_scope)
        .map_err(|error| OpenAiErrorResponse::from_api_error(&error))?;
    token_info
        .ensure_model_allowed(&requested_model)
        .map_err(|error| OpenAiErrorResponse::from_api_error(&error))?;
    token_svc.update_last_used_ip_async(token_info.token_id, client_ip.clone());

    let model_config = billing
        .get_model_config_for_endpoint(&requested_model, spec.endpoint_scope)
        .await
        .map_err(|error| OpenAiErrorResponse::from_api_error(&error))?;
    let group_ratio = billing
        .get_group_ratio(&token_info.group)
        .await
        .map_err(|error| {
            OpenAiErrorResponse::internal_with("failed to load group pricing", error)
        })?;

    let estimated_tokens = payload.estimated_tokens.max(1);
    rate_limiter
        .reserve(&token_info, &request_id, i64::from(estimated_tokens))
        .await
        .map_err(|error| OpenAiErrorResponse::from_quota_error(&error))?;

    let route_exclusions = RouteSelectionExclusions::default();
    let mut route_plan = router_svc
        .build_channel_plan_with_exclusions(
            &token_info.group,
            &requested_model,
            spec.endpoint_scope,
            &route_exclusions,
        )
        .await
        .map_err(|error| {
            OpenAiErrorResponse::internal_with("failed to build channel plan", error)
        })?;
    let max_retries = 3;
    let start = std::time::Instant::now();

    for attempt in 0..max_retries {
        let Some(channel) = route_plan.next() else {
            let _ = rate_limiter.finalize_failure_with_retry(&request_id).await;
            return Err(OpenAiErrorResponse::no_available_channel(
                "no available channel",
            ));
        };

        let actual_model = mapped_model(&channel, &requested_model);
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

        let form = match payload.to_form(&actual_model) {
            Ok(form) => form,
            Err(error) => {
                let _ = billing
                    .refund_with_retry(&request_id, token_info.token_id, pre_consumed)
                    .await;
                let _ = rate_limiter.finalize_failure_with_retry(&request_id).await;
                return Err(error);
            }
        };

        let mut request_builder = http_client
            .client()
            .post(build_upstream_url(
                &channel.base_url,
                spec.upstream_path,
                None,
            ))
            .multipart(form);
        request_builder =
            apply_upstream_auth(request_builder, channel.channel_type, &channel.api_key);
        request_builder = apply_forward_headers(request_builder, &headers, false);

        match request_builder.send().await {
            Ok(resp) if resp.status().is_success() => {
                let elapsed = start.elapsed().as_millis() as i64;
                let status = resp.status();
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
                            format!("failed to read upstream response: {error}"),
                        );
                        route_plan.exclude_selected_account(&channel);
                        if attempt == max_retries - 1 {
                            record_passthrough_failure(
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
                                format!("failed to read upstream response: {error}"),
                            );
                            let _ = rate_limiter.finalize_failure_with_retry(&request_id).await;
                            return Err(OpenAiErrorResponse::internal_with(
                                "failed to read upstream response",
                                error,
                            ));
                        }
                        continue;
                    }
                };
                let json_value = serde_json::from_slice::<Value>(&body_bytes).ok();
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
                        record_passthrough_failure(
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
                let usage = json_value
                    .as_ref()
                    .and_then(extract_usage_from_value)
                    .unwrap_or_else(|| fallback_usage(estimated_tokens));
                let upstream_model = json_value
                    .as_ref()
                    .and_then(extract_model_from_response_value)
                    .unwrap_or(actual_model.clone());

                spawn_resource_usage_accounting_task(
                    billing,
                    rate_limiter,
                    log_svc,
                    channel_svc,
                    token_info,
                    channel,
                    Some(model_config),
                    group_ratio,
                    pre_consumed,
                    usage,
                    request_id.clone(),
                    upstream_request_id.clone(),
                    Some(requested_model),
                    upstream_model,
                    client_ip,
                    user_agent,
                    spec.endpoint,
                    spec.request_format,
                    elapsed,
                    0,
                    false,
                    spec.endpoint_scope,
                );

                let mut response =
                    build_bytes_response(status, body_bytes, content_type, &request_id);
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
                    record_passthrough_failure(
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
                    record_passthrough_failure(
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
                        error.to_string(),
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

async fn relay_resource_get(
    token_info: TokenInfo,
    router_svc: ChannelRouter,
    http_client: UpstreamHttpClient,
    channel_svc: ChannelService,
    token_svc: TokenService,
    resource_affinity: ResourceAffinityService,
    client_ip: String,
    req: Request,
    upstream_path: String,
    spec: ResourceRequestSpec,
    affinity_keys: Vec<(&'static str, String)>,
) -> OpenAiApiResult<Response> {
    relay_resource_request(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip,
        req.headers().clone(),
        req.uri().query().map(ToOwned::to_owned),
        Method::GET,
        upstream_path,
        spec,
        affinity_keys,
        None,
        None,
        None,
    )
    .await
}

async fn relay_resource_delete(
    token_info: TokenInfo,
    router_svc: ChannelRouter,
    http_client: UpstreamHttpClient,
    channel_svc: ChannelService,
    token_svc: TokenService,
    resource_affinity: ResourceAffinityService,
    client_ip: String,
    req: Request,
    upstream_path: String,
    spec: ResourceRequestSpec,
    affinity_keys: Vec<(&'static str, String)>,
    delete_affinity: Option<(&'static str, String)>,
) -> OpenAiApiResult<Response> {
    relay_resource_request(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip,
        req.headers().clone(),
        req.uri().query().map(ToOwned::to_owned),
        Method::DELETE,
        upstream_path,
        spec,
        affinity_keys,
        None,
        None,
        delete_affinity,
    )
    .await
}

async fn relay_resource_bodyless_post(
    token_info: TokenInfo,
    router_svc: ChannelRouter,
    http_client: UpstreamHttpClient,
    channel_svc: ChannelService,
    token_svc: TokenService,
    resource_affinity: ResourceAffinityService,
    client_ip: String,
    req: Request,
    upstream_path: String,
    spec: ResourceRequestSpec,
    affinity_keys: Vec<(&'static str, String)>,
) -> OpenAiApiResult<Response> {
    relay_resource_request(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip,
        req.headers().clone(),
        req.uri().query().map(ToOwned::to_owned),
        Method::POST,
        upstream_path,
        spec,
        affinity_keys,
        None,
        None,
        None,
    )
    .await
}

async fn relay_resource_json_post(
    token_info: TokenInfo,
    router_svc: ChannelRouter,
    http_client: UpstreamHttpClient,
    channel_svc: ChannelService,
    token_svc: TokenService,
    resource_affinity: ResourceAffinityService,
    client_ip: String,
    headers: HeaderMap,
    mut body: Value,
    upstream_path: String,
    spec: ResourceRequestSpec,
    affinity_keys: Vec<(&'static str, String)>,
    delete_affinity: Option<(&'static str, String)>,
) -> OpenAiApiResult<Response> {
    let requested_model = model_from_json_body(&body, None);
    if let Some(model) = requested_model.as_ref() {
        token_info
            .ensure_model_allowed(model)
            .map_err(|error| OpenAiErrorResponse::from_api_error(&error))?;
    }

    relay_resource_request(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip,
        headers,
        None,
        Method::POST,
        upstream_path,
        spec,
        affinity_keys,
        Some(&mut body),
        None,
        delete_affinity,
    )
    .await
}

async fn relay_usage_resource_json_post(
    token_info: TokenInfo,
    router_svc: ChannelRouter,
    billing: BillingEngine,
    rate_limiter: RateLimitEngine,
    http_client: UpstreamHttpClient,
    log_svc: LogService,
    channel_svc: ChannelService,
    token_svc: TokenService,
    resource_affinity: ResourceAffinityService,
    client_ip: String,
    headers: HeaderMap,
    mut body: Value,
    upstream_path: String,
    spec: ResourceRequestSpec,
    endpoint: &'static str,
    request_format: &'static str,
    affinity_keys: Vec<(&'static str, String)>,
    delete_affinity: Option<(&'static str, String)>,
) -> OpenAiApiResult<Response> {
    let requested_model = model_from_json_body(&body, None);
    let request_id = extract_request_id(&headers);
    let user_agent = UserAgentInfo::from_headers(&headers).raw;
    token_info
        .ensure_endpoint_allowed(spec.endpoint_scope)
        .map_err(|error| OpenAiErrorResponse::from_api_error(&error))?;
    if let Some(requested_model) = requested_model.as_ref() {
        token_info
            .ensure_model_allowed(requested_model)
            .map_err(|error| OpenAiErrorResponse::from_api_error(&error))?;
    }
    token_svc.update_last_used_ip_async(token_info.token_id, client_ip.clone());

    let model_config = if let Some(requested_model) = requested_model.as_ref() {
        Some(
            billing
                .get_model_config_for_endpoint(requested_model, spec.endpoint_scope)
                .await
                .map_err(|error| OpenAiErrorResponse::from_api_error(&error))?,
        )
    } else {
        None
    };
    let group_ratio = billing
        .get_group_ratio(&token_info.group)
        .await
        .map_err(|error| {
            OpenAiErrorResponse::internal_with("failed to load group pricing", error)
        })?;

    let estimated_tokens = estimate_json_tokens(&body);
    let estimated_total_tokens = estimate_total_tokens_for_rate_limit(&body);
    let is_stream = json_body_requests_stream(&body);

    rate_limiter
        .reserve(&token_info, &request_id, estimated_total_tokens)
        .await
        .map_err(|error| OpenAiErrorResponse::from_quota_error(&error))?;

    let mut route_state = ResourceRouteState::new(
        &token_info,
        &router_svc,
        spec.endpoint_scope,
        requested_model.as_deref(),
    )
    .await?;
    let max_retries = 3;
    let start = std::time::Instant::now();

    for attempt in 0..max_retries {
        let Some(channel) = route_state
            .select(&token_info, &resource_affinity, &affinity_keys, Some(&body))
            .await?
        else {
            let _ = rate_limiter.finalize_failure_with_retry(&request_id).await;
            return Err(OpenAiErrorResponse::no_available_channel(if attempt == 0 {
                "no available channel"
            } else {
                "all channels failed"
            }));
        };

        let actual_model = requested_model
            .as_ref()
            .map(|requested_model| mapped_model(&channel, requested_model));
        if let Some(actual_model) = actual_model.as_ref() {
            ensure_json_model(&mut body, actual_model)?;
        }

        let pre_consumed = if let Some(model_config) = model_config.as_ref() {
            match billing
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
            }
        } else {
            0
        };

        let mut request_builder = http_client
            .client()
            .post(build_upstream_url(&channel.base_url, &upstream_path, None))
            .json(&body);
        request_builder =
            apply_upstream_auth(request_builder, channel.channel_type, &channel.api_key);
        request_builder = apply_forward_headers(request_builder, &headers, false);

        match request_builder.send().await {
            Ok(resp) if resp.status().is_success() => {
                let elapsed = start.elapsed().as_millis() as i64;
                let upstream_request_id = extract_upstream_request_id(resp.headers());

                if is_stream {
                    return Ok(build_generic_stream_response(
                        resp,
                        token_info,
                        pre_consumed,
                        model_config.clone(),
                        group_ratio,
                        channel,
                        requested_model.clone(),
                        estimated_tokens,
                        endpoint,
                        request_format,
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
                        spec.bind_resource_kind,
                        spec.endpoint_scope,
                    ));
                }

                let status = resp.status();
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
                            format!("failed to read upstream response: {error}"),
                        );
                        route_state.exclude_selected_account(&channel);
                        if attempt == max_retries - 1 {
                            record_passthrough_failure(
                                &log_svc,
                                &token_info,
                                &channel,
                                endpoint,
                                request_format,
                                requested_model.as_deref().unwrap_or_default(),
                                actual_model.as_deref().unwrap_or_default(),
                                model_config
                                    .as_ref()
                                    .map(|config| config.model_name.as_str())
                                    .unwrap_or_else(|| {
                                        requested_model.as_deref().unwrap_or_default()
                                    }),
                                &request_id,
                                &upstream_request_id,
                                elapsed,
                                is_stream,
                                &client_ip,
                                &user_agent,
                                0,
                                format!("failed to read upstream response: {error}"),
                            );
                            let _ = rate_limiter.finalize_failure_with_retry(&request_id).await;
                            return Err(OpenAiErrorResponse::internal_with(
                                "failed to read upstream response",
                                error,
                            ));
                        }
                        continue;
                    }
                };
                let json_value = serde_json::from_slice::<Value>(&body_bytes).ok();
                if let Some(message) =
                    unusable_success_response_message(status, &body_bytes, endpoint, false)
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
                    route_state.exclude_selected_channel(&channel);
                    if attempt == max_retries - 1 {
                        record_passthrough_failure(
                            &log_svc,
                            &token_info,
                            &channel,
                            endpoint,
                            request_format,
                            requested_model.as_deref().unwrap_or_default(),
                            actual_model.as_deref().unwrap_or_default(),
                            model_config
                                .as_ref()
                                .map(|config| config.model_name.as_str())
                                .unwrap_or_else(|| requested_model.as_deref().unwrap_or_default()),
                            &request_id,
                            &upstream_request_id,
                            elapsed,
                            is_stream,
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

                if let Some(value) = json_value.as_ref() {
                    bind_resource_affinities(
                        &token_info,
                        &resource_affinity,
                        &channel,
                        spec.bind_resource_kind,
                        value,
                    )
                    .await;
                }

                if let Some((kind, id)) = delete_affinity.as_ref()
                    && let Err(error) = resource_affinity.delete(&token_info, kind, id).await
                {
                    tracing::warn!("failed to delete resource affinity: {error}");
                }

                let usage = json_value
                    .as_ref()
                    .and_then(extract_usage_from_value)
                    .unwrap_or_else(|| fallback_usage(estimated_tokens));
                let upstream_model = json_value
                    .as_ref()
                    .and_then(extract_model_from_response_value)
                    .or_else(|| actual_model.clone())
                    .unwrap_or_default();

                spawn_resource_usage_accounting_task(
                    billing,
                    rate_limiter,
                    log_svc,
                    channel_svc,
                    token_info,
                    channel,
                    model_config.clone(),
                    group_ratio,
                    pre_consumed,
                    usage,
                    request_id.clone(),
                    upstream_request_id.clone(),
                    requested_model.clone(),
                    upstream_model,
                    client_ip,
                    user_agent,
                    endpoint,
                    request_format,
                    elapsed,
                    0,
                    false,
                    spec.endpoint_scope,
                );

                let mut response =
                    build_bytes_response(status, body_bytes, content_type, &request_id);
                insert_upstream_request_id_header(&mut response, &upstream_request_id);
                return Ok(response);
            }
            Ok(resp) => {
                let elapsed = start.elapsed().as_millis() as i64;
                let status_code = resp.status().as_u16() as i32;
                let status = resp.status();
                let response_headers = resp.headers().clone();
                let response_body = resp.bytes().await.unwrap_or_default();
                let _ = billing
                    .refund_with_retry(&request_id, token_info.token_id, pre_consumed)
                    .await;
                let failure = classify_upstream_provider_failure(
                    channel.channel_type,
                    status,
                    &response_headers,
                    &response_body,
                );
                channel_svc.record_relay_failure_async(
                    channel.channel_id,
                    channel.account_id,
                    elapsed,
                    status_code,
                    failure.message.clone(),
                );
                apply_upstream_failure_scope(&mut route_state, &channel, failure.scope);
                if attempt == max_retries - 1 {
                    record_passthrough_failure(
                        &log_svc,
                        &token_info,
                        &channel,
                        endpoint,
                        request_format,
                        requested_model.as_deref().unwrap_or_default(),
                        actual_model.as_deref().unwrap_or_default(),
                        model_config
                            .as_ref()
                            .map(|config| config.model_name.as_str())
                            .unwrap_or_else(|| requested_model.as_deref().unwrap_or_default()),
                        &request_id,
                        &extract_upstream_request_id(&response_headers),
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
                route_state.exclude_selected_account(&channel);
                if attempt == max_retries - 1 {
                    record_passthrough_failure(
                        &log_svc,
                        &token_info,
                        &channel,
                        endpoint,
                        request_format,
                        requested_model.as_deref().unwrap_or_default(),
                        actual_model.as_deref().unwrap_or_default(),
                        model_config
                            .as_ref()
                            .map(|config| config.model_name.as_str())
                            .unwrap_or_else(|| requested_model.as_deref().unwrap_or_default()),
                        &request_id,
                        "",
                        elapsed,
                        is_stream,
                        &client_ip,
                        &user_agent,
                        0,
                        error.to_string(),
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

async fn relay_resource_multipart_post(
    token_info: TokenInfo,
    router_svc: ChannelRouter,
    http_client: UpstreamHttpClient,
    channel_svc: ChannelService,
    token_svc: TokenService,
    resource_affinity: ResourceAffinityService,
    client_ip: String,
    headers: HeaderMap,
    multipart: summer_web::axum::extract::Multipart,
    upstream_path: String,
    spec: ResourceRequestSpec,
    affinity_keys: Vec<(&'static str, String)>,
    delete_affinity: Option<(&'static str, String)>,
) -> OpenAiApiResult<Response> {
    let payload = parse_multipart_payload(multipart).await.map_err(|error| {
        OpenAiErrorResponse::internal_with("failed to parse multipart body", error)
    })?;
    if let Some(model) = payload.model.as_ref() {
        token_info
            .ensure_model_allowed(model)
            .map_err(|error| OpenAiErrorResponse::from_api_error(&error))?;
    }

    relay_resource_request(
        token_info,
        router_svc,
        http_client,
        channel_svc,
        token_svc,
        resource_affinity,
        client_ip,
        headers,
        None,
        Method::POST,
        upstream_path,
        spec,
        affinity_keys,
        None,
        Some(payload),
        delete_affinity,
    )
    .await
}

async fn relay_resource_request(
    token_info: TokenInfo,
    router_svc: ChannelRouter,
    http_client: UpstreamHttpClient,
    channel_svc: ChannelService,
    token_svc: TokenService,
    resource_affinity: ResourceAffinityService,
    client_ip: String,
    headers: HeaderMap,
    query: Option<String>,
    method: Method,
    upstream_path: String,
    spec: ResourceRequestSpec,
    affinity_keys: Vec<(&'static str, String)>,
    mut json_body: Option<&mut Value>,
    multipart_body: Option<ParsedMultipartPayload>,
    delete_affinity: Option<(&'static str, String)>,
) -> OpenAiApiResult<Response> {
    let request_id = extract_request_id(&headers);
    let requested_model = json_body
        .as_deref()
        .and_then(|body| model_from_json_body(body, None));
    let is_stream = json_body.as_deref().is_some_and(json_body_requests_stream);
    token_info
        .ensure_endpoint_allowed(spec.endpoint_scope)
        .map_err(|error| OpenAiErrorResponse::from_api_error(&error))?;
    token_svc.update_last_used_ip_async(token_info.token_id, client_ip);

    let request_method = reqwest::Method::from_bytes(method.as_str().as_bytes())
        .map_err(|error| OpenAiErrorResponse::internal_with("invalid request method", error))?;
    let max_retries = 3;
    let start = std::time::Instant::now();
    let requested_model_from_multipart = multipart_body
        .as_ref()
        .and_then(|payload| payload.model.as_deref());
    let mut route_state = ResourceRouteState::new(
        &token_info,
        &router_svc,
        spec.endpoint_scope,
        requested_model
            .as_deref()
            .or(requested_model_from_multipart),
    )
    .await?;

    for attempt in 0..max_retries {
        let Some(channel) = route_state
            .select(
                &token_info,
                &resource_affinity,
                &affinity_keys,
                json_body.as_deref(),
            )
            .await?
        else {
            return Err(OpenAiErrorResponse::no_available_channel(if attempt == 0 {
                "no available channel"
            } else {
                "all channels failed"
            }));
        };

        let mut request_builder = http_client.client().request(
            request_method.clone(),
            build_upstream_url(&channel.base_url, &upstream_path, query.as_deref()),
        );
        request_builder =
            apply_upstream_auth(request_builder, channel.channel_type, &channel.api_key);

        if let Some(body) = json_body.as_deref_mut() {
            if let Some(model) = requested_model.as_deref() {
                ensure_json_model(body, &mapped_model(&channel, model))?;
            }
            request_builder = request_builder.json(body);
        } else if let Some(payload) = multipart_body.as_ref() {
            let actual_model = payload
                .model
                .as_ref()
                .map(|model| mapped_model(&channel, model));
            request_builder =
                request_builder.multipart(payload.to_form(actual_model.as_deref().unwrap_or(""))?);
        }
        request_builder = apply_forward_headers(request_builder, &headers, false);

        let response = match request_builder.send().await {
            Ok(response) => response,
            Err(error) => {
                channel_svc.record_relay_failure_async(
                    channel.channel_id,
                    channel.account_id,
                    start.elapsed().as_millis() as i64,
                    0,
                    error.to_string(),
                );
                route_state.exclude_selected_account(&channel);
                if attempt == max_retries - 1 {
                    return Err(OpenAiErrorResponse::internal_with(
                        "failed to send upstream request",
                        error,
                    ));
                }
                continue;
            }
        };

        let status = response.status();
        let response_headers = response.headers().clone();
        let upstream_request_id = extract_upstream_request_id(&response_headers);
        let content_type = response_headers.get(CONTENT_TYPE).cloned();
        if status.is_success() && is_stream {
            return Ok(build_resource_passthrough_stream_response(
                response,
                token_info,
                channel,
                request_id,
                upstream_request_id,
                start.elapsed().as_millis() as i64,
                channel_svc,
                resource_affinity,
                spec.bind_resource_kind,
            ));
        }
        let upstream_body = match response.bytes().await {
            Ok(body) => body,
            Err(error) => {
                channel_svc.record_relay_failure_async(
                    channel.channel_id,
                    channel.account_id,
                    start.elapsed().as_millis() as i64,
                    0,
                    format!("failed to read upstream response: {error}"),
                );
                route_state.exclude_selected_account(&channel);
                if attempt == max_retries - 1 {
                    return Err(OpenAiErrorResponse::internal_with(
                        "failed to read upstream response",
                        error,
                    ));
                }
                continue;
            }
        };

        if status.is_success() {
            if let Some(message) = unusable_success_response_message(
                status,
                &upstream_body,
                &upstream_path,
                allow_empty_success_body_for_upstream_path(&upstream_path),
            ) {
                channel_svc.record_relay_failure_async(
                    channel.channel_id,
                    channel.account_id,
                    start.elapsed().as_millis() as i64,
                    status.as_u16() as i32,
                    message.clone(),
                );
                route_state.exclude_selected_channel(&channel);
                if attempt == max_retries - 1 {
                    return Err(OpenAiErrorResponse::unsupported_endpoint(message));
                }
                continue;
            }

            if let Ok(value) = serde_json::from_slice::<Value>(&upstream_body) {
                bind_resource_affinities(
                    &token_info,
                    &resource_affinity,
                    &channel,
                    spec.bind_resource_kind,
                    &value,
                )
                .await;
            }

            if let Some((kind, id)) = delete_affinity.as_ref()
                && let Err(error) = resource_affinity.delete(&token_info, kind, id).await
            {
                tracing::warn!("failed to delete resource affinity: {error}");
            }

            if let Err(error) = channel_svc
                .record_relay_success(
                    channel.channel_id,
                    channel.account_id,
                    start.elapsed().as_millis() as i64,
                )
                .await
            {
                tracing::warn!("failed to update relay success health state: {error}");
            }

            let mut response =
                build_bytes_response(status, upstream_body, content_type, &request_id);
            insert_upstream_request_id_header(&mut response, &upstream_request_id);
            return Ok(response);
        }

        let failure = classify_upstream_provider_failure(
            channel.channel_type,
            status,
            &response_headers,
            &upstream_body,
        );
        channel_svc.record_relay_failure_async(
            channel.channel_id,
            channel.account_id,
            start.elapsed().as_millis() as i64,
            status.as_u16() as i32,
            failure.message.clone(),
        );
        apply_upstream_failure_scope(&mut route_state, &channel, failure.scope);
        if attempt == max_retries - 1 {
            return Err(failure.error);
        }
    }

    Err(OpenAiErrorResponse::no_available_channel(
        "all channels failed",
    ))
}

fn build_generic_stream_response(
    upstream: reqwest::Response,
    token_info: TokenInfo,
    pre_consumed: i64,
    model_config: Option<ModelConfigInfo>,
    group_ratio: f64,
    channel: SelectedChannel,
    requested_model: Option<String>,
    estimated_prompt_tokens: i32,
    endpoint: &'static str,
    request_format: &'static str,
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
    bind_resource_kind: Option<&'static str>,
    endpoint_scope: &'static str,
) -> Response {
    let status = upstream.status();
    let content_type = upstream.headers().get(CONTENT_TYPE).cloned();
    let response_request_id = request_id.clone();
    let response_upstream_request_id = upstream_request_id.clone();

    let stream = async_stream::stream! {
        let start = std::time::Instant::now();
        let mut tracker = GenericStreamTracker::default();
        let mut first_token_time = None;
        let mut stream_error = None;
        let mut byte_stream = upstream.bytes_stream();

        while let Some(result) = byte_stream.next().await {
            match result {
                Ok(chunk) => {
                    tracker.ingest(&chunk, &start, &mut first_token_time);
                    yield Ok::<Bytes, Infallible>(chunk);
                }
                Err(error) => {
                    tracing::error!("generic stream read error: {error}");
                    stream_error = Some(error.to_string());
                    break;
                }
            }
        }

        let total_elapsed = start_elapsed + start.elapsed().as_millis() as i64;
        if let Some(resource_kind) = bind_resource_kind
            && !tracker.resource_id.is_empty()
        {
            bind_resource_affinity(
                &token_info,
                &resource_affinity,
                &channel,
                resource_kind,
                &tracker.resource_id,
            )
            .await;
        }
        bind_resource_affinity_refs(
            &token_info,
            &resource_affinity,
            &channel,
            &tracker.resource_refs,
        )
        .await;

        if let Some(usage) = tracker.usage {
            let upstream_model = if tracker.upstream_model.is_empty() {
                requested_model.clone().unwrap_or_default()
            } else {
                tracker.upstream_model
            };

            spawn_resource_usage_accounting_task(
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
                total_elapsed,
                first_token_time.unwrap_or(0) as i32,
                true,
                endpoint_scope,
            );
        } else {
            billing.refund_later(request_id.clone(), token_info.token_id, pre_consumed);
            let rl = rate_limiter.clone();
            let request_id_for_task = request_id.clone();
            tokio::spawn(async move {
                if let Err(error) = rl.finalize_failure_with_retry(&request_id_for_task).await {
                    tracing::warn!("failed to finalize rate limit failure: {error}");
                }
            });
            channel_svc.record_relay_failure_async(
                channel.channel_id,
                channel.account_id,
                total_elapsed,
                0,
                stream_error.unwrap_or_else(|| {
                    format!("stream ended without usage; estimated_prompt_tokens={estimated_prompt_tokens}")
                }),
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
    insert_upstream_request_id_header(&mut response, &response_upstream_request_id);
    response
}

fn build_resource_passthrough_stream_response(
    upstream: reqwest::Response,
    token_info: TokenInfo,
    channel: SelectedChannel,
    request_id: String,
    upstream_request_id: String,
    start_elapsed: i64,
    channel_svc: ChannelService,
    resource_affinity: ResourceAffinityService,
    bind_resource_kind: Option<&'static str>,
) -> Response {
    let status = upstream.status();
    let content_type = upstream.headers().get(CONTENT_TYPE).cloned();
    let response_request_id = request_id;

    let stream = async_stream::stream! {
        let start = std::time::Instant::now();
        let mut tracker = GenericStreamTracker::default();
        let mut first_token_time = None;
        let mut stream_error = None;
        let mut byte_stream = upstream.bytes_stream();

        while let Some(result) = byte_stream.next().await {
            match result {
                Ok(chunk) => {
                    tracker.ingest(&chunk, &start, &mut first_token_time);
                    yield Ok::<Bytes, Infallible>(chunk);
                }
                Err(error) => {
                    tracing::error!("resource stream read error: {error}");
                    stream_error = Some(error.to_string());
                    break;
                }
            }
        }

        let total_elapsed = start_elapsed + start.elapsed().as_millis() as i64;
        if let Some(resource_kind) = bind_resource_kind
            && !tracker.resource_id.is_empty()
        {
            bind_resource_affinity(
                &token_info,
                &resource_affinity,
                &channel,
                resource_kind,
                &tracker.resource_id,
            )
            .await;
        }
        bind_resource_affinity_refs(
            &token_info,
            &resource_affinity,
            &channel,
            &tracker.resource_refs,
        )
        .await;

        if let Some(error) = stream_error {
            channel_svc.record_relay_failure_async(
                channel.channel_id,
                channel.account_id,
                total_elapsed,
                0,
                error,
            );
        } else if let Err(error) = channel_svc
            .record_relay_success(channel.channel_id, channel.account_id, total_elapsed)
            .await
        {
            tracing::warn!("failed to update relay success health state: {error}");
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
    insert_upstream_request_id_header(&mut response, &upstream_request_id);
    response
}

#[allow(clippy::too_many_arguments)]
fn spawn_resource_usage_accounting_task(
    billing: BillingEngine,
    rate_limiter: RateLimitEngine,
    log_svc: LogService,
    channel_svc: ChannelService,
    token_info: TokenInfo,
    channel: SelectedChannel,
    model_config: Option<ModelConfigInfo>,
    group_ratio: f64,
    pre_consumed: i64,
    usage: Usage,
    request_id: String,
    upstream_request_id: String,
    requested_model: Option<String>,
    upstream_model: String,
    client_ip: String,
    user_agent: String,
    endpoint: &'static str,
    request_format: &'static str,
    elapsed: i64,
    first_token_time: i32,
    is_stream: bool,
    endpoint_scope: &'static str,
) {
    tokio::spawn(async move {
        let Some(accounting_model) =
            usage_accounting_model(requested_model.as_deref(), &upstream_model)
        else {
            tracing::warn!("failed to determine usage accounting model for endpoint {endpoint}");
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
            return;
        };

        let model_config = match model_config {
            Some(model_config) => model_config,
            None => match billing
                .get_model_config_for_endpoint(&accounting_model, endpoint_scope)
                .await
            {
                Ok(model_config) => model_config,
                Err(error) => {
                    tracing::warn!("failed to load model config for usage accounting: {error}");
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
                    return;
                }
            },
        };

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
                requested_model: requested_model.unwrap_or_else(|| accounting_model.clone()),
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

fn usage_accounting_model(requested_model: Option<&str>, upstream_model: &str) -> Option<String> {
    requested_model
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            let upstream_model = upstream_model.trim();
            (!upstream_model.is_empty()).then(|| upstream_model.to_string())
        })
}

async fn bind_resource_affinities(
    token_info: &TokenInfo,
    resource_affinity: &ResourceAffinityService,
    channel: &SelectedChannel,
    primary_kind: Option<&'static str>,
    value: &Value,
) {
    if let Some(resource_kind) = primary_kind
        && let Some(id) = extract_generic_resource_id(value)
    {
        bind_resource_affinity(token_info, resource_affinity, channel, resource_kind, &id).await;
    }

    let refs = referenced_resource_ids(value);
    bind_resource_affinity_refs(token_info, resource_affinity, channel, &refs).await;
}

async fn bind_resource_affinity_refs(
    token_info: &TokenInfo,
    resource_affinity: &ResourceAffinityService,
    channel: &SelectedChannel,
    refs: &[(&'static str, String)],
) {
    for (resource_kind, resource_id) in refs {
        bind_resource_affinity(
            token_info,
            resource_affinity,
            channel,
            resource_kind,
            resource_id,
        )
        .await;
    }
}

async fn bind_resource_affinity(
    token_info: &TokenInfo,
    resource_affinity: &ResourceAffinityService,
    channel: &SelectedChannel,
    resource_kind: &'static str,
    resource_id: &str,
) {
    if resource_id.trim().is_empty() {
        return;
    }

    if let Err(error) = resource_affinity
        .bind(token_info, resource_kind, resource_id, channel)
        .await
    {
        tracing::warn!("failed to bind resource affinity: {error}");
    }
}

fn mapped_model(channel: &SelectedChannel, requested_model: &str) -> String {
    channel
        .model_mapping
        .get(requested_model)
        .and_then(Value::as_str)
        .unwrap_or(requested_model)
        .to_string()
}

fn model_from_json_body(body: &Value, default_model: Option<&str>) -> Option<String> {
    body.get("model")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .or_else(|| default_model.map(ToOwned::to_owned))
}

fn json_body_requests_stream(body: &Value) -> bool {
    body.get("stream").and_then(Value::as_bool).unwrap_or(false)
}

fn ensure_json_model(body: &mut Value, model: &str) -> OpenAiApiResult<()> {
    let Some(map) = body.as_object_mut() else {
        return Err(OpenAiErrorResponse::invalid_request(
            "request body must be a JSON object",
        ));
    };
    map.insert("model".into(), Value::String(model.to_string()));
    Ok(())
}

fn estimate_json_tokens(body: &Value) -> i32 {
    let tokens = ((body.to_string().len() as f64) / 4.0).ceil() as i32;
    tokens.max(1)
}

fn estimate_total_tokens_for_rate_limit(body: &Value) -> i64 {
    let output_tokens = body
        .get("max_tokens")
        .or_else(|| body.get("max_output_tokens"))
        .and_then(Value::as_i64)
        .unwrap_or(0)
        .max(0);
    i64::from(estimate_json_tokens(body)) + output_tokens.max(1)
}

fn extract_usage_from_value(value: &Value) -> Option<Usage> {
    let usage = value.get("usage")?;

    if usage.get("prompt_tokens").is_some() {
        return Some(Usage {
            prompt_tokens: usage.get("prompt_tokens")?.as_i64()? as i32,
            completion_tokens: usage
                .get("completion_tokens")
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32,
            total_tokens: usage.get("total_tokens")?.as_i64()? as i32,
            cached_tokens: usage
                .get("cached_tokens")
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32,
            reasoning_tokens: usage
                .get("reasoning_tokens")
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32,
        });
    }

    Some(Usage {
        prompt_tokens: usage
            .get("input_tokens")
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32,
        completion_tokens: usage
            .get("output_tokens")
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32,
        total_tokens: usage.get("total_tokens")?.as_i64()? as i32,
        cached_tokens: usage
            .get("input_tokens_details")
            .and_then(|details| details.get("cached_tokens"))
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32,
        reasoning_tokens: usage
            .get("output_tokens_details")
            .and_then(|details| details.get("reasoning_tokens"))
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32,
    })
}

fn extract_model_from_response_value(value: &Value) -> Option<String> {
    value
        .get("model")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .or_else(|| {
            value
                .get("response")
                .and_then(|response| response.get("model"))
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
}

fn payload_has_text_delta(payload: &Value) -> bool {
    payload
        .get("choices")
        .and_then(Value::as_array)
        .is_some_and(|choices| {
            choices.iter().any(|choice| {
                choice
                    .get("text")
                    .and_then(Value::as_str)
                    .is_some_and(|text| !text.is_empty())
                    || choice
                        .get("delta")
                        .and_then(|delta| delta.get("content"))
                        .and_then(Value::as_str)
                        .is_some_and(|text| !text.is_empty())
            })
        })
        || payload
            .get("type")
            .and_then(Value::as_str)
            .is_some_and(|event_type| event_type == "response.output_text.delta")
}

async fn parse_multipart_payload(
    mut multipart: summer_web::axum::extract::Multipart,
) -> anyhow::Result<ParsedMultipartPayload> {
    let mut fields = Vec::new();
    let mut model = None;
    let mut estimated_tokens = 0_i32;

    while let Some(field) = multipart.next_field().await? {
        let Some(name) = field.name().map(ToOwned::to_owned) else {
            continue;
        };

        if let Some(file_name) = field.file_name().map(ToOwned::to_owned) {
            let content_type = field.content_type().map(ToOwned::to_owned);
            let bytes = field.bytes().await?;
            fields.push(MultipartField::File {
                name,
                file_name,
                content_type,
                bytes,
            });
            continue;
        }

        let value = field.text().await?;
        if name == "model" && !value.trim().is_empty() {
            model = Some(value.clone());
        }
        estimated_tokens += (((value.len() as f64) / 4.0).ceil() as i32).max(1);
        fields.push(MultipartField::Text { name, value });
    }

    Ok(ParsedMultipartPayload {
        fields,
        model,
        estimated_tokens: estimated_tokens.max(1),
    })
}

impl ParsedMultipartPayload {
    fn to_form(&self, actual_model: &str) -> OpenAiApiResult<Form> {
        let mut form = Form::new();
        let mut wrote_model = false;

        for field in &self.fields {
            match field {
                MultipartField::Text { name, value } => {
                    if name == "model" {
                        wrote_model = true;
                        form = form.text(name.clone(), actual_model.to_string());
                    } else {
                        form = form.text(name.clone(), value.clone());
                    }
                }
                MultipartField::File {
                    name,
                    file_name,
                    content_type,
                    bytes,
                } => {
                    let mut part = Part::bytes(bytes.clone().to_vec()).file_name(file_name.clone());
                    if let Some(content_type) = content_type {
                        part = part.mime_str(content_type).map_err(|error| {
                            OpenAiErrorResponse::internal_with(
                                "failed to build multipart file part",
                                error,
                            )
                        })?;
                    }
                    form = form.part(name.clone(), part);
                }
            }
        }

        if !wrote_model && !actual_model.is_empty() {
            form = form.text("model".to_string(), actual_model.to_string());
        }

        Ok(form)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::router::test_support::{
        MockRoute, MockUpstreamServer, MultipartRequestSpec, TestHarness, response_json,
        response_text,
    };
    use summer_ai_model::entity::channel_account::AccountStatus;
    use summer_ai_model::entity::log::LogStatus;
    use summer_web::axum::http::header;

    #[test]
    fn model_from_json_body_uses_default() {
        let body = serde_json::json!({"input": "hello"});
        assert_eq!(
            model_from_json_body(&body, Some("omni-moderation-latest")).as_deref(),
            Some("omni-moderation-latest")
        );
    }

    #[test]
    fn model_from_json_body_prefers_explicit_model() {
        let body = serde_json::json!({
            "model": "gpt-5.4",
            "input": "hello"
        });
        assert_eq!(
            model_from_json_body(&body, Some("omni-moderation-latest")).as_deref(),
            Some("gpt-5.4")
        );
    }

    #[test]
    fn json_body_requests_stream_detects_true_flag() {
        assert!(json_body_requests_stream(&serde_json::json!({
            "model": "gpt-5.4",
            "stream": true
        })));
        assert!(!json_body_requests_stream(&serde_json::json!({
            "model": "gpt-5.4"
        })));
    }

    #[test]
    fn estimate_total_tokens_uses_max_output_tokens() {
        let body = serde_json::json!({
            "model": "gpt-5.4",
            "input": "hello",
            "max_output_tokens": 512
        });
        assert!(estimate_total_tokens_for_rate_limit(&body) >= 512);
    }

    #[test]
    fn extract_usage_supports_chat_shape() {
        let usage = extract_usage_from_value(&serde_json::json!({
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 20,
                "total_tokens": 30
            }
        }))
        .unwrap();
        assert_eq!(usage.total_tokens, 30);
    }

    #[test]
    fn extract_usage_supports_responses_shape() {
        let usage = extract_usage_from_value(&serde_json::json!({
            "usage": {
                "input_tokens": 11,
                "output_tokens": 22,
                "total_tokens": 33,
                "input_tokens_details": {"cached_tokens": 4},
                "output_tokens_details": {"reasoning_tokens": 5}
            }
        }))
        .unwrap();
        assert_eq!(usage.prompt_tokens, 11);
        assert_eq!(usage.cached_tokens, 4);
        assert_eq!(usage.reasoning_tokens, 5);
    }

    #[test]
    fn extract_model_from_response_value_supports_nested_response() {
        let payload = serde_json::json!({
            "type": "response.completed",
            "response": {
                "id": "resp_123",
                "model": "gpt-5.4"
            }
        });
        assert_eq!(
            extract_model_from_response_value(&payload).as_deref(),
            Some("gpt-5.4")
        );
    }

    #[test]
    fn generic_stream_tracker_collects_resource_refs() {
        let body = Bytes::from_static(
            br#"data: {"id":"run_123","thread_id":"thread_123","assistant_id":"asst_123","usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}

"#,
        );
        let start = std::time::Instant::now();
        let mut first_token_time = None;
        let mut tracker = GenericStreamTracker::default();

        tracker.ingest(&body, &start, &mut first_token_time);

        assert_eq!(tracker.resource_id, "run_123");
        assert!(
            tracker
                .resource_refs
                .contains(&("thread", "thread_123".to_string()))
        );
        assert!(
            tracker
                .resource_refs
                .contains(&("assistant", "asst_123".to_string()))
        );
    }

    #[test]
    fn referenced_resource_ids_extract_known_fields() {
        use crate::router::openai_passthrough::resource::referenced_resource_ids;

        let body = serde_json::json!({
            "assistant_id": "asst_123",
            "file_id": "file_123",
            "previous_response_id": "resp_123"
        });

        let refs = referenced_resource_ids(&body);
        assert!(refs.contains(&("assistant", "asst_123".to_string())));
        assert!(refs.contains(&("file", "file_123".to_string())));
        assert!(refs.contains(&("response", "resp_123".to_string())));
    }

    #[test]
    fn referenced_resource_ids_extract_nested_resource_fields() {
        let body = serde_json::json!({
            "input_file_id": "file_input",
            "tool_resources": {
                "code_interpreter": {
                    "file_ids": ["file_a", "file_b"]
                },
                "file_search": {
                    "vector_store_ids": ["vs_1", "vs_2"]
                }
            }
        });

        let refs = referenced_resource_ids(&body);
        assert!(refs.contains(&("file", "file_input".to_string())));
        assert!(refs.contains(&("file", "file_a".to_string())));
        assert!(refs.contains(&("file", "file_b".to_string())));
        assert!(refs.contains(&("vector_store", "vs_1".to_string())));
        assert!(refs.contains(&("vector_store", "vs_2".to_string())));
    }

    #[test]
    fn resource_affinity_lookup_keys_keeps_explicit_keys_first() {
        use crate::router::openai_passthrough::resource::resource_affinity_lookup_keys;

        let body = serde_json::json!({
            "assistant_id": "asst_123",
            "thread_id": "thread_123",
            "run_id": "run_123"
        });

        let keys = resource_affinity_lookup_keys(
            &[("run", "run_123".into()), ("thread", "thread_123".into())],
            Some(&body),
        );

        assert_eq!(keys[0], ("run", "run_123".into()));
        assert_eq!(keys[1], ("thread", "thread_123".into()));
        assert!(keys.contains(&("assistant", "asst_123".into())));
    }

    #[test]
    fn resource_affinity_lookup_keys_deduplicates_exact_duplicates() {
        let body = serde_json::json!({
            "thread_id": "thread_123",
            "run_id": "run_123"
        });

        let keys = resource_affinity_lookup_keys(
            &[("thread", "thread_123".into()), ("run", "run_123".into())],
            Some(&body),
        );

        assert_eq!(keys.len(), 2);
        assert_eq!(keys[0], ("thread", "thread_123".into()));
        assert_eq!(keys[1], ("run", "run_123".into()));
    }

    #[test]
    fn resource_affinity_lookup_keys_covers_vector_store_file_chain() {
        let body = serde_json::json!({
            "vector_store_id": "vs_123",
            "file_id": "file_123"
        });

        let keys = resource_affinity_lookup_keys(&[("vector_store", "vs_123".into())], Some(&body));

        assert_eq!(keys[0], ("vector_store", "vs_123".into()));
        assert!(keys.contains(&("file", "file_123".into())));
    }

    #[test]
    fn resource_affinity_lookup_keys_covers_response_chain() {
        let body = serde_json::json!({
            "response_id": "resp_current",
            "previous_response_id": "resp_prev"
        });

        let keys = resource_affinity_lookup_keys(&[], Some(&body));

        assert_eq!(keys[0], ("response", "resp_current".into()));
        assert_eq!(keys[1], ("response", "resp_prev".into()));
    }

    #[test]
    fn resource_affinity_lookup_keys_prefers_thread_path_over_assistant_body_reference() {
        let body = serde_json::json!({
            "assistant_id": "asst_123"
        });

        let keys = resource_affinity_lookup_keys(&[("thread", "thread_123".into())], Some(&body));

        assert_eq!(keys[0], ("thread", "thread_123".into()));
        assert_eq!(keys[1], ("assistant", "asst_123".into()));
    }

    #[test]
    fn resource_affinity_lookup_keys_prefers_run_then_thread_for_submit_tool_outputs() {
        let keys = resource_affinity_lookup_keys(
            &[("run", "run_123".into()), ("thread", "thread_123".into())],
            None,
        );

        assert_eq!(keys[0], ("run", "run_123".into()));
        assert_eq!(keys[1], ("thread", "thread_123".into()));
    }

    #[test]
    fn resource_affinity_lookup_keys_prefers_file_before_vector_store_for_nested_file_routes() {
        let keys = resource_affinity_lookup_keys(
            &[
                ("file", "file_123".into()),
                ("vector_store", "vs_123".into()),
            ],
            None,
        );

        assert_eq!(keys[0], ("file", "file_123".into()));
        assert_eq!(keys[1], ("vector_store", "vs_123".into()));
    }

    #[test]
    fn resource_affinity_lookup_keys_appends_nested_tool_resources_after_explicit_chain_keys() {
        let body = serde_json::json!({
            "tool_resources": {
                "code_interpreter": {
                    "file_ids": ["file_a"]
                },
                "file_search": {
                    "vector_store_ids": ["vs_1"]
                }
            }
        });

        let keys = resource_affinity_lookup_keys(
            &[("run", "run_123".into()), ("thread", "thread_123".into())],
            Some(&body),
        );

        assert_eq!(keys[0], ("run", "run_123".into()));
        assert_eq!(keys[1], ("thread", "thread_123".into()));
        assert!(keys.contains(&("file", "file_a".into())));
        assert!(keys.contains(&("vector_store", "vs_1".into())));
    }

    #[test]
    fn usage_accounting_model_falls_back_to_upstream_model() {
        assert_eq!(
            usage_accounting_model(None, "gpt-5.4"),
            Some("gpt-5.4".to_string())
        );
    }

    #[test]
    fn usage_accounting_model_prefers_requested_model() {
        assert_eq!(
            usage_accounting_model(Some("gpt-5.4 xhigh"), "gpt-5.4"),
            Some("gpt-5.4 xhigh".to_string())
        );
    }

    #[test]
    fn usage_accounting_model_returns_none_when_both_inputs_are_blank() {
        assert_eq!(usage_accounting_model(Some("   "), " "), None);
    }

    #[test]
    fn build_upstream_url_preserves_query_string() {
        use crate::router::openai_passthrough::support::build_upstream_url;

        assert_eq!(
            build_upstream_url(
                "https://example.com/",
                "/v1/files",
                Some("limit=20&after=file_123")
            ),
            "https://example.com/v1/files?limit=20&after=file_123"
        );
    }

    #[test]
    fn build_upstream_url_avoids_duplicate_v1_for_azure_openai_base() {
        use crate::router::openai_passthrough::support::build_upstream_url;

        assert_eq!(
            build_upstream_url(
                "https://example-resource.openai.azure.com/openai/v1/",
                "/v1/models",
                Some("api-version=preview")
            ),
            "https://example-resource.openai.azure.com/openai/v1/models?api-version=preview"
        );
    }

    #[test]
    fn apply_upstream_auth_uses_api_key_for_azure_channels() {
        use crate::router::openai_passthrough::support::apply_upstream_auth;

        let request = apply_upstream_auth(
            reqwest::Client::new()
                .get("https://example-resource.openai.azure.com/openai/v1/models"),
            14,
            "azure-key",
        )
        .build()
        .expect("build request");

        assert_eq!(
            request
                .headers()
                .get("api-key")
                .and_then(|value| value.to_str().ok()),
            Some("azure-key")
        );
        assert!(request.headers().get("authorization").is_none());
    }

    #[test]
    fn should_forward_header_filters_sensitive_headers() {
        use crate::router::openai_passthrough::support::should_forward_header;

        assert!(!should_forward_header(&header::AUTHORIZATION, false));
        assert!(!should_forward_header(&header::CONTENT_LENGTH, false));
        assert!(should_forward_header(
            &header::HeaderName::from_static("x-request-id"),
            false
        ));
        assert!(should_forward_header(
            &header::HeaderName::from_static("openai-beta"),
            false
        ));
    }

    #[test]
    fn payload_has_text_delta_for_chat_chunk_and_responses_event() {
        assert!(payload_has_text_delta(&serde_json::json!({
            "choices": [{
                "delta": {"content": "hello"}
            }]
        })));
        assert!(payload_has_text_delta(&serde_json::json!({
            "type": "response.output_text.delta",
            "delta": "world"
        })));
    }

    #[test]
    fn detect_unusable_upstream_success_response_returns_message() {
        let payload = serde_json::json!({
            "error": {
                "message": "endpoint disabled",
                "code": "unsupported_endpoint"
            }
        });
        assert_eq!(
            detect_unusable_upstream_success_response(&payload).as_deref(),
            Some("endpoint disabled")
        );
    }

    #[test]
    fn detect_unusable_upstream_success_response_prefers_code_when_message_missing() {
        let payload = serde_json::json!({
            "error": {
                "code": "unsupported_endpoint"
            }
        });
        assert_eq!(
            detect_unusable_upstream_success_response(&payload).as_deref(),
            Some("unsupported_endpoint")
        );
    }

    #[test]
    fn detect_unusable_upstream_success_response_ignores_missing_error() {
        let payload = serde_json::json!({
            "result": "ok"
        });
        assert!(detect_unusable_upstream_success_response(&payload).is_none());
    }

    #[test]
    fn unusable_success_response_message_flags_empty_body() {
        let body = Bytes::from_static(b"   ");
        assert_eq!(
            unusable_success_response_message(StatusCode::OK, &body, "responses", false,)
                .as_deref(),
            Some("upstream returned an empty success response for endpoint responses")
        );
    }

    #[test]
    fn unusable_success_response_message_allows_empty_body_when_configured() {
        let body = Bytes::from_static(b"   ");
        assert!(
            unusable_success_response_message(StatusCode::OK, &body, "files/content", true)
                .is_none()
        );
    }

    #[tokio::test]
    #[ignore = "requires local postgres and redis"]
    async fn responses_resource_chain_prefers_bound_channel_over_default_fallback() {
        let primary = MockUpstreamServer::spawn(vec![
            MockRoute::json(
                Method::POST,
                "/v1/responses",
                Some("Bearer sk-primary"),
                None,
                StatusCode::OK,
                serde_json::json!({
                    "id": "resp_chain_primary",
                    "object": "response",
                    "status": "completed",
                    "model": "__MODEL__",
                    "output": [],
                    "usage": {
                        "input_tokens": 3,
                        "output_tokens": 2,
                        "total_tokens": 5
                    }
                }),
            ),
            MockRoute::json(
                Method::GET,
                "/v1/responses/resp_chain_primary",
                Some("Bearer sk-primary"),
                None,
                StatusCode::OK,
                serde_json::json!({
                    "id": "resp_chain_primary",
                    "object": "response",
                    "status": "completed",
                    "route": "primary"
                }),
            ),
            MockRoute::json(
                Method::GET,
                "/v1/responses/resp_chain_primary/input_items",
                Some("Bearer sk-primary"),
                None,
                StatusCode::OK,
                serde_json::json!({
                    "object": "list",
                    "data": [{
                        "id": "item_primary_1",
                        "type": "message",
                        "route": "primary"
                    }]
                }),
            ),
            MockRoute::json(
                Method::POST,
                "/v1/responses/resp_chain_primary/cancel",
                Some("Bearer sk-primary"),
                None,
                StatusCode::OK,
                serde_json::json!({
                    "id": "resp_chain_primary",
                    "object": "response",
                    "status": "cancelled",
                    "route": "primary"
                }),
            ),
        ])
        .await;
        let fallback = MockUpstreamServer::spawn(vec![]).await;
        let harness =
            TestHarness::responses_affinity_fixture(&primary.base_url, &fallback.base_url).await;
        primary.replace_placeholder("__MODEL__", &harness.model_name);

        let create_response = harness
            .json_request(
                Method::POST,
                "/v1/responses",
                "responses-chain-create",
                serde_json::json!({
                    "model": harness.model_name,
                    "input": "hello from responses chain",
                    "stream": false
                }),
            )
            .await;
        assert_eq!(create_response.status(), StatusCode::OK);
        let create_payload = response_json(create_response).await;
        assert_eq!(create_payload["id"], "resp_chain_primary");

        let get_response_payload = response_json(
            harness
                .empty_request(
                    Method::GET,
                    "/v1/responses/resp_chain_primary",
                    "responses-chain-get",
                )
                .await,
        )
        .await;
        assert_eq!(get_response_payload["route"], "primary");

        let input_items_payload = response_json(
            harness
                .empty_request(
                    Method::GET,
                    "/v1/responses/resp_chain_primary/input_items",
                    "responses-chain-input-items",
                )
                .await,
        )
        .await;
        assert_eq!(input_items_payload["data"][0]["route"], "primary");

        let cancel_payload = response_json(
            harness
                .empty_request(
                    Method::POST,
                    "/v1/responses/resp_chain_primary/cancel",
                    "responses-chain-cancel",
                )
                .await,
        )
        .await;
        assert_eq!(cancel_payload["route"], "primary");

        assert_eq!(primary.hit_count("/v1/responses"), 1);
        assert_eq!(primary.hit_count("/v1/responses/resp_chain_primary"), 1);
        assert_eq!(
            primary.hit_count("/v1/responses/resp_chain_primary/input_items"),
            1
        );
        assert_eq!(
            primary.hit_count("/v1/responses/resp_chain_primary/cancel"),
            1
        );
        assert_eq!(fallback.total_hits(), 0);

        harness.cleanup().await;
    }

    #[tokio::test]
    #[ignore = "requires local postgres and redis"]
    async fn assistants_thread_runs_chain_reuses_assistant_and_run_affinity() {
        let primary = MockUpstreamServer::spawn(vec![
            MockRoute::json(
                Method::POST,
                "/v1/assistants",
                Some("Bearer sk-primary"),
                None,
                StatusCode::OK,
                serde_json::json!({
                    "id": "asst_chain_primary",
                    "object": "assistant",
                    "model": "__MODEL__"
                }),
            ),
            MockRoute::json(
                Method::POST,
                "/v1/threads/runs",
                Some("Bearer sk-primary"),
                None,
                StatusCode::OK,
                serde_json::json!({
                    "id": "run_chain_primary",
                    "object": "thread.run",
                    "thread_id": "thread_chain_primary",
                    "assistant_id": "asst_chain_primary",
                    "model": "__MODEL__",
                    "status": "completed",
                    "usage": {
                        "prompt_tokens": 4,
                        "completion_tokens": 3,
                        "total_tokens": 7
                    }
                }),
            ),
            MockRoute::json(
                Method::POST,
                "/v1/threads/thread_chain_primary/runs/run_chain_primary/submit_tool_outputs",
                Some("Bearer sk-primary"),
                None,
                StatusCode::OK,
                serde_json::json!({
                    "id": "run_chain_primary",
                    "object": "thread.run",
                    "thread_id": "thread_chain_primary",
                    "assistant_id": "asst_chain_primary",
                    "model": "__MODEL__",
                    "status": "completed",
                    "route": "primary",
                    "usage": {
                        "prompt_tokens": 2,
                        "completion_tokens": 1,
                        "total_tokens": 3
                    }
                }),
            ),
            MockRoute::json(
                Method::GET,
                "/v1/threads/thread_chain_primary/runs/run_chain_primary",
                Some("Bearer sk-primary"),
                None,
                StatusCode::OK,
                serde_json::json!({
                    "id": "run_chain_primary",
                    "object": "thread.run",
                    "thread_id": "thread_chain_primary",
                    "assistant_id": "asst_chain_primary",
                    "status": "completed",
                    "route": "primary"
                }),
            ),
            MockRoute::json(
                Method::GET,
                "/v1/threads/thread_chain_primary/runs/run_chain_primary/steps",
                Some("Bearer sk-primary"),
                None,
                StatusCode::OK,
                serde_json::json!({
                    "object": "list",
                    "data": [{
                        "id": "step_chain_primary",
                        "object": "thread.run.step",
                        "route": "primary"
                    }]
                }),
            ),
            MockRoute::json(
                Method::GET,
                "/v1/threads/thread_chain_primary/runs/run_chain_primary/steps/step_chain_primary",
                Some("Bearer sk-primary"),
                None,
                StatusCode::OK,
                serde_json::json!({
                    "id": "step_chain_primary",
                    "object": "thread.run.step",
                    "route": "primary"
                }),
            ),
        ])
        .await;
        let fallback = MockUpstreamServer::spawn(vec![]).await;
        let harness =
            TestHarness::assistants_threads_affinity_fixture(&primary.base_url, &fallback.base_url)
                .await;
        primary.replace_placeholder("__MODEL__", &harness.model_name);

        let create_assistant_payload = response_json(
            harness
                .json_request(
                    Method::POST,
                    "/v1/assistants",
                    "assistant-chain-create",
                    serde_json::json!({
                        "model": harness.model_name,
                        "name": "integration assistant"
                    }),
                )
                .await,
        )
        .await;
        assert_eq!(create_assistant_payload["id"], "asst_chain_primary");

        let create_run_payload = response_json(
            harness
                .json_request(
                    Method::POST,
                    "/v1/threads/runs",
                    "assistant-chain-run",
                    serde_json::json!({
                        "assistant_id": "asst_chain_primary",
                        "thread": {
                            "messages": [{
                                "role": "user",
                                "content": "hello"
                            }]
                        }
                    }),
                )
                .await,
        )
        .await;
        assert_eq!(create_run_payload["id"], "run_chain_primary");

        let submit_payload = response_json(
            harness
                .json_request(
                    Method::POST,
                    "/v1/threads/thread_chain_primary/runs/run_chain_primary/submit_tool_outputs",
                    "assistant-chain-submit",
                    serde_json::json!({
                        "tool_outputs": [{
                            "tool_call_id": "call_123",
                            "output": "done"
                        }]
                    }),
                )
                .await,
        )
        .await;
        assert_eq!(submit_payload["route"], "primary");

        let get_run_payload = response_json(
            harness
                .empty_request(
                    Method::GET,
                    "/v1/threads/thread_chain_primary/runs/run_chain_primary",
                    "assistant-chain-get-run",
                )
                .await,
        )
        .await;
        assert_eq!(get_run_payload["route"], "primary");

        let list_steps_payload = response_json(
            harness
                .empty_request(
                    Method::GET,
                    "/v1/threads/thread_chain_primary/runs/run_chain_primary/steps",
                    "assistant-chain-list-steps",
                )
                .await,
        )
        .await;
        assert_eq!(list_steps_payload["data"][0]["route"], "primary");

        let get_step_payload = response_json(
            harness
                .empty_request(
                    Method::GET,
                    "/v1/threads/thread_chain_primary/runs/run_chain_primary/steps/step_chain_primary",
                    "assistant-chain-get-step",
                )
                .await,
        )
        .await;
        assert_eq!(get_step_payload["route"], "primary");

        assert_eq!(primary.hit_count("/v1/assistants"), 1);
        assert_eq!(primary.hit_count("/v1/threads/runs"), 1);
        assert_eq!(
            primary.hit_count(
                "/v1/threads/thread_chain_primary/runs/run_chain_primary/submit_tool_outputs"
            ),
            1
        );
        assert_eq!(
            primary.hit_count("/v1/threads/thread_chain_primary/runs/run_chain_primary"),
            1
        );
        assert_eq!(
            primary.hit_count("/v1/threads/thread_chain_primary/runs/run_chain_primary/steps"),
            1
        );
        assert_eq!(
            primary.hit_count(
                "/v1/threads/thread_chain_primary/runs/run_chain_primary/steps/step_chain_primary"
            ),
            1
        );
        assert_eq!(fallback.total_hits(), 0);

        harness.cleanup().await;
    }

    #[tokio::test]
    #[ignore = "requires local postgres and redis"]
    async fn threads_runs_create_without_request_model_settles_usage_from_response_model() {
        let primary = MockUpstreamServer::spawn(vec![MockRoute::json(
            Method::POST,
            "/v1/threads/runs",
            Some("Bearer sk-primary"),
            None,
            StatusCode::OK,
            serde_json::json!({
                "id": "run_usage_primary",
                "object": "thread.run",
                "thread_id": "thread_usage_primary",
                "assistant_id": "asst_usage_primary",
                "model": "__MODEL__",
                "status": "completed",
                "route": "primary",
                "usage": {
                    "prompt_tokens": 4,
                    "completion_tokens": 3,
                    "total_tokens": 7
                }
            }),
        )])
        .await;
        let harness = TestHarness::assistants_threads_affinity_fixture(
            &primary.base_url,
            "http://127.0.0.1:9",
        )
        .await;
        primary.replace_placeholder("__MODEL__", &harness.model_name);
        let request_id = format!("threads-runs-usage-{}", harness.model_name);

        let response = harness
            .json_request(
                Method::POST,
                "/v1/threads/runs",
                &request_id,
                serde_json::json!({
                    "assistant_id": "asst_usage_primary",
                    "thread": {
                        "messages": [{
                            "role": "user",
                            "content": "hello"
                        }]
                    }
                }),
            )
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        let payload = response_json(response).await;
        assert_eq!(payload["id"], "run_usage_primary");
        assert_eq!(payload["route"], "primary");

        let token = harness.wait_for_token_used_quota(7).await;
        assert_eq!(token.used_quota, 7);

        let log = harness.wait_for_log_by_request_id(&request_id).await;
        assert_eq!(log.endpoint, "threads/runs");
        assert_eq!(log.request_format, "openai/threads_runs");
        assert_eq!(log.requested_model, harness.model_name);
        assert_eq!(log.upstream_model, harness.model_name);
        assert_eq!(log.model_name, harness.model_name);
        assert_eq!(log.prompt_tokens, 4);
        assert_eq!(log.completion_tokens, 3);
        assert_eq!(log.total_tokens, 7);
        assert_eq!(log.quota, 7);
        assert_eq!(log.status_code, 200);
        assert_eq!(log.status, LogStatus::Success);
        assert!(!log.is_stream);

        harness.cleanup().await;
    }

    #[tokio::test]
    #[ignore = "requires local postgres and redis"]
    async fn threads_runs_stream_without_request_model_settles_usage_from_stream_tail() {
        let primary = MockUpstreamServer::spawn(vec![MockRoute::raw(
            Method::POST,
            "/v1/threads/runs",
            Some("Bearer sk-primary"),
            Some("\"stream\":true"),
            StatusCode::OK,
            "text/event-stream",
            concat!(
                "data: {\"id\":\"run_stream_primary\",\"object\":\"thread.run\",\"thread_id\":\"thread_stream_primary\",\"assistant_id\":\"asst_stream_primary\",\"model\":\"__MODEL__\",\"status\":\"in_progress\"}\n\n",
                "data: {\"id\":\"run_stream_primary\",\"object\":\"thread.run\",\"thread_id\":\"thread_stream_primary\",\"assistant_id\":\"asst_stream_primary\",\"model\":\"__MODEL__\",\"status\":\"completed\",\"usage\":{\"prompt_tokens\":4,\"completion_tokens\":3,\"total_tokens\":7}}\n\n",
                "data: [DONE]\n\n"
            ),
        )
        .with_response_headers(vec![("x-request-id", "run-stream-upstream-123")])])
        .await;
        let harness = TestHarness::assistants_threads_affinity_fixture(
            &primary.base_url,
            "http://127.0.0.1:9",
        )
        .await;
        primary.replace_placeholder("__MODEL__", &harness.model_name);
        let request_id = format!("threads-runs-stream-usage-{}", harness.model_name);

        let response = harness
            .json_request(
                Method::POST,
                "/v1/threads/runs",
                &request_id,
                serde_json::json!({
                    "assistant_id": "asst_stream_primary",
                    "stream": true,
                    "thread": {
                        "messages": [{
                            "role": "user",
                            "content": "hello"
                        }]
                    }
                }),
            )
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        let upstream_request_id = response
            .headers()
            .get("x-upstream-request-id")
            .and_then(|value| value.to_str().ok())
            .expect("thread run stream upstream request id")
            .to_string();
        let body = response_text(response).await;
        assert!(body.contains("run_stream_primary"));
        assert!(body.contains("\"total_tokens\":7"));
        assert_eq!(upstream_request_id, "run-stream-upstream-123");

        let token = harness.wait_for_token_used_quota(7).await;
        assert_eq!(token.used_quota, 7);

        let log = harness.wait_for_log_by_request_id(&request_id).await;
        assert_eq!(log.endpoint, "threads/runs");
        assert_eq!(log.request_format, "openai/threads_runs");
        assert_eq!(log.requested_model, harness.model_name);
        assert_eq!(log.upstream_model, harness.model_name);
        assert_eq!(log.model_name, harness.model_name);
        assert_eq!(log.upstream_request_id, "run-stream-upstream-123");
        assert_eq!(log.prompt_tokens, 4);
        assert_eq!(log.completion_tokens, 3);
        assert_eq!(log.total_tokens, 7);
        assert_eq!(log.quota, 7);
        assert_eq!(log.status_code, 200);
        assert_eq!(log.status, LogStatus::Success);
        assert!(log.is_stream);

        harness.cleanup().await;
    }

    #[tokio::test]
    #[ignore = "requires local postgres and redis"]
    async fn threads_runs_stream_falls_back_after_primary_overload() {
        let primary = MockUpstreamServer::spawn(vec![MockRoute::json(
            Method::POST,
            "/v1/threads/runs",
            Some("Bearer sk-primary"),
            Some("\"stream\":true"),
            StatusCode::SERVICE_UNAVAILABLE,
            serde_json::json!({
                "error": {
                    "message": "primary thread run upstream overloaded",
                    "type": "server_error"
                }
            }),
        )])
        .await;
        let fallback = MockUpstreamServer::spawn(vec![MockRoute::raw(
            Method::POST,
            "/v1/threads/runs",
            Some("Bearer sk-fallback"),
            Some("\"stream\":true"),
            StatusCode::OK,
            "text/event-stream",
            concat!(
                "data: {\"id\":\"run_stream_fallback\",\"object\":\"thread.run\",\"thread_id\":\"thread_stream_fallback\",\"assistant_id\":\"asst_stream_fallback\",\"model\":\"__MODEL__\",\"status\":\"in_progress\",\"route\":\"fallback\"}\n\n",
                "data: {\"id\":\"run_stream_fallback\",\"object\":\"thread.run\",\"thread_id\":\"thread_stream_fallback\",\"assistant_id\":\"asst_stream_fallback\",\"model\":\"__MODEL__\",\"status\":\"completed\",\"route\":\"fallback\",\"usage\":{\"prompt_tokens\":4,\"completion_tokens\":3,\"total_tokens\":7}}\n\n",
                "data: [DONE]\n\n"
            ),
        )
        .with_response_headers(vec![("x-request-id", "run-stream-fallback-upstream-123")])])
        .await;
        let harness =
            TestHarness::assistants_threads_affinity_fixture(&primary.base_url, &fallback.base_url)
                .await;
        fallback.replace_placeholder("__MODEL__", &harness.model_name);
        let request_id = format!("threads-runs-stream-fallback-{}", harness.model_name);

        let response = harness
            .json_request(
                Method::POST,
                "/v1/threads/runs",
                &request_id,
                serde_json::json!({
                    "assistant_id": "asst_stream_fallback",
                    "stream": true,
                    "thread": {
                        "messages": [{
                            "role": "user",
                            "content": "hello"
                        }]
                    }
                }),
            )
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        let upstream_request_id = response
            .headers()
            .get("x-upstream-request-id")
            .and_then(|value| value.to_str().ok())
            .expect("thread run fallback stream upstream request id")
            .to_string();
        let body = response_text(response).await;
        assert!(body.contains("run_stream_fallback"));
        assert!(body.contains("\"route\":\"fallback\""));
        assert!(body.contains("\"total_tokens\":7"));
        assert_eq!(upstream_request_id, "run-stream-fallback-upstream-123");

        let token = harness.wait_for_token_used_quota(7).await;
        assert_eq!(token.used_quota, 7);

        let log = harness.wait_for_log_by_request_id(&request_id).await;
        assert_eq!(log.endpoint, "threads/runs");
        assert_eq!(log.request_format, "openai/threads_runs");
        assert_eq!(log.requested_model, harness.model_name);
        assert_eq!(log.upstream_model, harness.model_name);
        assert_eq!(log.model_name, harness.model_name);
        assert_eq!(log.upstream_request_id, "run-stream-fallback-upstream-123");
        assert_eq!(log.total_tokens, 7);
        assert_eq!(log.quota, 7);
        assert_eq!(log.status, LogStatus::Success);
        assert!(log.is_stream);

        let primary_account = harness.wait_for_primary_account_overloaded().await;
        assert_eq!(primary_account.failure_streak, 1);
        assert!(primary_account.overload_until.is_some());
        assert!(primary_account.rate_limited_until.is_none());

        let primary_channel = harness.primary_channel_model().await;
        assert_eq!(primary_channel.failure_streak, 1);
        assert_eq!(primary_channel.last_health_status, 3);

        assert_eq!(primary.hit_count("/v1/threads/runs"), 1);
        assert_eq!(fallback.hit_count("/v1/threads/runs"), 1);

        harness.cleanup().await;
    }

    #[tokio::test]
    #[ignore = "requires local postgres and redis"]
    async fn threads_runs_stream_falls_back_after_primary_rate_limit() {
        let primary = MockUpstreamServer::spawn(vec![MockRoute::json(
            Method::POST,
            "/v1/threads/runs",
            Some("Bearer sk-primary"),
            Some("\"stream\":true"),
            StatusCode::TOO_MANY_REQUESTS,
            serde_json::json!({
                "error": {
                    "message": "primary thread run rate limited",
                    "type": "rate_limit_error"
                }
            }),
        )])
        .await;
        let fallback = MockUpstreamServer::spawn(vec![MockRoute::raw(
            Method::POST,
            "/v1/threads/runs",
            Some("Bearer sk-fallback"),
            Some("\"stream\":true"),
            StatusCode::OK,
            "text/event-stream",
            concat!(
                "data: {\"id\":\"run_stream_rate_limit_fallback\",\"object\":\"thread.run\",\"thread_id\":\"thread_stream_rate_limit_fallback\",\"assistant_id\":\"asst_stream_rate_limit_fallback\",\"model\":\"__MODEL__\",\"status\":\"in_progress\",\"route\":\"fallback\"}\n\n",
                "data: {\"id\":\"run_stream_rate_limit_fallback\",\"object\":\"thread.run\",\"thread_id\":\"thread_stream_rate_limit_fallback\",\"assistant_id\":\"asst_stream_rate_limit_fallback\",\"model\":\"__MODEL__\",\"status\":\"completed\",\"route\":\"fallback\",\"usage\":{\"prompt_tokens\":4,\"completion_tokens\":3,\"total_tokens\":7}}\n\n",
                "data: [DONE]\n\n"
            ),
        )
        .with_response_headers(vec![("x-request-id", "run-stream-rate-limit-upstream-123")])])
        .await;
        let harness =
            TestHarness::assistants_threads_affinity_fixture(&primary.base_url, &fallback.base_url)
                .await;
        fallback.replace_placeholder("__MODEL__", &harness.model_name);
        let request_id = format!("threads-runs-stream-rate-limit-{}", harness.model_name);

        let response = harness
            .json_request(
                Method::POST,
                "/v1/threads/runs",
                &request_id,
                serde_json::json!({
                    "assistant_id": "asst_stream_rate_limit_fallback",
                    "stream": true,
                    "thread": {
                        "messages": [{
                            "role": "user",
                            "content": "hello"
                        }]
                    }
                }),
            )
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        let upstream_request_id = response
            .headers()
            .get("x-upstream-request-id")
            .and_then(|value| value.to_str().ok())
            .expect("thread run rate limit fallback stream upstream request id")
            .to_string();
        let body = response_text(response).await;
        assert!(body.contains("run_stream_rate_limit_fallback"));
        assert!(body.contains("\"route\":\"fallback\""));
        assert!(body.contains("\"total_tokens\":7"));
        assert_eq!(upstream_request_id, "run-stream-rate-limit-upstream-123");

        let token = harness.wait_for_token_used_quota(7).await;
        assert_eq!(token.used_quota, 7);

        let log = harness.wait_for_log_by_request_id(&request_id).await;
        assert_eq!(log.endpoint, "threads/runs");
        assert_eq!(log.request_format, "openai/threads_runs");
        assert_eq!(log.requested_model, harness.model_name);
        assert_eq!(log.upstream_model, harness.model_name);
        assert_eq!(log.model_name, harness.model_name);
        assert_eq!(
            log.upstream_request_id,
            "run-stream-rate-limit-upstream-123"
        );
        assert_eq!(log.total_tokens, 7);
        assert_eq!(log.quota, 7);
        assert_eq!(log.status, LogStatus::Success);
        assert!(log.is_stream);

        let primary_account = harness.wait_for_primary_account_rate_limited().await;
        assert_eq!(primary_account.failure_streak, 1);
        assert!(primary_account.rate_limited_until.is_some());
        assert!(primary_account.overload_until.is_none());

        let primary_channel = harness.primary_channel_model().await;
        assert_eq!(primary_channel.failure_streak, 1);
        assert_eq!(primary_channel.last_health_status, 3);

        assert_eq!(primary.hit_count("/v1/threads/runs"), 1);
        assert_eq!(fallback.hit_count("/v1/threads/runs"), 1);

        harness.cleanup().await;
    }

    #[tokio::test]
    #[ignore = "requires local postgres and redis"]
    async fn files_vector_store_chain_keeps_affinity_after_default_route_switch() {
        let primary = MockUpstreamServer::spawn(vec![
            MockRoute::json(
                Method::POST,
                "/v1/files",
                Some("Bearer sk-primary"),
                Some("hello-file.txt"),
                StatusCode::OK,
                serde_json::json!({
                    "id": "file_chain_primary",
                    "object": "file",
                    "purpose": "assistants",
                    "route": "primary"
                }),
            ),
            MockRoute::json(
                Method::GET,
                "/v1/files/file_chain_primary",
                Some("Bearer sk-primary"),
                None,
                StatusCode::OK,
                serde_json::json!({
                    "id": "file_chain_primary",
                    "object": "file",
                    "route": "primary"
                }),
            ),
            MockRoute::raw(
                Method::GET,
                "/v1/files/file_chain_primary/content",
                Some("Bearer sk-primary"),
                None,
                StatusCode::OK,
                "text/plain",
                "primary-file-content",
            ),
            MockRoute::json(
                Method::POST,
                "/v1/vector_stores",
                Some("Bearer sk-primary"),
                Some("vs-chain-primary"),
                StatusCode::OK,
                serde_json::json!({
                    "id": "vs_chain_primary",
                    "object": "vector_store",
                    "name": "vs-chain-primary",
                    "route": "primary"
                }),
            ),
            MockRoute::json(
                Method::GET,
                "/v1/vector_stores/vs_chain_primary",
                Some("Bearer sk-primary"),
                None,
                StatusCode::OK,
                serde_json::json!({
                    "id": "vs_chain_primary",
                    "object": "vector_store",
                    "route": "primary"
                }),
            ),
            MockRoute::json(
                Method::POST,
                "/v1/vector_stores/vs_chain_primary/files",
                Some("Bearer sk-primary"),
                Some("file_chain_primary"),
                StatusCode::OK,
                serde_json::json!({
                    "id": "file_chain_primary",
                    "object": "vector_store.file",
                    "vector_store_id": "vs_chain_primary",
                    "route": "primary"
                }),
            ),
            MockRoute::json(
                Method::GET,
                "/v1/vector_stores/vs_chain_primary/files/file_chain_primary",
                Some("Bearer sk-primary"),
                None,
                StatusCode::OK,
                serde_json::json!({
                    "id": "file_chain_primary",
                    "object": "vector_store.file",
                    "vector_store_id": "vs_chain_primary",
                    "route": "primary"
                }),
            ),
            MockRoute::json(
                Method::POST,
                "/v1/vector_stores/vs_chain_primary/search",
                Some("Bearer sk-primary"),
                Some("find file_chain_primary"),
                StatusCode::OK,
                serde_json::json!({
                    "object": "list",
                    "data": [{
                        "id": "chunk_primary_1",
                        "route": "primary"
                    }]
                }),
            ),
        ])
        .await;
        let fallback = MockUpstreamServer::spawn(vec![
            MockRoute::json(
                Method::GET,
                "/v1/files",
                Some("Bearer sk-fallback"),
                None,
                StatusCode::OK,
                serde_json::json!({
                    "object": "list",
                    "data": [],
                    "route": "fallback"
                }),
            ),
            MockRoute::json(
                Method::GET,
                "/v1/vector_stores",
                Some("Bearer sk-fallback"),
                None,
                StatusCode::OK,
                serde_json::json!({
                    "object": "list",
                    "data": [],
                    "route": "fallback"
                }),
            ),
        ])
        .await;
        let harness = TestHarness::files_vector_stores_affinity_fixture(
            &primary.base_url,
            &fallback.base_url,
        )
        .await;

        let create_file_payload = response_json(
            harness
                .multipart_request(MultipartRequestSpec {
                    uri: "/v1/files",
                    request_id: "files-chain-create-file",
                    text_fields: &[("purpose", "assistants")],
                    file_field_name: "file",
                    file_name: "hello-file.txt",
                    file_content_type: "text/plain",
                    file_bytes: b"hello from file chain",
                })
                .await,
        )
        .await;
        assert_eq!(create_file_payload["id"], "file_chain_primary");

        let create_vector_store_payload = response_json(
            harness
                .json_request(
                    Method::POST,
                    "/v1/vector_stores",
                    "files-chain-create-vs",
                    serde_json::json!({
                        "name": "vs-chain-primary"
                    }),
                )
                .await,
        )
        .await;
        assert_eq!(create_vector_store_payload["id"], "vs_chain_primary");

        harness
            .promote_fallback_for_scopes(&["files", "vector_stores"])
            .await;

        let list_files_payload = response_json(
            harness
                .empty_request(Method::GET, "/v1/files", "files-chain-list-files")
                .await,
        )
        .await;
        assert_eq!(list_files_payload["route"], "fallback");

        let list_vector_stores_payload = response_json(
            harness
                .empty_request(
                    Method::GET,
                    "/v1/vector_stores",
                    "files-chain-list-vector-stores",
                )
                .await,
        )
        .await;
        assert_eq!(list_vector_stores_payload["route"], "fallback");

        let get_file_payload = response_json(
            harness
                .empty_request(
                    Method::GET,
                    "/v1/files/file_chain_primary",
                    "files-chain-get-file",
                )
                .await,
        )
        .await;
        assert_eq!(get_file_payload["route"], "primary");

        let file_content = response_text(
            harness
                .empty_request(
                    Method::GET,
                    "/v1/files/file_chain_primary/content",
                    "files-chain-get-content",
                )
                .await,
        )
        .await;
        assert_eq!(file_content, "primary-file-content");

        let get_vector_store_payload = response_json(
            harness
                .empty_request(
                    Method::GET,
                    "/v1/vector_stores/vs_chain_primary",
                    "files-chain-get-vector-store",
                )
                .await,
        )
        .await;
        assert_eq!(get_vector_store_payload["route"], "primary");

        let create_vector_store_file_payload = response_json(
            harness
                .json_request(
                    Method::POST,
                    "/v1/vector_stores/vs_chain_primary/files",
                    "files-chain-attach-file",
                    serde_json::json!({
                        "file_id": "file_chain_primary"
                    }),
                )
                .await,
        )
        .await;
        assert_eq!(create_vector_store_file_payload["route"], "primary");

        let get_vector_store_file_payload = response_json(
            harness
                .empty_request(
                    Method::GET,
                    "/v1/vector_stores/vs_chain_primary/files/file_chain_primary",
                    "files-chain-get-vector-store-file",
                )
                .await,
        )
        .await;
        assert_eq!(get_vector_store_file_payload["route"], "primary");

        let search_payload = response_json(
            harness
                .json_request(
                    Method::POST,
                    "/v1/vector_stores/vs_chain_primary/search",
                    "files-chain-search",
                    serde_json::json!({
                        "query": "find file_chain_primary"
                    }),
                )
                .await,
        )
        .await;
        assert_eq!(search_payload["data"][0]["route"], "primary");

        assert_eq!(primary.hit_count("/v1/files"), 1);
        assert_eq!(primary.hit_count("/v1/files/file_chain_primary"), 1);
        assert_eq!(primary.hit_count("/v1/files/file_chain_primary/content"), 1);
        assert_eq!(primary.hit_count("/v1/vector_stores"), 1);
        assert_eq!(primary.hit_count("/v1/vector_stores/vs_chain_primary"), 1);
        assert_eq!(
            primary.hit_count("/v1/vector_stores/vs_chain_primary/files"),
            1
        );
        assert_eq!(
            primary.hit_count("/v1/vector_stores/vs_chain_primary/files/file_chain_primary"),
            1
        );
        assert_eq!(
            primary.hit_count("/v1/vector_stores/vs_chain_primary/search"),
            1
        );
        assert_eq!(fallback.hit_count("/v1/files"), 1);
        assert_eq!(fallback.hit_count("/v1/vector_stores"), 1);

        harness.cleanup().await;
    }

    #[tokio::test]
    #[ignore = "requires local postgres and redis"]
    async fn batches_chain_keeps_affinity_after_default_route_switch() {
        let primary = MockUpstreamServer::spawn(vec![
            MockRoute::json(
                Method::POST,
                "/v1/batches",
                Some("Bearer sk-primary"),
                Some("/v1/responses"),
                StatusCode::OK,
                serde_json::json!({
                    "id": "batch_chain_primary",
                    "object": "batch",
                    "endpoint": "/v1/responses",
                    "status": "validating",
                    "route": "primary"
                }),
            ),
            MockRoute::json(
                Method::GET,
                "/v1/batches/batch_chain_primary",
                Some("Bearer sk-primary"),
                None,
                StatusCode::OK,
                serde_json::json!({
                    "id": "batch_chain_primary",
                    "object": "batch",
                    "status": "completed",
                    "route": "primary"
                }),
            ),
            MockRoute::json(
                Method::POST,
                "/v1/batches/batch_chain_primary/cancel",
                Some("Bearer sk-primary"),
                None,
                StatusCode::OK,
                serde_json::json!({
                    "id": "batch_chain_primary",
                    "object": "batch",
                    "status": "cancelling",
                    "route": "primary"
                }),
            ),
        ])
        .await;
        let fallback = MockUpstreamServer::spawn(vec![MockRoute::json(
            Method::GET,
            "/v1/batches",
            Some("Bearer sk-fallback"),
            None,
            StatusCode::OK,
            serde_json::json!({
                "object": "list",
                "data": [],
                "route": "fallback"
            }),
        )])
        .await;
        let harness =
            TestHarness::batches_affinity_fixture(&primary.base_url, &fallback.base_url).await;

        let create_batch_payload = response_json(
            harness
                .json_request(
                    Method::POST,
                    "/v1/batches",
                    "batches-chain-create",
                    serde_json::json!({
                        "input_file_id": "file_input_batch",
                        "endpoint": "/v1/responses",
                        "completion_window": "24h"
                    }),
                )
                .await,
        )
        .await;
        assert_eq!(create_batch_payload["id"], "batch_chain_primary");

        harness.promote_fallback_for_scopes(&["batches"]).await;

        let list_batches_payload = response_json(
            harness
                .empty_request(Method::GET, "/v1/batches", "batches-chain-list")
                .await,
        )
        .await;
        assert_eq!(list_batches_payload["route"], "fallback");

        let get_batch_payload = response_json(
            harness
                .empty_request(
                    Method::GET,
                    "/v1/batches/batch_chain_primary",
                    "batches-chain-get",
                )
                .await,
        )
        .await;
        assert_eq!(get_batch_payload["route"], "primary");

        let cancel_batch_payload = response_json(
            harness
                .empty_request(
                    Method::POST,
                    "/v1/batches/batch_chain_primary/cancel",
                    "batches-chain-cancel",
                )
                .await,
        )
        .await;
        assert_eq!(cancel_batch_payload["route"], "primary");

        assert_eq!(primary.hit_count("/v1/batches"), 1);
        assert_eq!(primary.hit_count("/v1/batches/batch_chain_primary"), 1);
        assert_eq!(
            primary.hit_count("/v1/batches/batch_chain_primary/cancel"),
            1
        );
        assert_eq!(fallback.hit_count("/v1/batches"), 1);

        harness.cleanup().await;
    }

    #[tokio::test]
    #[ignore = "requires local postgres and redis"]
    async fn vector_store_file_batch_chain_keeps_affinity_after_default_route_switch() {
        let primary = MockUpstreamServer::spawn(vec![
            MockRoute::json(
                Method::POST,
                "/v1/vector_stores",
                Some("Bearer sk-primary"),
                Some("vs-batch-primary"),
                StatusCode::OK,
                serde_json::json!({
                    "id": "vs_batch_primary",
                    "object": "vector_store",
                    "name": "vs-batch-primary",
                    "route": "primary"
                }),
            ),
            MockRoute::json(
                Method::GET,
                "/v1/vector_stores/vs_batch_primary/files",
                Some("Bearer sk-primary"),
                None,
                StatusCode::OK,
                serde_json::json!({
                    "object": "list",
                    "data": [{
                        "id": "file_chain_primary",
                        "object": "vector_store.file",
                        "route": "primary"
                    }]
                }),
            ),
            MockRoute::json(
                Method::POST,
                "/v1/vector_stores/vs_batch_primary/file_batches",
                Some("Bearer sk-primary"),
                Some("file_chain_primary"),
                StatusCode::OK,
                serde_json::json!({
                    "id": "vsfb_chain_primary",
                    "object": "vector_store.file_batch",
                    "vector_store_id": "vs_batch_primary",
                    "status": "in_progress",
                    "route": "primary"
                }),
            ),
            MockRoute::json(
                Method::GET,
                "/v1/vector_stores/vs_batch_primary/file_batches",
                Some("Bearer sk-primary"),
                None,
                StatusCode::OK,
                serde_json::json!({
                    "object": "list",
                    "data": [{
                        "id": "vsfb_chain_primary",
                        "object": "vector_store.file_batch",
                        "route": "primary"
                    }]
                }),
            ),
            MockRoute::json(
                Method::GET,
                "/v1/vector_stores/vs_batch_primary/file_batches/vsfb_chain_primary",
                Some("Bearer sk-primary"),
                None,
                StatusCode::OK,
                serde_json::json!({
                    "id": "vsfb_chain_primary",
                    "object": "vector_store.file_batch",
                    "vector_store_id": "vs_batch_primary",
                    "route": "primary"
                }),
            ),
            MockRoute::json(
                Method::POST,
                "/v1/vector_stores/vs_batch_primary/file_batches/vsfb_chain_primary/cancel",
                Some("Bearer sk-primary"),
                None,
                StatusCode::OK,
                serde_json::json!({
                    "id": "vsfb_chain_primary",
                    "object": "vector_store.file_batch",
                    "vector_store_id": "vs_batch_primary",
                    "status": "cancelled",
                    "route": "primary"
                }),
            ),
        ])
        .await;
        let fallback = MockUpstreamServer::spawn(vec![MockRoute::json(
            Method::GET,
            "/v1/vector_stores",
            Some("Bearer sk-fallback"),
            None,
            StatusCode::OK,
            serde_json::json!({
                "object": "list",
                "data": [],
                "route": "fallback"
            }),
        )])
        .await;
        let harness = TestHarness::files_vector_stores_affinity_fixture(
            &primary.base_url,
            &fallback.base_url,
        )
        .await;

        let create_vector_store_payload = response_json(
            harness
                .json_request(
                    Method::POST,
                    "/v1/vector_stores",
                    "vs-file-batch-create-store",
                    serde_json::json!({
                        "name": "vs-batch-primary"
                    }),
                )
                .await,
        )
        .await;
        assert_eq!(create_vector_store_payload["id"], "vs_batch_primary");

        harness
            .promote_fallback_for_scopes(&["vector_stores"])
            .await;

        let list_vector_stores_payload = response_json(
            harness
                .empty_request(
                    Method::GET,
                    "/v1/vector_stores",
                    "vs-file-batch-list-stores",
                )
                .await,
        )
        .await;
        assert_eq!(list_vector_stores_payload["route"], "fallback");

        let list_vector_store_files_payload = response_json(
            harness
                .empty_request(
                    Method::GET,
                    "/v1/vector_stores/vs_batch_primary/files",
                    "vs-file-batch-list-files",
                )
                .await,
        )
        .await;
        assert_eq!(
            list_vector_store_files_payload["data"][0]["route"],
            "primary"
        );

        let create_file_batch_payload = response_json(
            harness
                .json_request(
                    Method::POST,
                    "/v1/vector_stores/vs_batch_primary/file_batches",
                    "vs-file-batch-create-batch",
                    serde_json::json!({
                        "file_ids": ["file_chain_primary"]
                    }),
                )
                .await,
        )
        .await;
        assert_eq!(create_file_batch_payload["id"], "vsfb_chain_primary");

        let list_file_batches_payload = response_json(
            harness
                .empty_request(
                    Method::GET,
                    "/v1/vector_stores/vs_batch_primary/file_batches",
                    "vs-file-batch-list-batches",
                )
                .await,
        )
        .await;
        assert_eq!(list_file_batches_payload["data"][0]["route"], "primary");

        let get_file_batch_payload = response_json(
            harness
                .empty_request(
                    Method::GET,
                    "/v1/vector_stores/vs_batch_primary/file_batches/vsfb_chain_primary",
                    "vs-file-batch-get-batch",
                )
                .await,
        )
        .await;
        assert_eq!(get_file_batch_payload["route"], "primary");

        let cancel_file_batch_payload = response_json(
            harness
                .empty_request(
                    Method::POST,
                    "/v1/vector_stores/vs_batch_primary/file_batches/vsfb_chain_primary/cancel",
                    "vs-file-batch-cancel-batch",
                )
                .await,
        )
        .await;
        assert_eq!(cancel_file_batch_payload["route"], "primary");

        assert_eq!(primary.hit_count("/v1/vector_stores"), 1);
        assert_eq!(
            primary.hit_count("/v1/vector_stores/vs_batch_primary/files"),
            1
        );
        assert_eq!(
            primary.hit_count("/v1/vector_stores/vs_batch_primary/file_batches"),
            2
        );
        assert_eq!(
            primary.hit_count("/v1/vector_stores/vs_batch_primary/file_batches/vsfb_chain_primary"),
            1
        );
        assert_eq!(
            primary.hit_count(
                "/v1/vector_stores/vs_batch_primary/file_batches/vsfb_chain_primary/cancel"
            ),
            1
        );
        assert_eq!(fallback.hit_count("/v1/vector_stores"), 1);

        harness.cleanup().await;
    }

    #[tokio::test]
    #[ignore = "requires local postgres and redis"]
    async fn uploads_chain_keeps_affinity_after_default_route_switch() {
        let primary = MockUpstreamServer::spawn(vec![
            MockRoute::json(
                Method::POST,
                "/v1/uploads",
                Some("Bearer sk-primary"),
                Some("upload-chain-primary.bin"),
                StatusCode::OK,
                serde_json::json!({
                    "id": "upload_chain_primary",
                    "object": "upload",
                    "status": "in_progress",
                    "route": "primary"
                }),
            ),
            MockRoute::json(
                Method::GET,
                "/v1/uploads/upload_chain_primary",
                Some("Bearer sk-primary"),
                None,
                StatusCode::OK,
                serde_json::json!({
                    "id": "upload_chain_primary",
                    "object": "upload",
                    "status": "in_progress",
                    "route": "primary"
                }),
            ),
            MockRoute::json(
                Method::POST,
                "/v1/uploads/upload_chain_primary/parts",
                Some("Bearer sk-primary"),
                Some("upload-part.txt"),
                StatusCode::OK,
                serde_json::json!({
                    "id": "part_chain_primary",
                    "object": "upload.part",
                    "route": "primary"
                }),
            ),
            MockRoute::json(
                Method::POST,
                "/v1/uploads/upload_chain_primary/complete",
                Some("Bearer sk-primary"),
                Some("part_chain_primary"),
                StatusCode::OK,
                serde_json::json!({
                    "id": "file_chain_uploaded",
                    "object": "file",
                    "purpose": "assistants",
                    "route": "primary"
                }),
            ),
            MockRoute::json(
                Method::POST,
                "/v1/uploads/upload_chain_primary/cancel",
                Some("Bearer sk-primary"),
                None,
                StatusCode::OK,
                serde_json::json!({
                    "id": "upload_chain_primary",
                    "object": "upload",
                    "status": "cancelled",
                    "route": "primary"
                }),
            ),
        ])
        .await;
        let fallback = MockUpstreamServer::spawn(vec![MockRoute::json(
            Method::POST,
            "/v1/uploads",
            Some("Bearer sk-fallback"),
            Some("upload-chain-fallback.bin"),
            StatusCode::OK,
            serde_json::json!({
                "id": "upload_chain_fallback",
                "object": "upload",
                "status": "in_progress",
                "route": "fallback"
            }),
        )])
        .await;
        let harness =
            TestHarness::uploads_affinity_fixture(&primary.base_url, &fallback.base_url).await;

        let create_upload_payload = response_json(
            harness
                .json_request(
                    Method::POST,
                    "/v1/uploads",
                    "uploads-chain-create-primary",
                    serde_json::json!({
                        "filename": "upload-chain-primary.bin",
                        "purpose": "assistants",
                        "bytes": 18,
                        "mime_type": "application/octet-stream"
                    }),
                )
                .await,
        )
        .await;
        assert_eq!(create_upload_payload["id"], "upload_chain_primary");

        harness.promote_fallback_for_scopes(&["uploads"]).await;

        let create_upload_fallback_payload = response_json(
            harness
                .json_request(
                    Method::POST,
                    "/v1/uploads",
                    "uploads-chain-create-fallback",
                    serde_json::json!({
                        "filename": "upload-chain-fallback.bin",
                        "purpose": "assistants",
                        "bytes": 20,
                        "mime_type": "application/octet-stream"
                    }),
                )
                .await,
        )
        .await;
        assert_eq!(create_upload_fallback_payload["route"], "fallback");

        let get_upload_payload = response_json(
            harness
                .empty_request(
                    Method::GET,
                    "/v1/uploads/upload_chain_primary",
                    "uploads-chain-get-primary",
                )
                .await,
        )
        .await;
        assert_eq!(get_upload_payload["route"], "primary");

        let add_part_payload = response_json(
            harness
                .multipart_request(MultipartRequestSpec {
                    uri: "/v1/uploads/upload_chain_primary/parts",
                    request_id: "uploads-chain-add-part",
                    text_fields: &[],
                    file_field_name: "data",
                    file_name: "upload-part.txt",
                    file_content_type: "text/plain",
                    file_bytes: b"hello upload part",
                })
                .await,
        )
        .await;
        assert_eq!(add_part_payload["route"], "primary");

        let complete_upload_payload = response_json(
            harness
                .json_request(
                    Method::POST,
                    "/v1/uploads/upload_chain_primary/complete",
                    "uploads-chain-complete-primary",
                    serde_json::json!({
                        "part_ids": ["part_chain_primary"]
                    }),
                )
                .await,
        )
        .await;
        assert_eq!(complete_upload_payload["route"], "primary");

        let cancel_upload_payload = response_json(
            harness
                .empty_request(
                    Method::POST,
                    "/v1/uploads/upload_chain_primary/cancel",
                    "uploads-chain-cancel-primary",
                )
                .await,
        )
        .await;
        assert_eq!(cancel_upload_payload["route"], "primary");

        assert_eq!(primary.hit_count("/v1/uploads"), 1);
        assert_eq!(primary.hit_count("/v1/uploads/upload_chain_primary"), 1);
        assert_eq!(
            primary.hit_count("/v1/uploads/upload_chain_primary/parts"),
            1
        );
        assert_eq!(
            primary.hit_count("/v1/uploads/upload_chain_primary/complete"),
            1
        );
        assert_eq!(
            primary.hit_count("/v1/uploads/upload_chain_primary/cancel"),
            1
        );
        assert_eq!(fallback.hit_count("/v1/uploads"), 1);

        harness.cleanup().await;
    }

    #[tokio::test]
    #[ignore = "requires local postgres and redis"]
    async fn fine_tuning_chain_keeps_affinity_after_default_route_switch() {
        let primary = MockUpstreamServer::spawn(vec![
            MockRoute::json(
                Method::POST,
                "/v1/fine_tuning/jobs",
                Some("Bearer sk-primary"),
                Some("file_train_primary"),
                StatusCode::OK,
                serde_json::json!({
                    "id": "ftjob_chain_primary",
                    "object": "fine_tuning.job",
                    "status": "validating_files",
                    "route": "primary"
                }),
            ),
            MockRoute::json(
                Method::GET,
                "/v1/fine_tuning/jobs/ftjob_chain_primary",
                Some("Bearer sk-primary"),
                None,
                StatusCode::OK,
                serde_json::json!({
                    "id": "ftjob_chain_primary",
                    "object": "fine_tuning.job",
                    "status": "succeeded",
                    "route": "primary"
                }),
            ),
            MockRoute::json(
                Method::POST,
                "/v1/fine_tuning/jobs/ftjob_chain_primary/cancel",
                Some("Bearer sk-primary"),
                None,
                StatusCode::OK,
                serde_json::json!({
                    "id": "ftjob_chain_primary",
                    "object": "fine_tuning.job",
                    "status": "cancelled",
                    "route": "primary"
                }),
            ),
            MockRoute::json(
                Method::GET,
                "/v1/fine_tuning/jobs/ftjob_chain_primary/events",
                Some("Bearer sk-primary"),
                None,
                StatusCode::OK,
                serde_json::json!({
                    "object": "list",
                    "data": [{
                        "id": "ftevent_chain_primary",
                        "object": "fine_tuning.job.event",
                        "route": "primary"
                    }]
                }),
            ),
            MockRoute::json(
                Method::GET,
                "/v1/fine_tuning/jobs/ftjob_chain_primary/checkpoints",
                Some("Bearer sk-primary"),
                None,
                StatusCode::OK,
                serde_json::json!({
                    "object": "list",
                    "data": [{
                        "id": "ftckpt_chain_primary",
                        "object": "fine_tuning.job.checkpoint",
                        "route": "primary"
                    }]
                }),
            ),
        ])
        .await;
        let fallback = MockUpstreamServer::spawn(vec![MockRoute::json(
            Method::GET,
            "/v1/fine_tuning/jobs",
            Some("Bearer sk-fallback"),
            None,
            StatusCode::OK,
            serde_json::json!({
                "object": "list",
                "data": [],
                "route": "fallback"
            }),
        )])
        .await;
        let harness =
            TestHarness::fine_tuning_affinity_fixture(&primary.base_url, &fallback.base_url).await;

        let create_job_payload = response_json(
            harness
                .json_request(
                    Method::POST,
                    "/v1/fine_tuning/jobs",
                    "fine-tuning-chain-create",
                    serde_json::json!({
                        "training_file": "file_train_primary",
                        "model": harness.model_name
                    }),
                )
                .await,
        )
        .await;
        assert_eq!(
            create_job_payload["id"], "ftjob_chain_primary",
            "unexpected create fine-tuning job payload: {create_job_payload}"
        );

        harness.promote_fallback_for_scopes(&["fine_tuning"]).await;

        let list_jobs_payload = response_json(
            harness
                .empty_request(
                    Method::GET,
                    "/v1/fine_tuning/jobs",
                    "fine-tuning-chain-list",
                )
                .await,
        )
        .await;
        assert_eq!(list_jobs_payload["route"], "fallback");

        let get_job_payload = response_json(
            harness
                .empty_request(
                    Method::GET,
                    "/v1/fine_tuning/jobs/ftjob_chain_primary",
                    "fine-tuning-chain-get",
                )
                .await,
        )
        .await;
        assert_eq!(get_job_payload["route"], "primary");

        let list_events_payload = response_json(
            harness
                .empty_request(
                    Method::GET,
                    "/v1/fine_tuning/jobs/ftjob_chain_primary/events",
                    "fine-tuning-chain-events",
                )
                .await,
        )
        .await;
        assert_eq!(list_events_payload["data"][0]["route"], "primary");

        let list_checkpoints_payload = response_json(
            harness
                .empty_request(
                    Method::GET,
                    "/v1/fine_tuning/jobs/ftjob_chain_primary/checkpoints",
                    "fine-tuning-chain-checkpoints",
                )
                .await,
        )
        .await;
        assert_eq!(list_checkpoints_payload["data"][0]["route"], "primary");

        let cancel_job_payload = response_json(
            harness
                .empty_request(
                    Method::POST,
                    "/v1/fine_tuning/jobs/ftjob_chain_primary/cancel",
                    "fine-tuning-chain-cancel",
                )
                .await,
        )
        .await;
        assert_eq!(cancel_job_payload["route"], "primary");

        assert_eq!(primary.hit_count("/v1/fine_tuning/jobs"), 1);
        assert_eq!(
            primary.hit_count("/v1/fine_tuning/jobs/ftjob_chain_primary"),
            1
        );
        assert_eq!(
            primary.hit_count("/v1/fine_tuning/jobs/ftjob_chain_primary/events"),
            1
        );
        assert_eq!(
            primary.hit_count("/v1/fine_tuning/jobs/ftjob_chain_primary/checkpoints"),
            1
        );
        assert_eq!(
            primary.hit_count("/v1/fine_tuning/jobs/ftjob_chain_primary/cancel"),
            1
        );
        assert_eq!(fallback.hit_count("/v1/fine_tuning/jobs"), 1);

        harness.cleanup().await;
    }

    #[tokio::test]
    #[ignore = "requires local postgres and redis"]
    async fn file_and_vector_store_delete_clear_affinity_after_successful_delete() {
        let primary = MockUpstreamServer::spawn(vec![
            MockRoute::json(
                Method::POST,
                "/v1/files",
                Some("Bearer sk-primary"),
                Some("delete-me.txt"),
                StatusCode::OK,
                serde_json::json!({
                    "id": "file_delete_primary",
                    "object": "file",
                    "purpose": "assistants",
                    "route": "primary"
                }),
            ),
            MockRoute::json(
                Method::DELETE,
                "/v1/files/file_delete_primary",
                Some("Bearer sk-primary"),
                None,
                StatusCode::OK,
                serde_json::json!({
                    "id": "file_delete_primary",
                    "object": "file.deleted",
                    "deleted": true,
                    "route": "primary"
                }),
            ),
            MockRoute::json(
                Method::GET,
                "/v1/files/file_delete_primary",
                Some("Bearer sk-primary"),
                None,
                StatusCode::OK,
                serde_json::json!({
                    "id": "file_delete_primary",
                    "object": "file",
                    "route": "stale-primary"
                }),
            ),
            MockRoute::json(
                Method::POST,
                "/v1/vector_stores",
                Some("Bearer sk-primary"),
                Some("vs-delete-primary"),
                StatusCode::OK,
                serde_json::json!({
                    "id": "vs_delete_primary",
                    "object": "vector_store",
                    "route": "primary"
                }),
            ),
            MockRoute::json(
                Method::DELETE,
                "/v1/vector_stores/vs_delete_primary",
                Some("Bearer sk-primary"),
                None,
                StatusCode::OK,
                serde_json::json!({
                    "id": "vs_delete_primary",
                    "object": "vector_store.deleted",
                    "deleted": true,
                    "route": "primary"
                }),
            ),
            MockRoute::json(
                Method::GET,
                "/v1/vector_stores/vs_delete_primary",
                Some("Bearer sk-primary"),
                None,
                StatusCode::OK,
                serde_json::json!({
                    "id": "vs_delete_primary",
                    "object": "vector_store",
                    "route": "stale-primary"
                }),
            ),
        ])
        .await;
        let fallback = MockUpstreamServer::spawn(vec![
            MockRoute::json(
                Method::GET,
                "/v1/files/file_delete_primary",
                Some("Bearer sk-fallback"),
                None,
                StatusCode::OK,
                serde_json::json!({
                    "id": "file_delete_primary",
                    "object": "file",
                    "route": "fallback"
                }),
            ),
            MockRoute::json(
                Method::GET,
                "/v1/vector_stores/vs_delete_primary",
                Some("Bearer sk-fallback"),
                None,
                StatusCode::OK,
                serde_json::json!({
                    "id": "vs_delete_primary",
                    "object": "vector_store",
                    "route": "fallback"
                }),
            ),
        ])
        .await;
        let harness = TestHarness::files_vector_stores_affinity_fixture(
            &primary.base_url,
            &fallback.base_url,
        )
        .await;

        let create_file_payload = response_json(
            harness
                .multipart_request(MultipartRequestSpec {
                    uri: "/v1/files",
                    request_id: "delete-affinity-create-file",
                    text_fields: &[("purpose", "assistants")],
                    file_field_name: "file",
                    file_name: "delete-me.txt",
                    file_content_type: "text/plain",
                    file_bytes: b"delete affinity file",
                })
                .await,
        )
        .await;
        assert_eq!(create_file_payload["id"], "file_delete_primary");

        let create_vector_store_payload = response_json(
            harness
                .json_request(
                    Method::POST,
                    "/v1/vector_stores",
                    "delete-affinity-create-vector-store",
                    serde_json::json!({
                        "name": "vs-delete-primary"
                    }),
                )
                .await,
        )
        .await;
        assert_eq!(create_vector_store_payload["id"], "vs_delete_primary");

        let delete_file_payload = response_json(
            harness
                .empty_request(
                    Method::DELETE,
                    "/v1/files/file_delete_primary",
                    "delete-affinity-delete-file",
                )
                .await,
        )
        .await;
        assert_eq!(delete_file_payload["route"], "primary");

        let delete_vector_store_payload = response_json(
            harness
                .empty_request(
                    Method::DELETE,
                    "/v1/vector_stores/vs_delete_primary",
                    "delete-affinity-delete-vector-store",
                )
                .await,
        )
        .await;
        assert_eq!(delete_vector_store_payload["route"], "primary");

        harness
            .promote_fallback_for_scopes(&["files", "vector_stores"])
            .await;

        let get_deleted_file_payload = response_json(
            harness
                .empty_request(
                    Method::GET,
                    "/v1/files/file_delete_primary",
                    "delete-affinity-get-file",
                )
                .await,
        )
        .await;
        assert_eq!(get_deleted_file_payload["route"], "fallback");

        let get_deleted_vector_store_payload = response_json(
            harness
                .empty_request(
                    Method::GET,
                    "/v1/vector_stores/vs_delete_primary",
                    "delete-affinity-get-vector-store",
                )
                .await,
        )
        .await;
        assert_eq!(get_deleted_vector_store_payload["route"], "fallback");

        assert_eq!(primary.hit_count("/v1/files"), 1);
        assert_eq!(primary.hit_count("/v1/files/file_delete_primary"), 1);
        assert_eq!(primary.hit_count("/v1/vector_stores"), 1);
        assert_eq!(primary.hit_count("/v1/vector_stores/vs_delete_primary"), 1);
        assert_eq!(fallback.hit_count("/v1/files/file_delete_primary"), 1);
        assert_eq!(fallback.hit_count("/v1/vector_stores/vs_delete_primary"), 1);

        harness.cleanup().await;
    }

    #[tokio::test]
    #[ignore = "requires local postgres and redis"]
    async fn assistant_and_thread_delete_clear_affinity_after_successful_delete() {
        let primary = MockUpstreamServer::spawn(vec![
            MockRoute::json(
                Method::POST,
                "/v1/assistants",
                Some("Bearer sk-primary"),
                None,
                StatusCode::OK,
                serde_json::json!({
                    "id": "asst_delete_primary",
                    "object": "assistant",
                    "model": "__MODEL__",
                    "route": "primary"
                }),
            ),
            MockRoute::json(
                Method::POST,
                "/v1/threads",
                Some("Bearer sk-primary"),
                None,
                StatusCode::OK,
                serde_json::json!({
                    "id": "thread_delete_primary",
                    "object": "thread",
                    "route": "primary"
                }),
            ),
            MockRoute::json(
                Method::DELETE,
                "/v1/assistants/asst_delete_primary",
                Some("Bearer sk-primary"),
                None,
                StatusCode::OK,
                serde_json::json!({
                    "id": "asst_delete_primary",
                    "object": "assistant.deleted",
                    "deleted": true,
                    "route": "primary"
                }),
            ),
            MockRoute::json(
                Method::GET,
                "/v1/assistants/asst_delete_primary",
                Some("Bearer sk-primary"),
                None,
                StatusCode::OK,
                serde_json::json!({
                    "id": "asst_delete_primary",
                    "object": "assistant",
                    "route": "stale-primary"
                }),
            ),
            MockRoute::json(
                Method::DELETE,
                "/v1/threads/thread_delete_primary",
                Some("Bearer sk-primary"),
                None,
                StatusCode::OK,
                serde_json::json!({
                    "id": "thread_delete_primary",
                    "object": "thread.deleted",
                    "deleted": true,
                    "route": "primary"
                }),
            ),
            MockRoute::json(
                Method::GET,
                "/v1/threads/thread_delete_primary",
                Some("Bearer sk-primary"),
                None,
                StatusCode::OK,
                serde_json::json!({
                    "id": "thread_delete_primary",
                    "object": "thread",
                    "route": "stale-primary"
                }),
            ),
        ])
        .await;
        let fallback = MockUpstreamServer::spawn(vec![
            MockRoute::json(
                Method::GET,
                "/v1/assistants/asst_delete_primary",
                Some("Bearer sk-fallback"),
                None,
                StatusCode::OK,
                serde_json::json!({
                    "id": "asst_delete_primary",
                    "object": "assistant",
                    "route": "fallback"
                }),
            ),
            MockRoute::json(
                Method::GET,
                "/v1/threads/thread_delete_primary",
                Some("Bearer sk-fallback"),
                None,
                StatusCode::OK,
                serde_json::json!({
                    "id": "thread_delete_primary",
                    "object": "thread",
                    "route": "fallback"
                }),
            ),
        ])
        .await;
        let harness = TestHarness::assistants_threads_fallback_affinity_fixture(
            &primary.base_url,
            &fallback.base_url,
        )
        .await;
        primary.replace_placeholder("__MODEL__", &harness.model_name);

        let create_assistant_payload = response_json(
            harness
                .json_request(
                    Method::POST,
                    "/v1/assistants",
                    "delete-affinity-create-assistant",
                    serde_json::json!({
                        "model": harness.model_name,
                        "name": "delete-affinity-assistant"
                    }),
                )
                .await,
        )
        .await;
        assert_eq!(create_assistant_payload["id"], "asst_delete_primary");

        let create_thread_payload = response_json(
            harness
                .json_request(
                    Method::POST,
                    "/v1/threads",
                    "delete-affinity-create-thread",
                    serde_json::json!({
                        "messages": [{
                            "role": "user",
                            "content": "hello"
                        }]
                    }),
                )
                .await,
        )
        .await;
        assert_eq!(create_thread_payload["id"], "thread_delete_primary");

        let delete_assistant_payload = response_json(
            harness
                .empty_request(
                    Method::DELETE,
                    "/v1/assistants/asst_delete_primary",
                    "delete-affinity-delete-assistant",
                )
                .await,
        )
        .await;
        assert_eq!(delete_assistant_payload["route"], "primary");

        let delete_thread_payload = response_json(
            harness
                .empty_request(
                    Method::DELETE,
                    "/v1/threads/thread_delete_primary",
                    "delete-affinity-delete-thread",
                )
                .await,
        )
        .await;
        assert_eq!(delete_thread_payload["route"], "primary");

        harness
            .promote_fallback_for_scopes(&["assistants", "threads"])
            .await;

        let get_deleted_assistant_payload = response_json(
            harness
                .empty_request(
                    Method::GET,
                    "/v1/assistants/asst_delete_primary",
                    "delete-affinity-get-assistant",
                )
                .await,
        )
        .await;
        assert_eq!(get_deleted_assistant_payload["route"], "fallback");

        let get_deleted_thread_payload = response_json(
            harness
                .empty_request(
                    Method::GET,
                    "/v1/threads/thread_delete_primary",
                    "delete-affinity-get-thread",
                )
                .await,
        )
        .await;
        assert_eq!(get_deleted_thread_payload["route"], "fallback");

        assert_eq!(primary.hit_count("/v1/assistants"), 1);
        assert_eq!(primary.hit_count("/v1/assistants/asst_delete_primary"), 1);
        assert_eq!(primary.hit_count("/v1/threads"), 1);
        assert_eq!(primary.hit_count("/v1/threads/thread_delete_primary"), 1);
        assert_eq!(fallback.hit_count("/v1/assistants/asst_delete_primary"), 1);
        assert_eq!(fallback.hit_count("/v1/threads/thread_delete_primary"), 1);

        harness.cleanup().await;
    }

    #[tokio::test]
    #[ignore = "requires local postgres and redis"]
    async fn upload_complete_binds_file_affinity_after_default_route_switch() {
        let primary = MockUpstreamServer::spawn(vec![
            MockRoute::json(
                Method::POST,
                "/v1/uploads",
                Some("Bearer sk-primary"),
                Some("upload-chain-primary.bin"),
                StatusCode::OK,
                serde_json::json!({
                    "id": "upload_chain_primary",
                    "object": "upload",
                    "status": "in_progress",
                    "route": "primary"
                }),
            ),
            MockRoute::json(
                Method::POST,
                "/v1/uploads/upload_chain_primary/parts",
                Some("Bearer sk-primary"),
                Some("upload-part.txt"),
                StatusCode::OK,
                serde_json::json!({
                    "id": "part_chain_primary",
                    "object": "upload.part",
                    "route": "primary"
                }),
            ),
            MockRoute::json(
                Method::POST,
                "/v1/uploads/upload_chain_primary/complete",
                Some("Bearer sk-primary"),
                Some("part_chain_primary"),
                StatusCode::OK,
                serde_json::json!({
                    "id": "file_completed_primary",
                    "object": "file",
                    "purpose": "assistants",
                    "route": "primary"
                }),
            ),
            MockRoute::json(
                Method::GET,
                "/v1/uploads/upload_chain_primary",
                Some("Bearer sk-primary"),
                None,
                StatusCode::OK,
                serde_json::json!({
                    "id": "upload_chain_primary",
                    "object": "upload",
                    "route": "primary"
                }),
            ),
            MockRoute::json(
                Method::GET,
                "/v1/files/file_completed_primary",
                Some("Bearer sk-primary"),
                None,
                StatusCode::OK,
                serde_json::json!({
                    "id": "file_completed_primary",
                    "object": "file",
                    "route": "primary"
                }),
            ),
        ])
        .await;
        let fallback = MockUpstreamServer::spawn(vec![
            MockRoute::json(
                Method::POST,
                "/v1/uploads",
                Some("Bearer sk-fallback"),
                Some("upload-chain-fallback.bin"),
                StatusCode::OK,
                serde_json::json!({
                    "id": "upload_chain_fallback",
                    "object": "upload",
                    "status": "in_progress",
                    "route": "fallback"
                }),
            ),
            MockRoute::json(
                Method::GET,
                "/v1/uploads/upload_chain_primary",
                Some("Bearer sk-fallback"),
                None,
                StatusCode::OK,
                serde_json::json!({
                    "id": "upload_chain_primary",
                    "object": "upload",
                    "route": "fallback"
                }),
            ),
            MockRoute::json(
                Method::GET,
                "/v1/files/file_completed_primary",
                Some("Bearer sk-fallback"),
                None,
                StatusCode::OK,
                serde_json::json!({
                    "id": "file_completed_primary",
                    "object": "file",
                    "route": "fallback"
                }),
            ),
        ])
        .await;
        let harness =
            TestHarness::uploads_files_affinity_fixture(&primary.base_url, &fallback.base_url)
                .await;

        let create_upload_payload = response_json(
            harness
                .json_request(
                    Method::POST,
                    "/v1/uploads",
                    "uploads-file-affinity-create-primary",
                    serde_json::json!({
                        "filename": "upload-chain-primary.bin",
                        "purpose": "assistants",
                        "bytes": 18,
                        "mime_type": "application/octet-stream"
                    }),
                )
                .await,
        )
        .await;
        assert_eq!(create_upload_payload["id"], "upload_chain_primary");

        let add_part_payload = response_json(
            harness
                .multipart_request(MultipartRequestSpec {
                    uri: "/v1/uploads/upload_chain_primary/parts",
                    request_id: "uploads-file-affinity-add-part",
                    text_fields: &[],
                    file_field_name: "data",
                    file_name: "upload-part.txt",
                    file_content_type: "text/plain",
                    file_bytes: b"hello upload part",
                })
                .await,
        )
        .await;
        assert_eq!(add_part_payload["id"], "part_chain_primary");

        let complete_upload_payload = response_json(
            harness
                .json_request(
                    Method::POST,
                    "/v1/uploads/upload_chain_primary/complete",
                    "uploads-file-affinity-complete",
                    serde_json::json!({
                        "part_ids": ["part_chain_primary"]
                    }),
                )
                .await,
        )
        .await;
        assert_eq!(complete_upload_payload["id"], "file_completed_primary");

        harness
            .promote_fallback_for_scopes(&["uploads", "files"])
            .await;

        let create_fallback_upload_payload = response_json(
            harness
                .json_request(
                    Method::POST,
                    "/v1/uploads",
                    "uploads-file-affinity-create-fallback",
                    serde_json::json!({
                        "filename": "upload-chain-fallback.bin",
                        "purpose": "assistants",
                        "bytes": 20,
                        "mime_type": "application/octet-stream"
                    }),
                )
                .await,
        )
        .await;
        assert_eq!(create_fallback_upload_payload["route"], "fallback");

        let get_original_upload_payload = response_json(
            harness
                .empty_request(
                    Method::GET,
                    "/v1/uploads/upload_chain_primary",
                    "uploads-file-affinity-get-upload",
                )
                .await,
        )
        .await;
        assert_eq!(get_original_upload_payload["route"], "primary");

        let get_completed_file_payload = response_json(
            harness
                .empty_request(
                    Method::GET,
                    "/v1/files/file_completed_primary",
                    "uploads-file-affinity-get-file",
                )
                .await,
        )
        .await;
        assert_eq!(get_completed_file_payload["route"], "primary");

        assert_eq!(primary.hit_count("/v1/uploads"), 1);
        assert_eq!(
            primary.hit_count("/v1/uploads/upload_chain_primary/parts"),
            1
        );
        assert_eq!(
            primary.hit_count("/v1/uploads/upload_chain_primary/complete"),
            1
        );
        assert_eq!(primary.hit_count("/v1/uploads/upload_chain_primary"), 1);
        assert_eq!(primary.hit_count("/v1/files/file_completed_primary"), 1);
        assert_eq!(fallback.hit_count("/v1/uploads"), 1);
        assert_eq!(fallback.hit_count("/v1/uploads/upload_chain_primary"), 0);
        assert_eq!(fallback.hit_count("/v1/files/file_completed_primary"), 0);

        harness.cleanup().await;
    }

    #[tokio::test]
    #[ignore = "requires local postgres and redis"]
    async fn delete_vector_store_file_keeps_file_and_vector_store_affinity() {
        let primary = MockUpstreamServer::spawn(vec![
            MockRoute::json(
                Method::POST,
                "/v1/vector_stores",
                Some("Bearer sk-primary"),
                Some("vs-keep-primary"),
                StatusCode::OK,
                serde_json::json!({
                    "id": "vs_keep_primary",
                    "object": "vector_store",
                    "route": "primary"
                }),
            ),
            MockRoute::json(
                Method::POST,
                "/v1/vector_stores/vs_keep_primary/files",
                Some("Bearer sk-primary"),
                Some("file_keep_primary"),
                StatusCode::OK,
                serde_json::json!({
                    "id": "file_keep_primary",
                    "object": "vector_store.file",
                    "vector_store_id": "vs_keep_primary",
                    "route": "primary"
                }),
            ),
            MockRoute::json(
                Method::DELETE,
                "/v1/vector_stores/vs_keep_primary/files/file_keep_primary",
                Some("Bearer sk-primary"),
                None,
                StatusCode::OK,
                serde_json::json!({
                    "id": "file_keep_primary",
                    "object": "vector_store.file.deleted",
                    "deleted": true,
                    "route": "primary"
                }),
            ),
            MockRoute::json(
                Method::GET,
                "/v1/vector_stores/vs_keep_primary",
                Some("Bearer sk-primary"),
                None,
                StatusCode::OK,
                serde_json::json!({
                    "id": "vs_keep_primary",
                    "object": "vector_store",
                    "route": "primary"
                }),
            ),
            MockRoute::json(
                Method::GET,
                "/v1/files/file_keep_primary",
                Some("Bearer sk-primary"),
                None,
                StatusCode::OK,
                serde_json::json!({
                    "id": "file_keep_primary",
                    "object": "file",
                    "route": "primary"
                }),
            ),
        ])
        .await;
        let fallback = MockUpstreamServer::spawn(vec![
            MockRoute::json(
                Method::GET,
                "/v1/vector_stores/vs_keep_primary",
                Some("Bearer sk-fallback"),
                None,
                StatusCode::OK,
                serde_json::json!({
                    "id": "vs_keep_primary",
                    "object": "vector_store",
                    "route": "fallback"
                }),
            ),
            MockRoute::json(
                Method::GET,
                "/v1/files/file_keep_primary",
                Some("Bearer sk-fallback"),
                None,
                StatusCode::OK,
                serde_json::json!({
                    "id": "file_keep_primary",
                    "object": "file",
                    "route": "fallback"
                }),
            ),
        ])
        .await;
        let harness = TestHarness::files_vector_stores_affinity_fixture(
            &primary.base_url,
            &fallback.base_url,
        )
        .await;

        let create_vector_store_payload = response_json(
            harness
                .json_request(
                    Method::POST,
                    "/v1/vector_stores",
                    "keep-affinity-create-vector-store",
                    serde_json::json!({
                        "name": "vs-keep-primary"
                    }),
                )
                .await,
        )
        .await;
        assert_eq!(create_vector_store_payload["id"], "vs_keep_primary");

        let attach_file_payload = response_json(
            harness
                .json_request(
                    Method::POST,
                    "/v1/vector_stores/vs_keep_primary/files",
                    "keep-affinity-attach-file",
                    serde_json::json!({
                        "file_id": "file_keep_primary"
                    }),
                )
                .await,
        )
        .await;
        assert_eq!(attach_file_payload["id"], "file_keep_primary");

        let delete_vector_store_file_payload = response_json(
            harness
                .empty_request(
                    Method::DELETE,
                    "/v1/vector_stores/vs_keep_primary/files/file_keep_primary",
                    "keep-affinity-delete-vector-store-file",
                )
                .await,
        )
        .await;
        assert_eq!(delete_vector_store_file_payload["route"], "primary");

        harness
            .promote_fallback_for_scopes(&["files", "vector_stores"])
            .await;

        let get_vector_store_payload = response_json(
            harness
                .empty_request(
                    Method::GET,
                    "/v1/vector_stores/vs_keep_primary",
                    "keep-affinity-get-vector-store",
                )
                .await,
        )
        .await;
        assert_eq!(get_vector_store_payload["route"], "primary");

        let get_file_payload = response_json(
            harness
                .empty_request(
                    Method::GET,
                    "/v1/files/file_keep_primary",
                    "keep-affinity-get-file",
                )
                .await,
        )
        .await;
        assert_eq!(get_file_payload["route"], "primary");

        assert_eq!(primary.hit_count("/v1/vector_stores"), 1);
        assert_eq!(
            primary.hit_count("/v1/vector_stores/vs_keep_primary/files"),
            1
        );
        assert_eq!(
            primary.hit_count("/v1/vector_stores/vs_keep_primary/files/file_keep_primary"),
            1
        );
        assert_eq!(primary.hit_count("/v1/vector_stores/vs_keep_primary"), 1);
        assert_eq!(primary.hit_count("/v1/files/file_keep_primary"), 1);
        assert_eq!(fallback.hit_count("/v1/vector_stores/vs_keep_primary"), 0);
        assert_eq!(fallback.hit_count("/v1/files/file_keep_primary"), 0);

        harness.cleanup().await;
    }

    #[tokio::test]
    #[ignore = "requires local postgres and redis"]
    async fn model_passthrough_endpoints_follow_default_route_switch() {
        let primary = MockUpstreamServer::spawn(vec![
            MockRoute::json(
                Method::POST,
                "/v1/completions",
                Some("Bearer sk-primary"),
                Some("tell me a joke"),
                StatusCode::OK,
                serde_json::json!({
                    "id": "cmpl_primary",
                    "object": "text_completion",
                    "model": "__MODEL__",
                    "choices": [{
                        "index": 0,
                        "text": "primary",
                        "finish_reason": "stop"
                    }],
                    "usage": {
                        "prompt_tokens": 2,
                        "completion_tokens": 1,
                        "total_tokens": 3
                    },
                    "route": "primary"
                }),
            ),
            MockRoute::json(
                Method::POST,
                "/v1/images/generations",
                Some("Bearer sk-primary"),
                Some("draw a sunset"),
                StatusCode::OK,
                serde_json::json!({
                    "created": 1,
                    "data": [{
                        "url": "https://primary.example/image.png",
                        "route": "primary"
                    }]
                }),
            ),
            MockRoute::json(
                Method::POST,
                "/v1/images/edits",
                Some("Bearer sk-primary"),
                Some("edit-primary.png"),
                StatusCode::OK,
                serde_json::json!({
                    "created": 1,
                    "data": [{
                        "b64_json": "primary-edit",
                        "route": "primary"
                    }]
                }),
            ),
            MockRoute::json(
                Method::POST,
                "/v1/images/variations",
                Some("Bearer sk-primary"),
                Some("variation-primary.png"),
                StatusCode::OK,
                serde_json::json!({
                    "created": 1,
                    "data": [{
                        "b64_json": "primary-variation",
                        "route": "primary"
                    }]
                }),
            ),
            MockRoute::json(
                Method::POST,
                "/v1/audio/transcriptions",
                Some("Bearer sk-primary"),
                Some("voice-primary.wav"),
                StatusCode::OK,
                serde_json::json!({
                    "text": "primary transcript",
                    "route": "primary"
                }),
            ),
            MockRoute::json(
                Method::POST,
                "/v1/audio/translations",
                Some("Bearer sk-primary"),
                Some("voice-translation-primary.wav"),
                StatusCode::OK,
                serde_json::json!({
                    "text": "primary translation",
                    "route": "primary"
                }),
            ),
            MockRoute::raw(
                Method::POST,
                "/v1/audio/speech",
                Some("Bearer sk-primary"),
                Some("say hello from primary"),
                StatusCode::OK,
                "audio/mpeg",
                "primary-audio",
            ),
            MockRoute::json(
                Method::POST,
                "/v1/moderations",
                Some("Bearer sk-primary"),
                Some("moderate primary text"),
                StatusCode::OK,
                serde_json::json!({
                    "id": "modr_primary",
                    "model": "__MODEL__",
                    "results": [{
                        "flagged": false,
                        "route": "primary"
                    }]
                }),
            ),
            MockRoute::json(
                Method::POST,
                "/v1/rerank",
                Some("Bearer sk-primary"),
                Some("rerank primary query"),
                StatusCode::OK,
                serde_json::json!({
                    "results": [{
                        "index": 0,
                        "relevance_score": 0.91,
                        "route": "primary"
                    }]
                }),
            ),
        ])
        .await;
        let fallback = MockUpstreamServer::spawn(vec![
            MockRoute::json(
                Method::POST,
                "/v1/completions",
                Some("Bearer sk-fallback"),
                Some("tell me a joke"),
                StatusCode::OK,
                serde_json::json!({
                    "id": "cmpl_fallback",
                    "object": "text_completion",
                    "model": "__MODEL__",
                    "choices": [{
                        "index": 0,
                        "text": "fallback",
                        "finish_reason": "stop"
                    }],
                    "usage": {
                        "prompt_tokens": 2,
                        "completion_tokens": 1,
                        "total_tokens": 3
                    },
                    "route": "fallback"
                }),
            ),
            MockRoute::json(
                Method::POST,
                "/v1/images/generations",
                Some("Bearer sk-fallback"),
                Some("draw a sunset"),
                StatusCode::OK,
                serde_json::json!({
                    "created": 1,
                    "data": [{
                        "url": "https://fallback.example/image.png",
                        "route": "fallback"
                    }]
                }),
            ),
            MockRoute::json(
                Method::POST,
                "/v1/images/edits",
                Some("Bearer sk-fallback"),
                Some("edit-fallback.png"),
                StatusCode::OK,
                serde_json::json!({
                    "created": 1,
                    "data": [{
                        "b64_json": "fallback-edit",
                        "route": "fallback"
                    }]
                }),
            ),
            MockRoute::json(
                Method::POST,
                "/v1/images/variations",
                Some("Bearer sk-fallback"),
                Some("variation-fallback.png"),
                StatusCode::OK,
                serde_json::json!({
                    "created": 1,
                    "data": [{
                        "b64_json": "fallback-variation",
                        "route": "fallback"
                    }]
                }),
            ),
            MockRoute::json(
                Method::POST,
                "/v1/audio/transcriptions",
                Some("Bearer sk-fallback"),
                Some("voice-fallback.wav"),
                StatusCode::OK,
                serde_json::json!({
                    "text": "fallback transcript",
                    "route": "fallback"
                }),
            ),
            MockRoute::json(
                Method::POST,
                "/v1/audio/translations",
                Some("Bearer sk-fallback"),
                Some("voice-translation-fallback.wav"),
                StatusCode::OK,
                serde_json::json!({
                    "text": "fallback translation",
                    "route": "fallback"
                }),
            ),
            MockRoute::raw(
                Method::POST,
                "/v1/audio/speech",
                Some("Bearer sk-fallback"),
                Some("say hello from fallback"),
                StatusCode::OK,
                "audio/mpeg",
                "fallback-audio",
            ),
            MockRoute::json(
                Method::POST,
                "/v1/moderations",
                Some("Bearer sk-fallback"),
                Some("moderate fallback text"),
                StatusCode::OK,
                serde_json::json!({
                    "id": "modr_fallback",
                    "model": "__MODEL__",
                    "results": [{
                        "flagged": false,
                        "route": "fallback"
                    }]
                }),
            ),
            MockRoute::json(
                Method::POST,
                "/v1/rerank",
                Some("Bearer sk-fallback"),
                Some("rerank fallback query"),
                StatusCode::OK,
                serde_json::json!({
                    "results": [{
                        "index": 0,
                        "relevance_score": 0.88,
                        "route": "fallback"
                    }]
                }),
            ),
        ])
        .await;
        let harness =
            TestHarness::model_passthrough_affinity_fixture(&primary.base_url, &fallback.base_url)
                .await;
        primary.replace_placeholder("__MODEL__", &harness.model_name);
        fallback.replace_placeholder("__MODEL__", &harness.model_name);

        let completions_primary_payload = response_json(
            harness
                .json_request(
                    Method::POST,
                    "/v1/completions",
                    "model-passthrough-completions-primary",
                    serde_json::json!({
                        "model": harness.model_name,
                        "prompt": "tell me a joke"
                    }),
                )
                .await,
        )
        .await;
        assert_eq!(completions_primary_payload["route"], "primary");

        harness
            .promote_fallback_for_scopes(&[
                "completions",
                "images",
                "audio",
                "moderations",
                "rerank",
            ])
            .await;

        let completions_fallback_payload = response_json(
            harness
                .json_request(
                    Method::POST,
                    "/v1/completions",
                    "model-passthrough-completions-fallback",
                    serde_json::json!({
                        "model": harness.model_name,
                        "prompt": "tell me a joke"
                    }),
                )
                .await,
        )
        .await;
        assert_eq!(completions_fallback_payload["route"], "fallback");

        let image_generations_payload = response_json(
            harness
                .json_request(
                    Method::POST,
                    "/v1/images/generations",
                    "model-passthrough-image-generations",
                    serde_json::json!({
                        "model": harness.model_name,
                        "prompt": "draw a sunset"
                    }),
                )
                .await,
        )
        .await;
        assert_eq!(image_generations_payload["data"][0]["route"], "fallback");

        let image_edits_payload = response_json(
            harness
                .multipart_request(MultipartRequestSpec {
                    uri: "/v1/images/edits",
                    request_id: "model-passthrough-image-edits",
                    text_fields: &[("model", &harness.model_name)],
                    file_field_name: "image",
                    file_name: "edit-fallback.png",
                    file_content_type: "image/png",
                    file_bytes: b"fallback edit image",
                })
                .await,
        )
        .await;
        assert_eq!(image_edits_payload["data"][0]["route"], "fallback");

        let image_variations_payload = response_json(
            harness
                .multipart_request(MultipartRequestSpec {
                    uri: "/v1/images/variations",
                    request_id: "model-passthrough-image-variations",
                    text_fields: &[("model", &harness.model_name)],
                    file_field_name: "image",
                    file_name: "variation-fallback.png",
                    file_content_type: "image/png",
                    file_bytes: b"fallback variation image",
                })
                .await,
        )
        .await;
        assert_eq!(image_variations_payload["data"][0]["route"], "fallback");

        let audio_transcriptions_payload = response_json(
            harness
                .multipart_request(MultipartRequestSpec {
                    uri: "/v1/audio/transcriptions",
                    request_id: "model-passthrough-audio-transcriptions",
                    text_fields: &[("model", &harness.model_name)],
                    file_field_name: "file",
                    file_name: "voice-fallback.wav",
                    file_content_type: "audio/wav",
                    file_bytes: b"fallback audio bytes",
                })
                .await,
        )
        .await;
        assert_eq!(audio_transcriptions_payload["route"], "fallback");

        let audio_translations_payload = response_json(
            harness
                .multipart_request(MultipartRequestSpec {
                    uri: "/v1/audio/translations",
                    request_id: "model-passthrough-audio-translations",
                    text_fields: &[("model", &harness.model_name)],
                    file_field_name: "file",
                    file_name: "voice-translation-fallback.wav",
                    file_content_type: "audio/wav",
                    file_bytes: b"fallback translation audio bytes",
                })
                .await,
        )
        .await;
        assert_eq!(audio_translations_payload["route"], "fallback");

        let audio_speech_payload = response_text(
            harness
                .json_request(
                    Method::POST,
                    "/v1/audio/speech",
                    "model-passthrough-audio-speech",
                    serde_json::json!({
                        "model": harness.model_name,
                        "input": "say hello from fallback",
                        "voice": "alloy"
                    }),
                )
                .await,
        )
        .await;
        assert_eq!(audio_speech_payload, "fallback-audio");

        let moderations_payload = response_json(
            harness
                .json_request(
                    Method::POST,
                    "/v1/moderations",
                    "model-passthrough-moderations",
                    serde_json::json!({
                        "model": harness.model_name,
                        "input": "moderate fallback text"
                    }),
                )
                .await,
        )
        .await;
        assert_eq!(moderations_payload["results"][0]["route"], "fallback");

        let rerank_payload = response_json(
            harness
                .json_request(
                    Method::POST,
                    "/v1/rerank",
                    "model-passthrough-rerank",
                    serde_json::json!({
                        "model": harness.model_name,
                        "query": "rerank fallback query",
                        "documents": ["a", "b"]
                    }),
                )
                .await,
        )
        .await;
        assert_eq!(rerank_payload["results"][0]["route"], "fallback");

        assert_eq!(primary.hit_count("/v1/completions"), 1);
        assert_eq!(fallback.hit_count("/v1/completions"), 1);
        assert_eq!(fallback.hit_count("/v1/images/generations"), 1);
        assert_eq!(fallback.hit_count("/v1/images/edits"), 1);
        assert_eq!(fallback.hit_count("/v1/images/variations"), 1);
        assert_eq!(fallback.hit_count("/v1/audio/transcriptions"), 1);
        assert_eq!(fallback.hit_count("/v1/audio/translations"), 1);
        assert_eq!(fallback.hit_count("/v1/audio/speech"), 1);
        assert_eq!(fallback.hit_count("/v1/moderations"), 1);
        assert_eq!(fallback.hit_count("/v1/rerank"), 1);

        harness.cleanup().await;
    }

    #[tokio::test]
    #[ignore = "requires local postgres and redis"]
    async fn image_generations_route_settles_fallback_usage_and_persists_log() {
        let primary = MockUpstreamServer::spawn(vec![
            MockRoute::json(
                Method::POST,
                "/v1/images/generations",
                Some("Bearer sk-primary"),
                Some("draw a sunset"),
                StatusCode::OK,
                serde_json::json!({
                    "created": 1,
                    "data": [{
                        "url": "https://primary.example/image.png",
                        "route": "primary"
                    }]
                }),
            )
            .with_response_headers(vec![("x-request-id", "img-upstream-primary-123")]),
        ])
        .await;
        let harness = TestHarness::model_passthrough_affinity_fixture(
            &primary.base_url,
            "http://127.0.0.1:9",
        )
        .await;
        let request_body = serde_json::json!({
            "model": harness.model_name,
            "prompt": "draw a sunset"
        });
        let expected_tokens = i64::from(estimate_json_tokens(&request_body));
        let request_id = format!(
            "model-passthrough-image-generations-accounting-{}",
            harness.model_name
        );

        let response = harness
            .json_request(
                Method::POST,
                "/v1/images/generations",
                &request_id,
                request_body,
            )
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        let upstream_request_id = response
            .headers()
            .get("x-upstream-request-id")
            .and_then(|value| value.to_str().ok())
            .expect("image generations upstream request id")
            .to_string();
        let payload = response_json(response).await;
        assert_eq!(payload["data"][0]["route"], "primary");
        assert_eq!(upstream_request_id, "img-upstream-primary-123");

        let token = harness.wait_for_token_used_quota(expected_tokens).await;
        assert_eq!(token.used_quota, expected_tokens);

        let log = harness.wait_for_log_by_request_id(&request_id).await;
        assert_eq!(log.endpoint, "images/generations");
        assert_eq!(log.request_format, "openai/images_generations");
        assert_eq!(log.requested_model, harness.model_name);
        assert_eq!(log.upstream_model, harness.model_name);
        assert_eq!(log.model_name, harness.model_name);
        assert_eq!(log.upstream_request_id, "img-upstream-primary-123");
        assert_eq!(log.prompt_tokens, expected_tokens as i32);
        assert_eq!(log.completion_tokens, 0);
        assert_eq!(log.total_tokens, expected_tokens as i32);
        assert_eq!(log.quota, expected_tokens);
        assert_eq!(log.status_code, 200);
        assert_eq!(log.status, LogStatus::Success);
        assert!(!log.is_stream);

        harness.cleanup().await;
    }

    #[tokio::test]
    #[ignore = "requires local postgres and redis"]
    async fn image_generations_route_falls_back_after_primary_rate_limit() {
        let primary = MockUpstreamServer::spawn(vec![MockRoute::json(
            Method::POST,
            "/v1/images/generations",
            Some("Bearer sk-primary"),
            Some("draw a sunset"),
            StatusCode::TOO_MANY_REQUESTS,
            serde_json::json!({
                "error": {
                    "message": "primary image rate limited",
                    "type": "rate_limit_error"
                }
            }),
        )])
        .await;
        let fallback = MockUpstreamServer::spawn(vec![MockRoute::json(
            Method::POST,
            "/v1/images/generations",
            Some("Bearer sk-fallback"),
            Some("draw a sunset"),
            StatusCode::OK,
            serde_json::json!({
                "created": 1,
                "data": [{
                    "url": "https://fallback.example/image.png",
                    "route": "fallback"
                }]
            }),
        )])
        .await;
        let harness =
            TestHarness::model_passthrough_affinity_fixture(&primary.base_url, &fallback.base_url)
                .await;
        let request_body = serde_json::json!({
            "model": harness.model_name,
            "prompt": "draw a sunset"
        });
        let expected_tokens = i64::from(estimate_json_tokens(&request_body));
        let request_id = format!("image-generations-fallback-{}", harness.model_name);

        let response = harness
            .json_request(
                Method::POST,
                "/v1/images/generations",
                &request_id,
                request_body,
            )
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        let payload = response_json(response).await;
        assert_eq!(payload["data"][0]["route"], "fallback");

        let token = harness.wait_for_token_used_quota(expected_tokens).await;
        assert_eq!(token.used_quota, expected_tokens);

        let log = harness.wait_for_log_by_request_id(&request_id).await;
        assert_eq!(log.endpoint, "images/generations");
        assert_eq!(log.request_format, "openai/images_generations");
        assert_eq!(log.requested_model, harness.model_name);
        assert_eq!(log.upstream_model, harness.model_name);
        assert_eq!(log.total_tokens, expected_tokens as i32);
        assert_eq!(log.quota, expected_tokens);
        assert_eq!(log.status, LogStatus::Success);

        let primary_account = harness.wait_for_primary_account_rate_limited().await;
        assert_eq!(primary_account.failure_streak, 1);
        assert!(primary_account.rate_limited_until.is_some());

        let primary_channel = harness.primary_channel_model().await;
        assert_eq!(primary_channel.failure_streak, 1);
        assert_eq!(primary_channel.last_health_status, 3);

        assert_eq!(primary.hit_count("/v1/images/generations"), 1);
        assert_eq!(fallback.hit_count("/v1/images/generations"), 1);

        harness.cleanup().await;
    }

    #[tokio::test]
    #[ignore = "requires local postgres and redis"]
    async fn image_generations_route_skips_rate_limited_primary_on_next_request() {
        let primary = MockUpstreamServer::spawn(vec![MockRoute::json(
            Method::POST,
            "/v1/images/generations",
            Some("Bearer sk-primary"),
            Some("draw a sunset"),
            StatusCode::TOO_MANY_REQUESTS,
            serde_json::json!({
                "error": {
                    "message": "primary image rate limited",
                    "type": "rate_limit_error"
                }
            }),
        )])
        .await;
        let fallback = MockUpstreamServer::spawn(vec![MockRoute::json(
            Method::POST,
            "/v1/images/generations",
            Some("Bearer sk-fallback"),
            Some("draw a sunset"),
            StatusCode::OK,
            serde_json::json!({
                "created": 1,
                "data": [{
                    "url": "https://fallback.example/image.png",
                    "route": "fallback"
                }]
            }),
        )])
        .await;
        let harness =
            TestHarness::model_passthrough_affinity_fixture(&primary.base_url, &fallback.base_url)
                .await;
        let request_body = serde_json::json!({
            "model": harness.model_name,
            "prompt": "draw a sunset"
        });
        let expected_tokens = i64::from(estimate_json_tokens(&request_body));
        let first_request_id = format!("image-generations-rate-limit-first-{}", harness.model_name);
        let second_request_id =
            format!("image-generations-rate-limit-second-{}", harness.model_name);

        let first_response = harness
            .json_request(
                Method::POST,
                "/v1/images/generations",
                &first_request_id,
                request_body.clone(),
            )
            .await;
        assert_eq!(first_response.status(), StatusCode::OK);
        let first_payload = response_json(first_response).await;
        assert_eq!(first_payload["data"][0]["route"], "fallback");

        let primary_account = harness.wait_for_primary_account_rate_limited().await;
        assert_eq!(primary_account.failure_streak, 1);
        assert!(primary_account.rate_limited_until.is_some());

        let second_response = harness
            .json_request(
                Method::POST,
                "/v1/images/generations",
                &second_request_id,
                request_body,
            )
            .await;
        assert_eq!(second_response.status(), StatusCode::OK);
        let second_payload = response_json(second_response).await;
        assert_eq!(second_payload["data"][0]["route"], "fallback");

        let token = harness.wait_for_token_used_quota(expected_tokens * 2).await;
        assert_eq!(token.used_quota, expected_tokens * 2);

        assert_eq!(primary.hit_count("/v1/images/generations"), 1);
        assert_eq!(fallback.hit_count("/v1/images/generations"), 2);

        harness.cleanup().await;
    }

    #[tokio::test]
    #[ignore = "requires local postgres and redis"]
    async fn image_generations_route_quarantines_primary_account_after_auth_failure() {
        let primary = MockUpstreamServer::spawn(vec![MockRoute::json(
            Method::POST,
            "/v1/images/generations",
            Some("Bearer sk-primary"),
            Some("draw a sunset"),
            StatusCode::UNAUTHORIZED,
            serde_json::json!({
                "error": {
                    "message": "invalid api key",
                    "type": "authentication_error"
                }
            }),
        )])
        .await;
        let fallback = MockUpstreamServer::spawn(vec![MockRoute::json(
            Method::POST,
            "/v1/images/generations",
            Some("Bearer sk-fallback"),
            Some("draw a sunset"),
            StatusCode::OK,
            serde_json::json!({
                "created": 1,
                "data": [{
                    "url": "https://fallback.example/image-auth.png",
                    "route": "fallback"
                }]
            }),
        )])
        .await;
        let harness =
            TestHarness::model_passthrough_affinity_fixture(&primary.base_url, &fallback.base_url)
                .await;
        let request_body = serde_json::json!({
            "model": harness.model_name,
            "prompt": "draw a sunset"
        });
        let expected_tokens = i64::from(estimate_json_tokens(&request_body));
        let first_request_id = format!("image-generations-auth-first-{}", harness.model_name);
        let second_request_id = format!("image-generations-auth-second-{}", harness.model_name);

        let first_response = harness
            .json_request(
                Method::POST,
                "/v1/images/generations",
                &first_request_id,
                request_body.clone(),
            )
            .await;
        assert_eq!(first_response.status(), StatusCode::OK);
        let first_payload = response_json(first_response).await;
        assert_eq!(first_payload["data"][0]["route"], "fallback");

        let primary_account = harness.wait_for_primary_account_disabled().await;
        assert_eq!(primary_account.status, AccountStatus::Disabled);
        assert!(!primary_account.schedulable);
        assert_eq!(primary_account.failure_streak, 1);

        let primary_channel = harness.primary_channel_model().await;
        assert_eq!(primary_channel.failure_streak, 0);
        assert_eq!(primary_channel.last_health_status, 2);

        let second_response = harness
            .json_request(
                Method::POST,
                "/v1/images/generations",
                &second_request_id,
                request_body,
            )
            .await;
        assert_eq!(second_response.status(), StatusCode::OK);
        let second_payload = response_json(second_response).await;
        assert_eq!(second_payload["data"][0]["route"], "fallback");

        let token = harness.wait_for_token_used_quota(expected_tokens * 2).await;
        assert_eq!(token.used_quota, expected_tokens * 2);

        assert_eq!(primary.hit_count("/v1/images/generations"), 1);
        assert_eq!(fallback.hit_count("/v1/images/generations"), 2);

        harness.cleanup().await;
    }

    #[tokio::test]
    #[ignore = "requires local postgres and redis"]
    async fn image_generations_route_falls_back_after_primary_overload() {
        let primary = MockUpstreamServer::spawn(vec![MockRoute::json(
            Method::POST,
            "/v1/images/generations",
            Some("Bearer sk-primary"),
            Some("draw a sunset"),
            StatusCode::SERVICE_UNAVAILABLE,
            serde_json::json!({
                "error": {
                    "message": "primary image upstream overloaded",
                    "type": "server_error"
                }
            }),
        )])
        .await;
        let fallback = MockUpstreamServer::spawn(vec![MockRoute::json(
            Method::POST,
            "/v1/images/generations",
            Some("Bearer sk-fallback"),
            Some("draw a sunset"),
            StatusCode::OK,
            serde_json::json!({
                "created": 1,
                "data": [{
                    "url": "https://fallback.example/image-overload.png",
                    "route": "fallback"
                }]
            }),
        )])
        .await;
        let harness =
            TestHarness::model_passthrough_affinity_fixture(&primary.base_url, &fallback.base_url)
                .await;
        let request_body = serde_json::json!({
            "model": harness.model_name,
            "prompt": "draw a sunset"
        });
        let expected_tokens = i64::from(estimate_json_tokens(&request_body));
        let request_id = format!("image-generations-overload-{}", harness.model_name);

        let response = harness
            .json_request(
                Method::POST,
                "/v1/images/generations",
                &request_id,
                request_body,
            )
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        let payload = response_json(response).await;
        assert_eq!(payload["data"][0]["route"], "fallback");

        let token = harness.wait_for_token_used_quota(expected_tokens).await;
        assert_eq!(token.used_quota, expected_tokens);

        let log = harness.wait_for_log_by_request_id(&request_id).await;
        assert_eq!(log.endpoint, "images/generations");
        assert_eq!(log.request_format, "openai/images_generations");
        assert_eq!(log.requested_model, harness.model_name);
        assert_eq!(log.upstream_model, harness.model_name);
        assert_eq!(log.total_tokens, expected_tokens as i32);
        assert_eq!(log.quota, expected_tokens);
        assert_eq!(log.status, LogStatus::Success);

        let primary_account = harness.wait_for_primary_account_overloaded().await;
        assert_eq!(primary_account.failure_streak, 1);
        assert!(primary_account.overload_until.is_some());
        assert!(primary_account.rate_limited_until.is_none());

        let primary_channel = harness.primary_channel_model().await;
        assert_eq!(primary_channel.failure_streak, 1);
        assert_eq!(primary_channel.last_health_status, 3);

        assert_eq!(primary.hit_count("/v1/images/generations"), 1);
        assert_eq!(fallback.hit_count("/v1/images/generations"), 1);

        harness.cleanup().await;
    }

    #[tokio::test]
    #[ignore = "requires local postgres and redis"]
    async fn uploads_create_without_model_does_not_consume_quota_or_persist_usage_log() {
        let primary = MockUpstreamServer::spawn(vec![MockRoute::json(
            Method::POST,
            "/v1/uploads",
            Some("Bearer sk-primary"),
            Some("upload-boundary-primary.bin"),
            StatusCode::OK,
            serde_json::json!({
                "id": "upload_boundary_primary",
                "object": "upload",
                "status": "in_progress",
                "route": "primary"
            }),
        )])
        .await;
        let harness =
            TestHarness::uploads_affinity_fixture(&primary.base_url, "http://127.0.0.1:9").await;
        let request_id = format!("uploads-boundary-create-{}", harness.model_name);

        let response = harness
            .json_request(
                Method::POST,
                "/v1/uploads",
                &request_id,
                serde_json::json!({
                    "filename": "upload-boundary-primary.bin",
                    "purpose": "assistants",
                    "bytes": 18,
                    "mime_type": "application/octet-stream"
                }),
            )
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        let payload = response_json(response).await;
        assert_eq!(payload["id"], "upload_boundary_primary");
        assert_eq!(payload["route"], "primary");

        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        let token = harness.token_model().await;
        assert_eq!(token.used_quota, 0);
        harness.assert_no_log_by_request_id(&request_id).await;

        harness.cleanup().await;
    }
}
