//! `POST /v1beta/models/{model}:{generateContent|streamGenerateContent}` —— Gemini 入口。
//!
//! Handler 负责：
//! - 从 path 里拆出 `{model}:{method}` —— 决定 `is_stream`
//! - 从 `?alt=sse` query 决定流式呈现模式（SSE vs JSON-array）
//! - 调 [`PipelineCall::execute`] 跑完整流程
//! - 流式：按 `wants_sse` 选 [`sse_response`] 或 [`collect_sse_to_json_array`]
//!
//! # 流式响应的两种模式
//!
//! Gemini `streamGenerateContent` 官方行为：
//! - 默认（无 `?alt=sse`）返 JSON array：`Content-Type: application/json`
//! - `?alt=sse` 返 SSE：`Content-Type: text/event-stream`
//!
//! google-genai SDK 默认带 `alt=sse`，所以 SDK 用户透明；裸 HTTP 用户取决于是否带参数。

use serde::Deserialize;
use summer_ai_core::AdapterError;
use summer_ai_core::types::ingress_wire::gemini::GeminiGenerateContentRequest;
use summer_web::axum::Json;
use summer_web::axum::body::Body;
use summer_web::axum::extract::{Path, Query};
use summer_web::axum::response::{IntoResponse, Response};
use summer_web::extractor::Component;
use summer_web::post;

use crate::auth::AiToken;
use crate::convert::ingress::{GeminiIngress, IngressFormat};
use crate::error::{GeminiResult, RelayError};
use crate::extract::RelayRequestMeta;
use crate::pipeline::{EngineOutcome, PipelineCall};
use crate::service::channel_store::ChannelStore;
use crate::service::stream_driver::{collect_sse_to_json_array, sse_response};
use crate::service::tracking::TrackingService;

/// `?alt=sse` 的 query 参数。
#[derive(Debug, Default, Deserialize)]
pub struct GeminiQueryParams {
    /// `"sse"` → SSE 模式；缺省或其他值 → JSON array 模式（仅对 `streamGenerateContent` 生效）。
    #[serde(default)]
    pub alt: Option<String>,
}

impl GeminiQueryParams {
    fn wants_sse(&self) -> bool {
        self.alt.as_deref() == Some("sse")
    }
}

/// `POST /v1beta/models/{target}` 其中 `target = {model}:{method}`。
#[post("/v1beta/models/{target}")]
pub async fn generate_content(
    AiToken(token): AiToken,
    Path(target): Path<String>,
    Query(params): Query<GeminiQueryParams>,
    Component(http): Component<reqwest::Client>,
    Component(store): Component<ChannelStore>,
    Component(tracking): Component<TrackingService>,
    meta: RelayRequestMeta,
    Json(gemini_req): Json<GeminiGenerateContentRequest>,
) -> GeminiResult<Response> {
    // target 形如 "gemini-2.5-flash:generateContent" 或 ":streamGenerateContent"
    let Some((model, method)) = target.split_once(':') else {
        return Err(RelayError::Adapter(AdapterError::Unsupported {
            adapter: "gemini",
            feature: "invalid path: expected {model}:{generateContent|streamGenerateContent}",
        })
        .into());
    };

    let is_stream = method == "streamGenerateContent";
    let wants_sse = params.wants_sse();
    let logical_model = model.to_string();
    let client_req_snapshot = serde_json::to_value(&gemini_req).ok();

    let call = PipelineCall::<GeminiIngress> {
        endpoint: meta.endpoint,
        format: IngressFormat::Gemini,
        token,
        is_stream,
        logical_model,
        client_ip: meta.client_ip,
        user_agent: meta.user_agent,
        client_headers: meta.client_headers,
        client_req: gemini_req,
        client_req_snapshot,
        http,
        store,
        tracking,
    };

    match call.execute().await? {
        EngineOutcome::NonStream(resp) => Ok(Json(resp).into_response()),
        EngineOutcome::Stream(body_stream) => {
            if wants_sse {
                Ok(sse_response(Body::from_stream(body_stream)))
            } else {
                // Gemini 默认（不带 `?alt=sse`）的 JSON-array 模式
                let events = collect_sse_to_json_array(body_stream).await?;
                Ok(Json(events).into_response())
            }
        }
    }
}
