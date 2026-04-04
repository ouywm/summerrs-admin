use anyhow::Context;
use sea_orm::{ActiveModelTrait, EntityTrait, QueryFilter, QueryOrder, Set};
use summer::plugin::Service;
use summer_common::error::{ApiErrors, ApiResult};
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};

use summer_ai_model::dto::platform::*;
use summer_ai_model::entity::config_entry;
use summer_ai_model::vo::platform::*;

#[derive(Clone, Service)]
pub struct PlatformConfigService {
    #[inject(component)]
    db: DbConn,
}

impl PlatformConfigService {
    pub async fn list_configs(
        &self,
        query: QueryConfigEntryDto,
        pagination: Pagination,
    ) -> ApiResult<Page<ConfigEntryVo>> {
        let page = config_entry::Entity::find()
            .filter(query)
            .order_by_asc(config_entry::Column::Category)
            .order_by_asc(config_entry::Column::ConfigKey)
            .page(&self.db, &pagination)
            .await
            .context("查询配置列表失败")?;
        Ok(page.map(ConfigEntryVo::from_model))
    }
    pub async fn get_config(&self, id: i64) -> ApiResult<ConfigEntryVo> {
        let m = config_entry::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询配置失败")
            .map_err(ApiErrors::Internal)?
            .ok_or_else(|| ApiErrors::NotFound("配置不存在".to_string()))?;
        Ok(ConfigEntryVo::from_model(m))
    }
    pub async fn create_config(
        &self,
        dto: CreateConfigEntryDto,
        operator: &str,
    ) -> ApiResult<ConfigEntryVo> {
        let now = chrono::Utc::now().fixed_offset();
        let a = config_entry::ActiveModel {
            config_key: Set(dto.config_key),
            config_value: Set(dto.config_value),
            value_type: Set(dto.value_type),
            category: Set(dto.category),
            description: Set(dto.description),
            status: Set(1),
            create_by: Set(operator.into()),
            update_by: Set(operator.into()),
            create_time: Set(now),
            update_time: Set(now),
            ..Default::default()
        };
        let m = a
            .insert(&self.db)
            .await
            .context("创建配置失败")
            .map_err(ApiErrors::Internal)?;
        Ok(ConfigEntryVo::from_model(m))
    }
    pub async fn update_config(
        &self,
        id: i64,
        dto: UpdateConfigEntryDto,
        operator: &str,
    ) -> ApiResult<ConfigEntryVo> {
        let m = config_entry::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询")
            .map_err(ApiErrors::Internal)?
            .ok_or_else(|| ApiErrors::NotFound("配置不存在".to_string()))?;
        let mut a: config_entry::ActiveModel = m.into();
        if let Some(v) = dto.config_value {
            a.config_value = Set(v);
        }
        if let Some(v) = dto.description {
            a.description = Set(v);
        }
        if let Some(v) = dto.status {
            a.status = Set(v);
        }
        a.update_by = Set(operator.into());
        let u = a
            .update(&self.db)
            .await
            .context("更新配置失败")
            .map_err(ApiErrors::Internal)?;
        Ok(ConfigEntryVo::from_model(u))
    }
    pub async fn delete_config(&self, id: i64) -> ApiResult<()> {
        config_entry::Entity::delete_by_id(id)
            .exec(&self.db)
            .await
            .context("删除配置失败")
            .map_err(ApiErrors::Internal)?;
        Ok(())
    }
}
