use anyhow::Context;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use summer::plugin::Service;
use summer_sea_orm::DbConn;

use summer_ai_core::types::model::{ModelListResponse, ModelObject};
use summer_ai_model::entity::ability;
use summer_ai_model::entity::model_config;
use summer_common::error::{ApiErrors, ApiResult};

#[derive(Clone, Service)]
pub struct ModelService {
    #[inject(component)]
    db: DbConn,
}

impl ModelService {
    /// 获取指定分组可用的模型列表
    pub async fn list_available(&self, group: &str) -> ApiResult<ModelListResponse> {
        // 从 ability 表查询可用模型（去重）
        let abilities = ability::Entity::find()
            .filter(ability::Column::ChannelGroup.eq(group))
            .filter(ability::Column::Enabled.eq(true))
            .all(&self.db)
            .await
            .context("查询可用模型失败")
            .map_err(ApiErrors::Internal)?;

        let mut model_names: Vec<String> = abilities.into_iter().map(|a| a.model).collect();
        model_names.sort();
        model_names.dedup();

        // 查询模型配置以补充详情
        let configs = model_config::Entity::find()
            .filter(model_config::Column::Enabled.eq(true))
            .all(&self.db)
            .await
            .context("查询模型配置失败")
            .map_err(ApiErrors::Internal)?;

        let config_map: std::collections::HashMap<String, &model_config::Model> =
            configs.iter().map(|c| (c.model_name.clone(), c)).collect();

        let data: Vec<ModelObject> = model_names
            .into_iter()
            .map(|name| {
                let cfg = config_map.get(&name);
                ModelObject {
                    id: name.clone(),
                    object: "model".into(),
                    created: cfg.map(|c| c.create_time.timestamp()).unwrap_or(0),
                    owned_by: cfg
                        .map(|c| c.vendor_code.clone())
                        .unwrap_or_else(|| "unknown".into()),
                }
            })
            .collect();

        Ok(ModelListResponse {
            object: "list".into(),
            data,
        })
    }
}
