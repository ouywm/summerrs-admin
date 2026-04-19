//! `POST /v1/chat/completions` —— OpenAI 兼容聊天接口。
//!
//! # 当前（P3 walking skeleton）
//!
//! 调 [`crate::service::chat`]：`AdapterDispatcher::build_chat_request` → reqwest →
//! 非流式 `parse_chat_response` / 流式原样透传 bytes。
//!
//! **硬编码**：
//! - `AdapterKind::OpenAI`
//! - `base_url`：env `OPENAI_BASE_URL`（默认 `https://api.openai.com/v1`）
//! - `api_key`：env `OPENAI_API_KEY`（必须）
//! - `actual_model = request.model`（不做模型映射）
//!
//! # 后续 Phase
//!
//! - P4：从 `ai.channel` / `ai.channel_account` 读 upstream 配置（ChannelRouter::pick）
//! - P5：加 AiAuthLayer 鉴权
//! - P6：加 BillingLayer 三阶段扣费
//! - P3.5：流式响应走 `parse_chat_stream_event` → 重新序列化（egress converter）

use summer_admin_macros::no_auth;
use summer_ai_core::{AdapterKind, ChatRequest, ServiceTarget};
use summer_web::Router;
use summer_web::axum::Json;
use summer_web::axum::body::Body;
use summer_web::axum::http::{HeaderValue, StatusCode, header};
use summer_web::axum::response::{IntoResponse, Response};
use summer_web::extractor::Component;
use summer_web::handler::TypeRouter;
use summer_web::post;

use crate::error::{RelayError, RelayResult};
use crate::service::chat;

const DEFAULT_OPENAI_BASE_URL: &str = "https://api.openai.com/v1";

/// `POST /v1/chat/completions`
#[no_auth]
#[post("/v1/chat/completions")]
pub async fn chat_completions(
    Component(http): Component<reqwest::Client>,
    Json(request): Json<ChatRequest>,
) -> RelayResult<Response> {
    let api_key =
        std::env::var("OPENAI_API_KEY").map_err(|_| RelayError::MissingConfig("OPENAI_API_KEY"))?;
    let base_url =
        std::env::var("OPENAI_BASE_URL").unwrap_or_else(|_| DEFAULT_OPENAI_BASE_URL.to_string());

    // walking skeleton：硬编码 kind + actual_model = request.model
    let kind = AdapterKind::OpenAI;
    let target = ServiceTarget::bearer(base_url, api_key, &request.model);
    let is_stream = request.stream;

    tracing::debug!(
        model = %request.model,
        messages = request.messages.len(),
        stream = is_stream,
        "summer-ai chat_completions"
    );

    if is_stream {
        let upstream = chat::invoke_stream_raw(&http, kind, &target, &request).await?;
        let body = Body::from_stream(upstream.bytes_stream());
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
        let response = chat::invoke_non_stream(&http, kind, &target, &request).await?;
        Ok(Json(response).into_response())
    }
}

pub fn routes(router: Router) -> Router {
    router.typed_route(chat_completions)
}
