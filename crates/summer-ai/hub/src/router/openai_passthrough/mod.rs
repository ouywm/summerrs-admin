#![allow(clippy::too_many_arguments)]

use std::convert::Infallible;

use bytes::Bytes;
use futures::StreamExt;
use reqwest::multipart::{Form, Part};
use serde_json::Value;
use summer_ai_core::types::common::Usage;
use summer_ai_core::types::error::{OpenAiApiResult, OpenAiErrorResponse};
use summer_ai_model::entity::channel::ChannelType;
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
use crate::router::openai::{apply_upstream_failure_scope, classify_upstream_provider_failure};
use crate::service::channel::ChannelService;
use crate::service::log::{AiFailureLogRecord, AiUsageLogRecord, LogService};
use crate::service::openai_http::{
    extract_request_id, extract_upstream_request_id, fallback_usage, insert_request_id_header,
    insert_upstream_request_id_header,
};
use crate::service::openai_relay_support::read_multipart_field_bytes_limited;
use crate::service::resource_affinity::ResourceAffinityService;
use crate::service::response_bridge::ResponseBridgeService;
use crate::service::token::{TokenInfo, TokenService};

mod assistants_threads;
mod batches;
mod fine_tuning;
mod relay_json;
mod relay_stream;
pub(crate) mod resource;
mod responses;
pub(crate) mod support;
#[cfg(test)]
mod tests;
mod uploads_models;
mod vector_stores;

#[allow(unused_imports)]
pub use assistants_threads::*;
#[allow(unused_imports)]
pub use batches::*;
#[allow(unused_imports)]
pub use fine_tuning::*;
#[allow(unused_imports)]
pub(crate) use relay_json::{
    relay_json_model_request, relay_resource_bodyless_post, relay_resource_delete,
    relay_resource_get, relay_resource_json_post, relay_usage_resource_json_post,
};
#[allow(unused_imports)]
pub(crate) use relay_stream::{
    bind_resource_affinities, build_generic_stream_response, ensure_json_model,
    estimate_json_tokens, estimate_total_tokens_for_rate_limit, extract_model_from_response_value,
    extract_usage_from_value, json_body_requests_stream, mapped_model, payload_has_text_delta,
    relay_resource_multipart_post, relay_resource_request, spawn_resource_usage_accounting_task,
};
#[allow(unused_imports)]
pub use responses::*;
#[allow(unused_imports)]
pub use uploads_models::*;
#[allow(unused_imports)]
pub use vector_stores::*;

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

#[allow(dead_code)]
#[derive(Clone, Copy)]
pub(crate) struct JsonModelRelaySpec {
    pub(crate) endpoint_scope: &'static str,
    pub(crate) upstream_path: &'static str,
    pub(crate) endpoint: &'static str,
    pub(crate) request_format: &'static str,
    pub(crate) default_model: Option<&'static str>,
}

#[derive(Debug, Clone)]
pub(crate) enum MultipartField {
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
pub(crate) struct ParsedMultipartPayload {
    pub(crate) fields: Vec<MultipartField>,
    pub(crate) model: Option<String>,
}

#[derive(Default)]
pub(crate) struct GenericStreamTracker {
    pub(crate) buffer: String,
    pub(crate) usage: Option<Usage>,
    pub(crate) upstream_model: String,
    pub(crate) resource_id: String,
    pub(crate) resource_refs: Vec<(&'static str, String)>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn record_passthrough_failure(
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
    pub(crate) fn ingest(
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

            if first_token_time.is_none() && relay_stream::payload_has_text_delta(&payload) {
                *first_token_time = Some(start.elapsed().as_millis() as i64);
            }

            if self.upstream_model.is_empty()
                && let Some(model) = relay_stream::extract_model_from_response_value(&payload)
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

            if let Some(usage) = relay_stream::extract_usage_from_value(&payload) {
                self.usage = Some(usage);
            }
        }
    }
}

pub(crate) fn map_response_bridge_error(
    action: &'static str,
    error: impl std::error::Error + Send + Sync + 'static,
) -> OpenAiErrorResponse {
    OpenAiErrorResponse::internal_with(action, error)
}
