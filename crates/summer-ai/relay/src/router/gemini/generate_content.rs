//! `POST /v1beta/models/{model}:{generateContent|streamGenerateContent}` —— Gemini 入口。
//!
//! 上游当前硬编码为 OpenAI，`OPENAI_API_KEY` / `OPENAI_BASE_URL` 从 env 读。
//! 流式暂返 501（Gemini 流式需要把 canonical event 串行序列化成 SSE，
//! 需要 stream driver 集成，后续做）。

use summer_admin_macros::no_auth;
use summer_ai_core::types::ingress_wire::gemini::GeminiGenerateContentRequest;
use summer_ai_core::{AdapterKind, ServiceTarget};
use summer_web::Router;
use summer_web::axum::Json;
use summer_web::axum::extract::Path;
use summer_web::axum::response::{IntoResponse, Response};
use summer_web::extractor::Component;
use summer_web::handler::TypeRouter;
use summer_web::post;

use crate::convert::ingress::{GeminiIngress, IngressConverter, IngressCtx};
use crate::error::{RelayError, RelayResult};
use crate::service::chat;

const DEFAULT_OPENAI_BASE_URL: &str = "https://api.openai.com/v1";

/// `POST /v1beta/models/{target}` 其中 `target = {model}:{method}`。
#[no_auth]
#[post("/v1beta/models/{target}")]
pub async fn generate_content(
    Path(target): Path<String>,
    Component(http): Component<reqwest::Client>,
    Json(gemini_req): Json<GeminiGenerateContentRequest>,
) -> RelayResult<Response> {
    // target 形如 "gemini-2.5-flash:generateContent" 或 ":streamGenerateContent"
    let Some((model, method)) = target.split_once(':') else {
        return Err(RelayError::MissingConfig(
            "invalid path: expected {model}:{generateContent|streamGenerateContent}",
        ));
    };

    let is_stream = method == "streamGenerateContent";

    let api_key =
        std::env::var("OPENAI_API_KEY").map_err(|_| RelayError::MissingConfig("OPENAI_API_KEY"))?;
    let base_url =
        std::env::var("OPENAI_BASE_URL").unwrap_or_else(|_| DEFAULT_OPENAI_BASE_URL.to_string());

    let kind = AdapterKind::OpenAI;
    let model_string = model.to_string();

    tracing::debug!(
        model = %model_string,
        contents = gemini_req.contents.len(),
        stream = is_stream,
        method = %method,
        "gemini generate_content"
    );

    if is_stream {
        // 流式需要 SSE stream driver 集成（后续做）
        return Err(RelayError::NotImplemented("gemini streaming"));
    }

    let ctx = IngressCtx::new(kind, &model_string, &model_string);
    let canonical_req = GeminiIngress::to_canonical(gemini_req, &ctx)?;
    let upstream_target = ServiceTarget::bearer(base_url, api_key, &model_string);

    let canonical_resp =
        chat::invoke_non_stream(&http, kind, &upstream_target, &canonical_req).await?;
    let gemini_resp = GeminiIngress::from_canonical(canonical_resp, &ctx)?;

    Ok(Json(gemini_resp).into_response())
}

pub fn routes(router: Router) -> Router {
    router.typed_route(generate_content)
}
