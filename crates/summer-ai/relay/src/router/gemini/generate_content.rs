//! `POST /v1beta/models/{model}:{generateContent|streamGenerateContent}` —— Gemini 入口。
//!
//! 选路：[`ChannelStore::pick`] → [`build_service_target`] 得到实际上游。
//! 流式通过 stream driver 把上游 SSE 重组成 Gemini SSE chunks (`data: {json}\n\n`)。

use summer_admin_macros::no_auth;
use summer_ai_core::types::ingress_wire::gemini::GeminiGenerateContentRequest;
use summer_web::Router;
use summer_web::axum::Json;
use summer_web::axum::body::Body;
use summer_web::axum::extract::Path;
use summer_web::axum::response::{IntoResponse, Response};
use summer_web::extractor::Component;
use summer_web::handler::TypeRouter;
use summer_web::post;

use crate::auth::AiToken;
use crate::convert::ingress::{GeminiIngress, IngressConverter, IngressCtx};
use crate::error::{RelayError, RelayResult};
use crate::service::channel_store::{ChannelStore, build_service_target};
use crate::service::stream_driver::sse_response;
use crate::service::{chat, stream_driver};

/// `POST /v1beta/models/{target}` 其中 `target = {model}:{method}`。
#[no_auth]
#[post("/v1beta/models/{target}")]
pub async fn generate_content(
    AiToken(ctx): AiToken,
    Path(target): Path<String>,
    Component(http): Component<reqwest::Client>,
    Component(store): Component<ChannelStore>,
    Json(gemini_req): Json<GeminiGenerateContentRequest>,
) -> RelayResult<Response> {
    // target 形如 "gemini-2.5-flash:generateContent" 或 ":streamGenerateContent"
    let Some((model, method)) = target.split_once(':') else {
        return Err(RelayError::MissingConfig(
            "invalid path: expected {model}:{generateContent|streamGenerateContent}",
        ));
    };

    let is_stream = method == "streamGenerateContent";
    let logical_model = model.to_string();

    let (channel, account) =
        store
            .pick(&logical_model)
            .await?
            .ok_or_else(|| RelayError::NoAvailableChannel {
                model: logical_model.clone(),
            })?;
    let (kind, upstream_target) = build_service_target(&channel, &account, &logical_model)?;

    tracing::debug!(
        token_id = ctx.token_id,
        user_id = ctx.user_id,
        model = %logical_model,
        actual_model = %upstream_target.actual_model,
        adapter = %kind.as_lower_str(),
        channel_id = channel.id,
        account_id = account.id,
        contents = gemini_req.contents.len(),
        stream = is_stream,
        method = %method,
        "gemini generate_content"
    );

    let ctx = IngressCtx::new(kind, &logical_model, &upstream_target.actual_model);
    let mut canonical_req = GeminiIngress::to_canonical(gemini_req, &ctx)?;
    canonical_req.stream = is_stream;

    if is_stream {
        let upstream =
            chat::invoke_stream_raw(&http, kind, &upstream_target, &canonical_req).await?;
        let body_stream =
            stream_driver::transcode_stream::<GeminiIngress>(upstream, kind, upstream_target, ctx);
        let body = Body::from_stream(body_stream);
        Ok(sse_response(body))
    } else {
        let canonical_resp =
            chat::invoke_non_stream(&http, kind, &upstream_target, &canonical_req).await?;
        let gemini_resp = GeminiIngress::from_canonical(canonical_resp, &ctx)?;
        Ok(Json(gemini_resp).into_response())
    }
}

pub fn routes(router: Router) -> Router {
    router.typed_route(generate_content)
}
