//! `POST /v1/messages` —— Claude Messages API 兼容入口。
//!
//! # 当前（走路骨架）
//!
//! - 客户端用 Claude SDK 格式发请求
//! - 通过 [`ClaudeIngress::to_canonical`] 翻译成 canonical
//! - 复用 [`crate::service::chat`] 发给 OpenAI 上游
//! - 非流式响应：[`ClaudeIngress::from_canonical`] 翻译回 Claude JSON
//! - 流式响应：[`crate::service::stream_driver::transcode_stream`] 把上游 SSE
//!   重组成 Claude 6-event SSE
//!
//! **硬编码**：
//! - 上游 `AdapterKind::OpenAI`
//! - `base_url`：env `OPENAI_BASE_URL`（默认 `https://api.openai.com/v1`）
//! - `api_key`：env `OPENAI_API_KEY`（必须）
//! - `actual_model = request.model`（不做模型映射）

use summer_admin_macros::no_auth;
use summer_ai_core::types::ingress_wire::claude::ClaudeMessagesRequest;
use summer_ai_core::{AdapterKind, ServiceTarget};
use summer_web::Router;
use summer_web::axum::Json;
use summer_web::axum::body::Body;
use summer_web::axum::http::{HeaderValue, StatusCode, header};
use summer_web::axum::response::{IntoResponse, Response};
use summer_web::extractor::Component;
use summer_web::handler::TypeRouter;
use summer_web::post;

use crate::convert::ingress::{ClaudeIngress, IngressConverter, IngressCtx};
use crate::error::{RelayError, RelayResult};
use crate::service::{chat, stream_driver};

const DEFAULT_OPENAI_BASE_URL: &str = "https://api.openai.com/v1";

/// `POST /v1/messages`
#[no_auth]
#[post("/v1/messages")]
pub async fn messages(
    Component(http): Component<reqwest::Client>,
    Json(claude_req): Json<ClaudeMessagesRequest>,
) -> RelayResult<Response> {
    let api_key =
        std::env::var("OPENAI_API_KEY").map_err(|_| RelayError::MissingConfig("OPENAI_API_KEY"))?;
    let base_url =
        std::env::var("OPENAI_BASE_URL").unwrap_or_else(|_| DEFAULT_OPENAI_BASE_URL.to_string());

    let kind = AdapterKind::OpenAI;
    let logical_model = claude_req.model.clone();
    let is_stream = claude_req.stream;

    tracing::debug!(
        model = %logical_model,
        messages = claude_req.messages.len(),
        stream = is_stream,
        "claude /v1/messages"
    );

    let ctx = IngressCtx::new(kind, &logical_model, &logical_model);

    // client wire → canonical
    let mut canonical_req = ClaudeIngress::to_canonical(claude_req, &ctx)?;
    // stream 位必须显式标记——Adapter::build_chat_request 据此决定 URL/payload
    canonical_req.stream = is_stream;

    let target = ServiceTarget::bearer(base_url, api_key, &logical_model);

    if is_stream {
        let upstream = chat::invoke_stream_raw(&http, kind, &target, &canonical_req).await?;
        let body_stream =
            stream_driver::transcode_stream::<ClaudeIngress>(upstream, kind, target, ctx);
        let body = Body::from_stream(body_stream);
        Ok((
            StatusCode::OK,
            [
                (
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("text/event-stream"),
                ),
                (header::CACHE_CONTROL, HeaderValue::from_static("no-cache")),
                (header::CONNECTION, HeaderValue::from_static("keep-alive")),
            ],
            body,
        )
            .into_response())
    } else {
        let canonical_resp = chat::invoke_non_stream(&http, kind, &target, &canonical_req).await?;
        let claude_resp = ClaudeIngress::from_canonical(canonical_resp, &ctx)?;
        Ok(Json(claude_resp).into_response())
    }
}

pub fn routes(router: Router) -> Router {
    router.typed_route(messages)
}
