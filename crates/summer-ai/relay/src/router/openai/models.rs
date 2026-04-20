//! `GET /v1/models` —— OpenAI 兼容模型列表接口。
//!
//! 从 `ai.channel.models` JSONB 聚合去重得到模型清单，不再向上游 `/v1/models` 代理。
//! 运维侧在 `ai.channel` 增删模型即时生效（下次请求就重查）。
//!
//! 后续（P7 Admin CRUD）可能按 token 的 `allowed_models` 做过滤，当前阶段不做。

use summer_ai_core::{ModelInfo, ModelList};
use summer_web::axum::Json;
use summer_web::axum::response::{IntoResponse, Response};
use summer_web::extractor::Component;
use summer_web::get;

use crate::error::OpenAIResult;
use crate::service::channel_store::ChannelStore;

/// `GET /v1/models`
#[get("/v1/models")]
pub async fn list_models(Component(store): Component<ChannelStore>) -> OpenAIResult<Response> {
    let ids = store.list_enabled_model_names().await?;
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
