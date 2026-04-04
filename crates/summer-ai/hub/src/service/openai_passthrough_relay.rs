use summer::plugin::Service;
use summer_common::response::Json;
use summer_web::axum::extract::Request;
use summer_web::axum::http::HeaderMap;
use summer_web::axum::response::{IntoResponse, Response};

use crate::relay::billing::BillingEngine;
use crate::relay::channel_router::ChannelRouter;
use crate::relay::http_client::UpstreamHttpClient;
use crate::relay::rate_limit::RateLimitEngine;
use crate::router::openai_passthrough::{
    ResourceRequestSpec, map_response_bridge_error, relay_resource_bodyless_post,
    relay_resource_delete, relay_resource_get, relay_resource_json_post,
    relay_resource_multipart_post, relay_usage_resource_json_post,
};
use crate::service::channel::ChannelService;
use crate::service::log::LogService;
use crate::service::resource_affinity::ResourceAffinityService;
use crate::service::response_bridge::ResponseBridgeService;
use crate::service::token::{TokenInfo, TokenService};

use summer_ai_core::types::error::{OpenAiApiResult, OpenAiErrorResponse};

#[derive(Clone, Service)]
pub struct OpenAiPassthroughRelayService {
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
    resource_affinity: ResourceAffinityService,
    #[inject(component)]
    response_bridge: ResponseBridgeService,
}

