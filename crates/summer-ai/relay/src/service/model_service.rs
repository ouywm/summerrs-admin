//! ModelService —— AI 模型元数据查询服务。
//!
//! `ai.model_config` 是模型元数据的**权威表**：`model_name` / `display_name` /
//! `vendor_code` / `supported_endpoints` / `enabled` / 计费倍率等字段都在这。
//! `ai.channel.models` 只描述"某 channel 能承载哪些模型"，**不是**元数据源头。

use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder};
use summer::plugin::Service;
use summer_ai_core::ModelInfo;
use summer_ai_model::entity::billing::model_config;
use summer_sea_orm::DbConn;

use crate::error::RelayError;

/// 模型元数据查询（DB-backed；非热路径，不走 Redis）。
#[derive(Clone, Service)]
pub struct ModelService {
    #[inject(component)]
    db: DbConn,
}

impl ModelService {
    /// 列出所有 `enabled = true` 的模型，按 `model_name` 字典序。
    ///
    /// 映射到 OpenAI `/v1/models` 的 [`ModelInfo`] 格式：
    /// - `id` = `model_name`（客户端调用时传的 `model` 字段）
    /// - `owned_by` = `vendor_code`（`"openai"` / `"anthropic"` / `"google"` ...，
    ///   与 OpenAI 官方字段语义一致）
    /// - `created` = `create_time` 的 Unix 秒戳（OpenAI 官方是模型上线时间戳）
    /// - `object` = `"model"`（由 `ModelInfo::Default` 兜底）
    pub async fn list_enabled(&self) -> Result<Vec<ModelInfo>, RelayError> {
        let rows = model_config::Entity::find()
            .filter(model_config::Column::Enabled.eq(true))
            .order_by_asc(model_config::Column::ModelName)
            .all(&self.db)
            .await
            .map_err(RelayError::Database)?;

        Ok(rows
            .into_iter()
            .map(|m| ModelInfo {
                id: m.model_name,
                object: "model".to_string(),
                created: m.create_time.timestamp(),
                owned_by: m.vendor_code,
            })
            .collect())
    }
}
