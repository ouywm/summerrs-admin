//! `GET /v1/models` —— OpenAI 兼容模型列表接口。
//!
//! # 当前（P3 walking skeleton）
//!
//! 走 [`AdapterDispatcher::fetch_model_names`]（与 `/v1/chat/completions` 一致），
//! 向上游拉取真实模型清单并以 OpenAI `ModelList` 格式返回。
//!
//! **硬编码**：
//! - `AdapterKind::OpenAI`
//! - `base_url`：env `OPENAI_BASE_URL`（默认 `https://api.openai.com/v1`）
//! - `api_key`：env `OPENAI_API_KEY`（必须）
//!
//! # 后续 Phase
//!
//! - P4：从 `ai.channel.models` JSONB 聚合去重，不再去上游拉

use summer_admin_macros::no_auth;
use summer_ai_core::{AdapterDispatcher, AdapterKind, ModelInfo, ModelList, ServiceTarget};
use summer_web::Router;
use summer_web::axum::Json;
use summer_web::axum::response::{IntoResponse, Response};
use summer_web::extractor::Component;
use summer_web::get;
use summer_web::handler::TypeRouter;

use crate::error::{RelayError, RelayResult};

const DEFAULT_OPENAI_BASE_URL: &str = "https://api.openai.com/v1";

/// `GET /v1/models`
#[no_auth]
#[get("/v1/models")]
pub async fn list_models(Component(http): Component<reqwest::Client>) -> RelayResult<Response> {
    let api_key =
        std::env::var("OPENAI_API_KEY").map_err(|_| RelayError::MissingConfig("OPENAI_API_KEY"))?;
    let base_url =
        std::env::var("OPENAI_BASE_URL").unwrap_or_else(|_| DEFAULT_OPENAI_BASE_URL.to_string());

    let kind = AdapterKind::OpenAI;
    // actual_model 在 fetch_model_names 里不会用到，给空即可
    let target = ServiceTarget::bearer(base_url, api_key, "");

    let ids = AdapterDispatcher::fetch_model_names(kind, &target, &http).await?;

    let list = ModelList::new(
        ids.into_iter()
            .map(|id| ModelInfo {
                id,
                object: "model".to_string(),
                created: 0,
                owned_by: "summer-ai".to_string(),
            })
            .collect(),
    );

    Ok(Json(list).into_response())
}

pub fn routes(router: Router) -> Router {
    router.typed_route(list_models)
}
