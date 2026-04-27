use anyhow::Context;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder};
use summer::plugin::Service;
use summer_ai_model::dto::config_entry::{
    ConfigEntryQueryDto, CreateConfigEntryDto, UpdateConfigEntryDto,
};
use summer_ai_model::entity::platform::config_entry;
use summer_ai_model::vo::config_entry::ConfigEntryVo;
use summer_common::error::{ApiErrors, ApiResult};
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};

#[derive(Clone, Service)]
pub struct ConfigEntryService {
    #[inject(component)]
    db: DbConn,
}

impl ConfigEntryService {
    pub async fn list(
        &self,
        query: ConfigEntryQueryDto,
        pagination: Pagination,
    ) -> ApiResult<Page<ConfigEntryVo>> {
        let page: Page<config_entry::Model> = config_entry::Entity::find()
            .filter(query)
            .order_by_asc(config_entry::Column::ScopeType)
            .order_by_asc(config_entry::Column::ScopeId)
            .order_by_asc(config_entry::Column::Category)
            .order_by_asc(config_entry::Column::ConfigKey)
            .order_by_desc(config_entry::Column::VersionNo)
            .order_by_asc(config_entry::Column::Id)
            .page(&self.db, &pagination)
            .await
            .context("查询配置项列表失败")?;

        Ok(page.map(ConfigEntryVo::from_model))
    }

    pub async fn detail(&self, id: i64) -> ApiResult<ConfigEntryVo> {
        let model = self.find_model_by_id(id).await?;
        Ok(ConfigEntryVo::from_model(model))
    }

    pub async fn create(&self, dto: CreateConfigEntryDto, operator: &str) -> ApiResult<()> {
        dto.validate_business_rules()
            .map_err(ApiErrors::BadRequest)?;
        self.ensure_unique_config_entry(
            &dto.scope_type,
            dto.scope_id,
            &dto.category,
            &dto.config_key,
            None,
        )
        .await?;
        dto.into_active_model(operator)
            .map_err(ApiErrors::BadRequest)?
            .insert(&self.db)
            .await
            .context("创建配置项失败")?;
        Ok(())
    }

    pub async fn update(
        &self,
        id: i64,
        dto: UpdateConfigEntryDto,
        operator: &str,
    ) -> ApiResult<()> {
        let model = self.find_model_by_id(id).await?;
        dto.validate_business_rules(&model)
            .map_err(ApiErrors::BadRequest)?;

        let next_scope_type = dto
            .scope_type
            .clone()
            .unwrap_or_else(|| model.scope_type.clone());
        let next_scope_id = dto.scope_id.unwrap_or(model.scope_id);
        let next_category = dto
            .category
            .clone()
            .unwrap_or_else(|| model.category.clone());
        let next_config_key = dto
            .config_key
            .clone()
            .unwrap_or_else(|| model.config_key.clone());

        self.ensure_unique_config_entry(
            &next_scope_type,
            next_scope_id,
            &next_category,
            &next_config_key,
            Some(id),
        )
        .await?;

        let next_version_no = if dto.has_mutations() {
            model.version_no.saturating_add(1)
        } else {
            model.version_no
        };

        let mut active: config_entry::ActiveModel = model.into();
        dto.apply_to(&mut active, operator, next_version_no)
            .map_err(ApiErrors::BadRequest)?;
        active.update(&self.db).await.context("更新配置项失败")?;
        Ok(())
    }

    pub async fn delete(&self, id: i64) -> ApiResult<()> {
        let _model = self.find_model_by_id(id).await?;
        config_entry::Entity::delete_by_id(id)
            .exec(&self.db)
            .await
            .context("删除配置项失败")?;
        Ok(())
    }

    async fn ensure_unique_config_entry(
        &self,
        scope_type: &str,
        scope_id: i64,
        category: &str,
        config_key: &str,
        exclude_id: Option<i64>,
    ) -> ApiResult<()> {
        let normalized_scope_type = scope_type.trim().to_ascii_lowercase();
        let mut select = config_entry::Entity::find()
            .filter(config_entry::Column::ScopeType.eq(&normalized_scope_type))
            .filter(config_entry::Column::ScopeId.eq(scope_id))
            .filter(config_entry::Column::Category.eq(category))
            .filter(config_entry::Column::ConfigKey.eq(config_key));
        if let Some(exclude_id) = exclude_id {
            select = select.filter(config_entry::Column::Id.ne(exclude_id));
        }
        let exists = select.one(&self.db).await.context("检查配置项唯一性失败")?;
        if exists.is_some() {
            return Err(ApiErrors::Conflict(format!(
                "配置项已存在: scope_type={normalized_scope_type}, scope_id={scope_id}, category={category}, config_key={config_key}"
            )));
        }
        Ok(())
    }

    async fn find_model_by_id(&self, id: i64) -> ApiResult<config_entry::Model> {
        config_entry::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询配置项详情失败")?
            .ok_or_else(|| ApiErrors::NotFound(format!("配置项不存在: id={id}")))
    }
}
