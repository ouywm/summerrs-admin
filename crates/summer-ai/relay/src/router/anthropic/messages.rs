//! `POST /v1/messages` —— Anthropic Messages API 兼容入口。
//!
//! # 当前（走路骨架）
//!
//! - 客户端用 Anthropic SDK 格式发请求
//! - 通过 [`AnthropicIngress::to_canonical`] 翻译成 canonical
//! - 复用 [`crate::service::chat`] 发给 OpenAI 上游
//! - 非流式响应再用 [`AnthropicIngress::from_canonical`] 翻译回 Anthropic 格式
//!
//! **硬编码**：
//! - 上游 `AdapterKind::OpenAI`
//! - `base_url`：env `OPENAI_BASE_URL`（默认 `https://api.openai.com/v1`）
//! - `api_key`：env `OPENAI_API_KEY`（必须）
//! - `actual_model = request.model`（不做模型映射）
//!
//! # 流式暂不支持
//!
//! `AnthropicIngress::from_canonical_stream_event` 尚未实装 6-event 重组状态机，
//! 流式请求会直接返 501。等流式状态机落地后开放。

use summer_admin_macros::no_auth;
use summer_ai_core::types::ingress_wire::anthropic::AnthropicMessagesRequest;
use summer_ai_core::{AdapterKind, ServiceTarget};
use summer_web::Router;
use summer_web::axum::Json;
use summer_web::axum::response::{IntoResponse, Response};
use summer_web::extractor::Component;
use summer_web::handler::TypeRouter;
use summer_web::post;

use crate::convert::ingress::{AnthropicIngress, IngressConverter, IngressCtx};
use crate::error::{RelayError, RelayResult};
use crate::service::chat;

const DEFAULT_OPENAI_BASE_URL: &str = "https://api.openai.com/v1";

/// `POST /v1/messages`
#[no_auth]
#[post("/v1/messages")]
pub async fn messages(
    Component(http): Component<reqwest::Client>,
    Json(claude_req): Json<AnthropicMessagesRequest>,
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

    if is_stream {
        // Anthropic SSE 6-event 状态机尚未实装——非流式闭环先跑通
        return Err(RelayError::NotImplemented("claude streaming"));
    }

    // Ingress 上下文（硬编码：上游 OpenAI，walking skeleton 阶段不做 model mapping）
    let ctx = IngressCtx::new(kind, &logical_model, &logical_model);

    // client wire → canonical
    let canonical_req = AnthropicIngress::to_canonical(claude_req, &ctx)?;

    let target = ServiceTarget::bearer(base_url, api_key, &logical_model);

    // 走共享的 non-stream 链路（AdapterDispatcher::build_chat_request → reqwest → parse）
    let canonical_resp = chat::invoke_non_stream(&http, kind, &target, &canonical_req).await?;

    // canonical → client wire
    let claude_resp = AnthropicIngress::from_canonical(canonical_resp, &ctx)?;

    Ok(Json(claude_resp).into_response())
}

pub fn routes(router: Router) -> Router {
    router.typed_route(messages)
}
