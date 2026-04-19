//! `POST /v1/chat/completions` —— OpenAI 兼容聊天接口。
//!
//! 走路径：[`AiAuthLayer`](crate::auth::AiAuthLayer) 鉴权注入 [`AiTokenContext`](crate::auth::AiTokenContext)
//! → [`ChannelStore::pick`] 选 (channel, account) → [`build_service_target`] 生成
//! `(AdapterKind, ServiceTarget)` → [`crate::service::chat`] 发上游 → 非流式 parse /
//! 流式原样透传 bytes。
//!
//! `#[no_auth]` 保留：它告诉 summer-auth 全局 AuthLayer 跳过（避免把 Bearer sk-xxx
//! 当成 JWT 解析）；API Key 鉴权由本 crate 独立的 AiAuthLayer 完成。
//!
//! # 后续待加
//!
//! - BillingLayer 三阶段扣费
//! - 流式响应走 canonical event 解析 + 重新序列化（egress converter）

use summer_admin_macros::no_auth;
use summer_ai_core::ChatRequest;
use summer_web::Router;
use summer_web::axum::Json;
use summer_web::axum::body::Body;
use summer_web::axum::response::{IntoResponse, Response};
use summer_web::extractor::Component;
use summer_web::handler::TypeRouter;
use summer_web::post;

use crate::auth::AiToken;
use crate::error::{RelayError, RelayResult};
use crate::service::channel_store::{ChannelStore, build_service_target};
use crate::service::chat;
use crate::service::stream_driver::sse_response;

/// `POST /v1/chat/completions`
#[no_auth]
#[post("/v1/chat/completions")]
pub async fn chat_completions(
    AiToken(ctx): AiToken,
    Component(http): Component<reqwest::Client>,
    Component(store): Component<ChannelStore>,
    Json(request): Json<ChatRequest>,
) -> RelayResult<Response> {
    let logical_model = request.model.clone();
    let (channel, account) =
        store
            .pick(&logical_model)
            .await?
            .ok_or_else(|| RelayError::NoAvailableChannel {
                model: logical_model.clone(),
            })?;
    let (kind, target) = build_service_target(&channel, &account, &logical_model)?;
    let is_stream = request.stream;

    tracing::debug!(
        token_id = ctx.token_id,
        user_id = ctx.user_id,
        model = %logical_model,
        actual_model = %target.actual_model,
        channel_id = channel.id,
        account_id = account.id,
        messages = request.messages.len(),
        stream = is_stream,
        "summer-ai chat_completions"
    );

    if is_stream {
        let upstream = chat::invoke_stream_raw(&http, kind, &target, &request).await?;
        let body = Body::from_stream(upstream.bytes_stream());
        Ok(sse_response(body))
    } else {
        let response = chat::invoke_non_stream(&http, kind, &target, &request).await?;
        Ok(Json(response).into_response())
    }
}

pub fn routes(router: Router) -> Router {
    router.typed_route(chat_completions)
}
