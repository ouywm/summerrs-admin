use summer::plugin::Service;
use summer_web::axum::http::HeaderMap;
use summer_web::axum::response::Response;
use summer_web::extractor::Component;

use summer_ai_core::types::chat::ChatCompletionRequest;
use summer_ai_core::types::error::OpenAiApiResult;
use summer_common::extractor::ClientIp;
use summer_common::response::Json;

use crate::auth::extractor::AiToken;
use crate::relay::billing::BillingEngine;
use crate::relay::channel_router::ChannelRouter;
use crate::relay::http_client::UpstreamHttpClient;
use crate::relay::rate_limit::RateLimitEngine;
use crate::service::channel::ChannelService;
use crate::service::log::LogService;
use crate::service::request::RequestService;
use crate::service::runtime_ops::RuntimeOpsService;
use crate::service::token::TokenService;

#[derive(Clone, Service)]
pub struct OpenAiChatRelayService {
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
    #[inject(component)]
    request_svc: RequestService,
}

impl OpenAiChatRelayService {
    pub async fn relay(
        &self,
        token_info: crate::service::token::TokenInfo,
        client_ip: std::net::IpAddr,
        headers: HeaderMap,
        req: ChatCompletionRequest,
    ) -> OpenAiApiResult<Response> {
        crate::router::openai::relay_chat_completions_impl(
            AiToken(token_info),
            Component(self.router_svc.clone()),
            Component(self.billing.clone()),
            Component(self.rate_limiter.clone()),
            Component(self.http_client.clone()),
            Component(self.log_svc.clone()),
            Component(self.channel_svc.clone()),
            Component(self.token_svc.clone()),
            Component(self.runtime_ops.clone()),
            Component(self.request_svc.clone()),
            ClientIp(client_ip),
            headers,
            Json(req),
        )
        .await
    }
}
