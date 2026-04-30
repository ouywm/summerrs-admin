//! `GET /v1/models` —— OpenAI 兼容模型列表接口。
//!
//! 从 `ai.model_config` 查 `enabled = true` 的全部模型并按 OpenAI 官方格式返回。
//! 每行 `model_config` 的 `vendor_code` 映射为 `owned_by`、`create_time` 为 `created`，
//! 与 OpenAI 官方 `{id, object: "model", created, owned_by}` 语义对齐。
//!
//! 后续（P7 Admin CRUD / token 化）可能按 token 的 `allowed_models` 做过滤，
//! 当前阶段不做。

use summer_ai_core::ModelList;
use summer_web::axum::Json;
use summer_web::axum::response::{IntoResponse, Response};
use summer_web::extractor::Component;
use summer_web::get;

use crate::error::OpenAIResult;
use crate::service::model_service::ModelService;

/// `GET /v1/models`
#[get("/v1/models", group = "summer-ai-relay::openai")]
pub async fn list_models(Component(svc): Component<ModelService>) -> OpenAIResult<Response> {
    let data = svc.list_enabled().await?;
    Ok(Json(ModelList::new(data)).into_response())
}