impl OpenAiPassthroughRelayService {
    pub(crate) async fn relay_resource_get(
        &self,
        token_info: TokenInfo,
        client_ip: std::net::IpAddr,
        req: Request,
        upstream_path: String,
        spec: ResourceRequestSpec,
        affinity_keys: Vec<(&'static str, String)>,
    ) -> OpenAiApiResult<Response> {
        relay_resource_get(
            token_info,
            self.router_svc.clone(),
            self.http_client.clone(),
            self.channel_svc.clone(),
            self.token_svc.clone(),
            self.resource_affinity.clone(),
            client_ip.to_string(),
            req,
            upstream_path,
            spec,
            affinity_keys,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn relay_resource_delete(
        &self,
        token_info: TokenInfo,
        client_ip: std::net::IpAddr,
        req: Request,
        upstream_path: String,
        spec: ResourceRequestSpec,
        affinity_keys: Vec<(&'static str, String)>,
        delete_affinity: Option<(&'static str, String)>,
    ) -> OpenAiApiResult<Response> {
        relay_resource_delete(
            token_info,
            self.router_svc.clone(),
            self.http_client.clone(),
            self.channel_svc.clone(),
            self.token_svc.clone(),
            self.resource_affinity.clone(),
            client_ip.to_string(),
            req,
            upstream_path,
            spec,
            affinity_keys,
            delete_affinity,
        )
        .await
    }

    pub(crate) async fn relay_resource_bodyless_post(
        &self,
        token_info: TokenInfo,
        client_ip: std::net::IpAddr,
        req: Request,
        upstream_path: String,
        spec: ResourceRequestSpec,
        affinity_keys: Vec<(&'static str, String)>,
    ) -> OpenAiApiResult<Response> {
        relay_resource_bodyless_post(
            token_info,
            self.router_svc.clone(),
            self.http_client.clone(),
            self.channel_svc.clone(),
            self.token_svc.clone(),
            self.resource_affinity.clone(),
            client_ip.to_string(),
            req,
            upstream_path,
            spec,
            affinity_keys,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn relay_resource_json_post(
        &self,
        token_info: TokenInfo,
        client_ip: std::net::IpAddr,
        headers: HeaderMap,
        body: serde_json::Value,
        upstream_path: String,
        spec: ResourceRequestSpec,
        affinity_keys: Vec<(&'static str, String)>,
        delete_affinity: Option<(&'static str, String)>,
    ) -> OpenAiApiResult<Response> {
        relay_resource_json_post(
            token_info,
            self.router_svc.clone(),
            self.http_client.clone(),
            self.channel_svc.clone(),
            self.token_svc.clone(),
            self.resource_affinity.clone(),
            client_ip.to_string(),
            headers,
            body,
            upstream_path,
            spec,
            affinity_keys,
            delete_affinity,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn relay_resource_multipart_post(
        &self,
        token_info: TokenInfo,
        client_ip: std::net::IpAddr,
        headers: HeaderMap,
        multipart: summer_web::axum::extract::Multipart,
        upstream_path: String,
        spec: ResourceRequestSpec,
        affinity_keys: Vec<(&'static str, String)>,
        delete_affinity: Option<(&'static str, String)>,
    ) -> OpenAiApiResult<Response> {
        relay_resource_multipart_post(
            token_info,
            self.router_svc.clone(),
            self.http_client.clone(),
            self.channel_svc.clone(),
            self.token_svc.clone(),
            self.resource_affinity.clone(),
            client_ip.to_string(),
            headers,
            multipart,
            upstream_path,
            spec,
            affinity_keys,
            delete_affinity,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn relay_usage_resource_json_post(
        &self,
        token_info: TokenInfo,
        client_ip: std::net::IpAddr,
        headers: HeaderMap,
        body: serde_json::Value,
        upstream_path: String,
        spec: ResourceRequestSpec,
        endpoint: &'static str,
        request_format: &'static str,
        affinity_keys: Vec<(&'static str, String)>,
        delete_affinity: Option<(&'static str, String)>,
    ) -> OpenAiApiResult<Response> {
        relay_usage_resource_json_post(
            token_info,
            self.router_svc.clone(),
            self.billing.clone(),
            self.rate_limiter.clone(),
            self.http_client.clone(),
            self.log_svc.clone(),
            self.channel_svc.clone(),
            self.token_svc.clone(),
            self.resource_affinity.clone(),
            client_ip.to_string(),
            headers,
            body,
            upstream_path,
            spec,
            endpoint,
            request_format,
            affinity_keys,
            delete_affinity,
        )
        .await
    }

    pub(crate) async fn get_response(
        &self,
        token_info: TokenInfo,
        client_ip: std::net::IpAddr,
        response_id: String,
        req: Request,
    ) -> OpenAiApiResult<Response> {
        let client_ip = client_ip.to_string();
        if let Some(snapshot) = self
            .response_bridge
            .get_response(&token_info, &response_id)
            .await
            .map_err(|error| map_response_bridge_error("failed to load bridged response", error))?
        {
            token_info
                .ensure_endpoint_allowed("responses")
                .map_err(|error| OpenAiErrorResponse::from_api_error(&error))?;
            self.token_svc
                .update_last_used_ip_async(token_info.token_id, client_ip.clone());
            let request_id = crate::service::openai_http::extract_request_id(req.headers());
            let mut response = Json(snapshot.payload).into_response();
            crate::service::openai_http::insert_request_id_header(&mut response, &request_id);
            crate::service::openai_http::insert_upstream_request_id_header(
                &mut response,
                &snapshot.upstream_request_id,
            );
            return Ok(response);
        }

        relay_resource_get(
            token_info,
            self.router_svc.clone(),
            self.http_client.clone(),
            self.channel_svc.clone(),
            self.token_svc.clone(),
            self.resource_affinity.clone(),
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

    pub(crate) async fn get_response_input_items(
        &self,
        token_info: TokenInfo,
        client_ip: std::net::IpAddr,
        response_id: String,
        req: Request,
    ) -> OpenAiApiResult<Response> {
        let client_ip = client_ip.to_string();
        if let Some(snapshot) = self
            .response_bridge
            .get_input_items(&token_info, &response_id)
            .await
            .map_err(|error| {
                map_response_bridge_error("failed to load bridged response input items", error)
            })?
        {
            token_info
                .ensure_endpoint_allowed("responses")
                .map_err(|error| OpenAiErrorResponse::from_api_error(&error))?;
            self.token_svc
                .update_last_used_ip_async(token_info.token_id, client_ip.clone());
            let request_id = crate::service::openai_http::extract_request_id(req.headers());
            let mut response = Json(snapshot.payload).into_response();
            crate::service::openai_http::insert_request_id_header(&mut response, &request_id);
            crate::service::openai_http::insert_upstream_request_id_header(
                &mut response,
                &snapshot.upstream_request_id,
            );
            return Ok(response);
        }

        relay_resource_get(
            token_info,
            self.router_svc.clone(),
            self.http_client.clone(),
            self.channel_svc.clone(),
            self.token_svc.clone(),
            self.resource_affinity.clone(),
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

    pub(crate) async fn cancel_response(
        &self,
        token_info: TokenInfo,
        client_ip: std::net::IpAddr,
        response_id: String,
        req: Request,
    ) -> OpenAiApiResult<Response> {
        let client_ip = client_ip.to_string();
        if let Some(snapshot) = self
            .response_bridge
            .cancel(&token_info, &response_id)
            .await
            .map_err(|error| {
                map_response_bridge_error("failed to cancel bridged response", error)
            })?
        {
            token_info
                .ensure_endpoint_allowed("responses")
                .map_err(|error| OpenAiErrorResponse::from_api_error(&error))?;
            self.token_svc
                .update_last_used_ip_async(token_info.token_id, client_ip.clone());
            let request_id = crate::service::openai_http::extract_request_id(req.headers());
            let mut response = Json(snapshot.payload).into_response();
            crate::service::openai_http::insert_request_id_header(&mut response, &request_id);
            crate::service::openai_http::insert_upstream_request_id_header(
                &mut response,
                &snapshot.upstream_request_id,
            );
            return Ok(response);
        }

        relay_resource_bodyless_post(
            token_info,
            self.router_svc.clone(),
            self.http_client.clone(),
            self.channel_svc.clone(),
            self.token_svc.clone(),
            self.resource_affinity.clone(),
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
}
