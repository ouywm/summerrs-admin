use anyhow::Context;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder};
use summer::plugin::Service;
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};

use summer_ai_core::types::model::{ModelListResponse, ModelObject};
use summer_ai_model::dto::model_config::{
    CreateModelConfigDto, QueryModelConfigDto, UpdateModelConfigDto,
};
use summer_ai_model::entity::ability;
use summer_ai_model::entity::model_config;
use summer_ai_model::vo::model_config::ModelConfigVo;
use summer_common::error::{ApiErrors, ApiResult};

#[derive(Clone, Service)]
pub struct ModelService {
    #[inject(component)]
    db: DbConn,
}

impl ModelService {
    pub async fn list_model_configs(
        &self,
        query: QueryModelConfigDto,
        pagination: Pagination,
    ) -> ApiResult<Page<ModelConfigVo>> {
        let page = model_config::Entity::find()
            .filter(query)
            .order_by_desc(model_config::Column::Enabled)
            .order_by_desc(model_config::Column::Id)
            .page(&self.db, &pagination)
            .await
            .context("failed to list model configs")
            .map_err(ApiErrors::Internal)?;

        Ok(page.map(ModelConfigVo::from_model))
    }

    pub async fn get_model_config(&self, id: i64) -> ApiResult<ModelConfigVo> {
        let model = model_config::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("failed to query model config")
            .map_err(ApiErrors::Internal)?
            .ok_or_else(|| ApiErrors::NotFound("model config not found".into()))?;
        Ok(ModelConfigVo::from_model(model))
    }

    pub async fn create_model_config(
        &self,
        dto: CreateModelConfigDto,
        operator: &str,
    ) -> ApiResult<ModelConfigVo> {
        let model = dto
            .into_active_model(operator)
            .insert(&self.db)
            .await
            .context("failed to create model config")
            .map_err(ApiErrors::Internal)?;
        Ok(ModelConfigVo::from_model(model))
    }

    pub async fn update_model_config(
        &self,
        id: i64,
        dto: UpdateModelConfigDto,
        operator: &str,
    ) -> ApiResult<ModelConfigVo> {
        let mut active: model_config::ActiveModel = model_config::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("failed to query model config")
            .map_err(ApiErrors::Internal)?
            .ok_or_else(|| ApiErrors::NotFound("model config not found".into()))?
            .into();

        dto.apply_to(&mut active, operator);
        let model = active
            .update(&self.db)
            .await
            .context("failed to update model config")
            .map_err(ApiErrors::Internal)?;
        Ok(ModelConfigVo::from_model(model))
    }

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
