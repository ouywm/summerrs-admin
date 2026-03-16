//! Generated admin service skeleton.

use anyhow::Context;
use common::error::{ApiErrors, ApiResult};
use model::dto::sys_config::{ConfigQueryDto, CreateConfigDto, UpdateConfigDto};
use model::entity::sys_config;
use model::vo::sys_config::ConfigVo;
use sea_orm::{ActiveModelTrait, EntityTrait, QueryFilter};
use summer::plugin::Service;
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};

#[derive(Clone, Service)]
pub struct SysConfigService {
    #[inject(component)]
    db: DbConn,
}

impl SysConfigService {
    pub async fn list(
        &self,
        query: ConfigQueryDto,
        pagination: Pagination,
    ) -> ApiResult<Page<ConfigVo>> {
        let page = sys_config::Entity::find()
            .filter(query)
            .page(&self.db, &pagination)
            .await
            .context("查询系统参数配置表列表失败")?;

        Ok(page.map(ConfigVo::from))
    }

    pub async fn get_by_id(&self, id: i64) -> ApiResult<ConfigVo> {
        let model = sys_config::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询系统参数配置表详情失败")?
            .ok_or_else(|| ApiErrors::NotFound("系统参数配置表不存在".to_string()))?;

        Ok(ConfigVo::from(model))
    }

    pub async fn create(&self, dto: CreateConfigDto) -> ApiResult<()> {
        let active: sys_config::ActiveModel = dto.into();
        active
            .insert(&self.db)
            .await
            .context("创建系统参数配置表失败")?;
        Ok(())
    }

    pub async fn update(&self, id: i64, dto: UpdateConfigDto) -> ApiResult<()> {
        let model = sys_config::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询系统参数配置表详情失败")?
            .ok_or_else(|| ApiErrors::NotFound("系统参数配置表不存在".to_string()))?;

        let mut active: sys_config::ActiveModel = model.into();
        dto.apply_to(&mut active);
        active
            .update(&self.db)
            .await
            .context("更新系统参数配置表失败")?;
        Ok(())
    }

    pub async fn delete(&self, id: i64) -> ApiResult<()> {
        let result = sys_config::Entity::delete_by_id(id)
            .exec(&self.db)
            .await
            .context("删除系统参数配置表失败")?;

        if result.rows_affected == 0 {
            return Err(ApiErrors::NotFound("系统参数配置表不存在".to_string()));
        }

        Ok(())
    }
}
