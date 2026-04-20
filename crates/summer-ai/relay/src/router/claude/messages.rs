//! `POST /v1/messages` —— Claude Messages API 兼容入口。
//!
//! Handler 只做参数解包 + 调 [`PipelineCall::execute`] + 包装响应。
//! 鉴权 / 选路 / 翻译（Claude ↔ canonical）/ 发上游 / tracking 都在 engine 内完成。
//!
//! 错误用 [`ClaudeError`](crate::error::ClaudeError) newtype 包一层——`?` 自动转，
//! `IntoResponse` 时输出 Anthropic 官方格式 `{"type":"error","error":{...}}`。

use summer_ai_core::types::ingress_wire::claude::ClaudeMessagesRequest;
use summer_web::axum::Json;
use summer_web::axum::body::Body;
use summer_web::axum::response::{IntoResponse, Response};
use summer_web::extractor::Component;
use summer_web::post;

use crate::auth::AiToken;
use crate::convert::ingress::{ClaudeIngress, IngressFormat};
use crate::error::ClaudeResult;
use crate::extract::RelayRequestMeta;
use crate::pipeline::{EngineOutcome, PipelineCall};
use crate::service::channel_store::ChannelStore;
use crate::service::stream_driver::sse_response;
use crate::service::tracking::TrackingService;

/// `POST /v1/messages`
#[post("/v1/messages")]
pub async fn messages(
    AiToken(token): AiToken,
    Component(http): Component<reqwest::Client>,
    Component(store): Component<ChannelStore>,
    Component(tracking): Component<TrackingService>,
    meta: RelayRequestMeta,
    Json(claude_req): Json<ClaudeMessagesRequest>,
) -> ClaudeResult<Response> {
    let logical_model = claude_req.model.clone();
    let is_stream = claude_req.stream;
    let client_req_snapshot = serde_json::to_value(&claude_req).ok();

    let call = PipelineCall::<ClaudeIngress> {
        endpoint: meta.endpoint,
        format: IngressFormat::Claude,
        token,
        is_stream,
        logical_model,
        client_ip: meta.client_ip,
        user_agent: meta.user_agent,
        client_headers: meta.client_headers,
        client_req: claude_req,
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
