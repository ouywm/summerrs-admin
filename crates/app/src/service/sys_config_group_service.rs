use anyhow::Context;
use common::error::{ApiErrors, ApiResult};
use model::dto::sys_config_group::{
    ConfigGroupQueryDto, CreateConfigGroupDto, UpdateConfigGroupDto,
};
use model::entity::{sys_config, sys_config_group};
use model::vo::sys_config_group::ConfigGroupVo;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, Set,
};
use summer::plugin::Service;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};
use summer_sea_orm::DbConn;

#[derive(Clone, Service)]
pub struct SysConfigGroupService {
    #[inject(component)]
    db: DbConn,
}

impl SysConfigGroupService {
    pub async fn list(
        &self,
        query: ConfigGroupQueryDto,
        pagination: Pagination,
    ) -> ApiResult<Page<ConfigGroupVo>> {
        let page = sys_config_group::Entity::find()
            .filter(query)
            .order_by_asc(sys_config_group::Column::GroupSort)
            .order_by_asc(sys_config_group::Column::Id)
            .page(&self.db, &pagination)
            .await
            .context("查询系统参数分组列表失败")?;

        Ok(page.map(ConfigGroupVo::from))
    }

    pub async fn get_by_id(&self, id: i64) -> ApiResult<ConfigGroupVo> {
        Ok(ConfigGroupVo::from(self.find_model_by_id(id).await?))
    }

    pub async fn create(&self, dto: CreateConfigGroupDto, operator: &str) -> ApiResult<()> {
        self.ensure_group_code_unique(&dto.group_code, None).await?;

        let mut active: sys_config_group::ActiveModel = dto.into();
        active.create_by = Set(operator.to_string());
        active.update_by = Set(operator.to_string());
        active
            .insert(&self.db)
            .await
            .context("创建系统参数分组失败")?;
        Ok(())
    }

    pub async fn update(
        &self,
        id: i64,
        dto: UpdateConfigGroupDto,
        operator: &str,
    ) -> ApiResult<()> {
        let mut active: sys_config_group::ActiveModel = self.find_model_by_id(id).await?.into();
        dto.apply_to(&mut active);
        active.update_by = Set(operator.to_string());
        active
            .update(&self.db)
            .await
            .context("更新系统参数分组失败")?;
        Ok(())
    }

    pub async fn delete(&self, id: i64) -> ApiResult<()> {
        let model = self.find_model_by_id(id).await?;
        if model.is_system {
            return Err(ApiErrors::BadRequest("系统内置分组不允许删除".to_string()));
        }

        let config_count = sys_config::Entity::find()
            .filter(sys_config::Column::ConfigGroupId.eq(id))
            .count(&self.db)
            .await
            .context("查询分组下配置数量失败")?;

        if config_count > 0 {
            return Err(ApiErrors::BadRequest(
                "分组下仍存在配置项，无法删除".to_string(),
            ));
        }

        let result = sys_config_group::Entity::delete_by_id(id)
            .exec(&self.db)
            .await
            .context("删除系统参数分组失败")?;

        if result.rows_affected == 0 {
            return Err(ApiErrors::NotFound("系统参数分组不存在".to_string()));
        }

        Ok(())
    }

    async fn find_model_by_id(&self, id: i64) -> ApiResult<sys_config_group::Model> {
        sys_config_group::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询系统参数分组详情失败")?
            .ok_or_else(|| ApiErrors::NotFound("系统参数分组不存在".to_string()))
    }

    async fn ensure_group_code_unique(
        &self,
        group_code: &str,
        exclude_id: Option<i64>,
    ) -> ApiResult<()> {
        let mut query = sys_config_group::Entity::find()
            .filter(sys_config_group::Column::GroupCode.eq(group_code));
        if let Some(exclude_id) = exclude_id {
            query = query.filter(sys_config_group::Column::Id.ne(exclude_id));
        }

        let existing = query
            .one(&self.db)
            .await
            .context("检查分组编码是否重复失败")?;

        if existing.is_some() {
            return Err(ApiErrors::Conflict(format!(
                "分组编码已存在: {}",
                group_code
            )));
        }

        Ok(())
    }
}
