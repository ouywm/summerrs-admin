//! `POST /v1beta/models/{model}:{generateContent|streamGenerateContent}` —— Gemini 入口。
//!
//! 上游当前硬编码为 OpenAI，`OPENAI_API_KEY` / `OPENAI_BASE_URL` 从 env 读。
//! 流式通过 stream driver 把上游 SSE 重组成 Gemini SSE chunks (`data: {json}\n\n`)。

use summer_admin_macros::no_auth;
use summer_ai_core::types::ingress_wire::gemini::GeminiGenerateContentRequest;
use summer_ai_core::{AdapterKind, ServiceTarget};
use summer_web::Router;
use summer_web::axum::Json;
use summer_web::axum::body::Body;
use summer_web::axum::extract::Path;
use summer_web::axum::http::{HeaderValue, StatusCode, header};
use summer_web::axum::response::{IntoResponse, Response};
use summer_web::extractor::Component;
use summer_web::handler::TypeRouter;
use summer_web::post;

use crate::convert::ingress::{GeminiIngress, IngressConverter, IngressCtx};
use crate::error::{RelayError, RelayResult};
use crate::service::{chat, stream_driver};

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

    let ctx = IngressCtx::new(kind, &model_string, &model_string);
    let mut canonical_req = GeminiIngress::to_canonical(gemini_req, &ctx)?;
    canonical_req.stream = is_stream;

    let upstream_target = ServiceTarget::bearer(base_url, api_key, &model_string);

    if is_stream {
        let upstream =
            chat::invoke_stream_raw(&http, kind, &upstream_target, &canonical_req).await?;
        let body_stream =
            stream_driver::transcode_stream::<GeminiIngress>(upstream, kind, upstream_target, ctx);
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
        let canonical_resp =
            chat::invoke_non_stream(&http, kind, &upstream_target, &canonical_req).await?;
        let gemini_resp = GeminiIngress::from_canonical(canonical_resp, &ctx)?;
        Ok(Json(gemini_resp).into_response())
    }
}

pub fn routes(router: Router) -> Router {
    router.typed_route(generate_content)
}
