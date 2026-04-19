//! `POST /v1/responses` —— OpenAI Responses API 入口。
//!
//! 走路径：
//! - 客户端用 Responses API 格式发请求
//! - [`ChannelStore::pick`] 选 (channel, account) → [`build_service_target`]
//! - [`OpenAIResponsesIngress::to_canonical`] 翻译成 canonical `ChatRequest`
//! - 复用 [`crate::service::chat`] 发给上游（任意 chat 兼容上游）
//! - 非流式：[`OpenAIResponsesIngress::from_canonical`] 翻译回 Responses JSON
//! - 流式：[`crate::service::stream_driver::transcode_stream`] 把上游 SSE
//!   重组成 Responses 10+ event SSE（`response.created` … `response.completed`）

use summer_admin_macros::no_auth;
use summer_ai_core::types::ingress_wire::openai_responses::OpenAIResponsesRequest;
use summer_web::Router;
use summer_web::axum::Json;
use summer_web::axum::body::Body;
use summer_web::axum::response::{IntoResponse, Response};
use summer_web::extractor::Component;
use summer_web::handler::TypeRouter;
use summer_web::post;

use crate::auth::AiToken;
use crate::convert::ingress::{IngressConverter, IngressCtx, OpenAIResponsesIngress};
use crate::error::{RelayError, RelayResult};
use crate::service::channel_store::{ChannelStore, build_service_target};
use crate::service::stream_driver::sse_response;
use crate::service::{chat, stream_driver};

/// `POST /v1/responses`
#[no_auth]
#[post("/v1/responses")]
pub async fn responses(
    AiToken(ctx): AiToken,
    Component(http): Component<reqwest::Client>,
    Component(store): Component<ChannelStore>,
    Json(req): Json<OpenAIResponsesRequest>,
) -> RelayResult<Response> {
    let logical_model = req.model.clone();
    let is_stream = req.stream;

    let (channel, account) =
        store
            .pick(&logical_model)
            .await?
            .ok_or_else(|| RelayError::NoAvailableChannel {
                model: logical_model.clone(),
            })?;
    let (kind, target) = build_service_target(&channel, &account, &logical_model)?;

    tracing::debug!(
        token_id = ctx.token_id,
        user_id = ctx.user_id,
        model = %logical_model,
        actual_model = %target.actual_model,
        adapter = %kind.as_lower_str(),
        channel_id = channel.id,
        account_id = account.id,
        stream = is_stream,
        "openai /v1/responses"
    );

    let ctx = IngressCtx::new(kind, &logical_model, &target.actual_model);

    // client wire → canonical
    let mut canonical_req = OpenAIResponsesIngress::to_canonical(req, &ctx)?;
    // `to_canonical` 内部把 `stream` 硬编码为 false（converter 是纯协议翻译层，
    // 不关心调用策略）。这里由 handler 统一覆盖成真实值——决定后面走
    // invoke_stream_raw 还是 invoke_non_stream，以及让 adapter 的
    // build_chat_request 据此选对 URL / payload。
    canonical_req.stream = is_stream;

    if is_stream {
        let upstream = chat::invoke_stream_raw(&http, kind, &target, &canonical_req).await?;
        let body_stream =
            stream_driver::transcode_stream::<OpenAIResponsesIngress>(upstream, kind, target, ctx);
        let body = Body::from_stream(body_stream);
        Ok(sse_response(body))
    } else {
        let canonical_resp = chat::invoke_non_stream(&http, kind, &target, &canonical_req).await?;
        let responses_resp = OpenAIResponsesIngress::from_canonical(canonical_resp, &ctx)?;
        Ok(Json(responses_resp).into_response())
    }
}

pub fn routes(router: Router) -> Router {
    router.typed_route(responses)
}
