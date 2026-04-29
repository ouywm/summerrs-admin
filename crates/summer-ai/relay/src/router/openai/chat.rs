//! `POST /v1/chat/completions` —— OpenAI 兼容聊天接口。
//!
//! Handler 只做参数解包 + 调 [`PipelineCall::execute`] + 包装响应。
//! 鉴权 / 选路 / 翻译 / 发上游 / tracking 落库都在 engine 内完成。
//!
//! OpenAI 是 identity ingress（canonical 就是 OpenAI-flat 格式），走 engine 是为了
//! 统一四个入口——包括流式通过 [`stream_driver::transcode_stream`] 拿 usage 用于
//! tracking / billing，而不是原来的 `bytes_stream()` 裸透传。
//!
//! 鉴权由 `ApiKeyStrategy` 挂在 `"summer-ai-relay"` group layer 上完成；admin JWT
//! AuthLayer 挂在 `"summer-system"` group 上，不会拦到本 handler。

use crate::auth::AiToken;
use crate::convert::ingress::{IngressFormat, OpenAIIngress};
use crate::error::OpenAIResult;
use crate::extract::RelayRequestMeta;
use crate::pipeline::{EngineOutcome, PipelineCall};
use crate::service::channel_store::ChannelStore;
use crate::service::cooldown::CooldownService;
use crate::service::stream_driver::sse_response;
use crate::service::tracking::TrackingService;
use summer_ai_billing::{BillingService, PriceResolver};
use summer_ai_core::ChatRequest;
use summer_web::axum::Json;
use summer_web::axum::body::Body;
use summer_web::axum::response::{IntoResponse, Response};
use summer_web::extractor::Component;
use summer_web::post;

/// `POST /v1/chat/completions`
#[post("/v1/chat/completions")]
#[allow(clippy::too_many_arguments)]
pub async fn chat_completions(
    AiToken(token): AiToken,
    Component(http): Component<reqwest::Client>,
    Component(store): Component<ChannelStore>,
    Component(tracking): Component<TrackingService>,
    Component(cooldown): Component<CooldownService>,
    Component(billing): Component<BillingService>,
    Component(price_resolver): Component<PriceResolver>,
    meta: RelayRequestMeta,
    Json(request): Json<ChatRequest>,
) -> OpenAIResult<Response> {
    let logical_model = request.model.clone();
    let is_stream = request.stream;
    let client_req_snapshot = serde_json::to_value(&request).ok();

    let call = PipelineCall::<OpenAIIngress> {
        request_id: meta.request_id,
        endpoint: meta.endpoint,
        format: IngressFormat::OpenAI,
        token,
        is_stream,
        logical_model,
        client_ip: meta.client_ip,
        user_agent: meta.user_agent,
        client_headers: meta.client_headers,
        client_req: request,
        client_req_snapshot,
        http,
        store,
        tracking,
        cooldown,
        billing,
        price_resolver,
    };

    match call.execute().await? {
        EngineOutcome::NonStream(resp) => Ok(Json(resp).into_response()),
        EngineOutcome::Stream(body_stream) => Ok(sse_response(Body::from_stream(body_stream))),
    }
}
