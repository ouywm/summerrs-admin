//! `POST /v1/responses` —— OpenAI Responses API 入口。
//!
//! Handler 只做参数解包 + 调 [`PipelineCall::execute`] + 包装响应。
//! 鉴权 / 选路 / 翻译（Responses ↔ canonical）/ 发上游 / tracking 都在 engine 内。

use summer_ai_core::types::ingress_wire::openai_responses::OpenAIResponsesRequest;
use summer_web::axum::Json;
use summer_web::axum::body::Body;
use summer_web::axum::response::{IntoResponse, Response};
use summer_web::extractor::Component;
use summer_web::post;

use crate::auth::AiToken;
use crate::convert::ingress::{IngressFormat, OpenAIResponsesIngress};
use crate::error::OpenAIResult;
use crate::extract::RelayRequestMeta;
use crate::pipeline::{EngineOutcome, PipelineCall};
use crate::service::channel_store::ChannelStore;
use crate::service::stream_driver::sse_response;
use crate::service::tracking::TrackingService;

/// `POST /v1/responses`
#[post("/v1/responses")]
pub async fn responses(
    AiToken(token): AiToken,
    Component(http): Component<reqwest::Client>,
    Component(store): Component<ChannelStore>,
    Component(tracking): Component<TrackingService>,
    meta: RelayRequestMeta,
    Json(req): Json<OpenAIResponsesRequest>,
) -> OpenAIResult<Response> {
    let logical_model = req.model.clone();
    let is_stream = req.stream;
    let client_req_snapshot = serde_json::to_value(&req).ok();

    let call = PipelineCall::<OpenAIResponsesIngress> {
        endpoint: meta.endpoint,
        format: IngressFormat::OpenAIResponses,
        token,
        is_stream,
        logical_model,
        client_ip: meta.client_ip,
        user_agent: meta.user_agent,
        client_headers: meta.client_headers,
        client_req: req,
        client_req_snapshot,
        http,
        store,
        tracking,
    };

    match call.execute().await? {
        EngineOutcome::NonStream(resp) => Ok(Json(resp).into_response()),
        EngineOutcome::Stream(body_stream) => Ok(sse_response(Body::from_stream(body_stream))),
    }
}
