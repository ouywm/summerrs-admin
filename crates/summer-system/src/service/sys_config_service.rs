use anyhow::Context;
use summer_common::error::{ApiErrors, ApiResult};
use summer_model::dto::sys_config::{
    ConfigGroupFilterQueryDto, ConfigKeysDto, ConfigQueryDto, CreateConfigDto, UpdateConfigDto,
};
use summer_model::entity::{sys_config, sys_config_group};
use summer_model::vo::sys_config::{ConfigDetailVo, ConfigGroupBlockVo, ConfigGroupItemVo, ConfigValueVo};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set};
use std::collections::HashMap;
use summer::plugin::Service;
use summer_sea_orm::DbConn;

#[derive(Clone, Service)]
pub struct SysConfigService {
    #[inject(component)]
    db: DbConn,
}

impl SysConfigService {
    pub async fn get_by_id(&self, id: i64) -> ApiResult<ConfigDetailVo> {
        let result = sys_config::Entity::find_by_id(id)
            .find_also_related(sys_config_group::Entity)
            .one(&self.db)
            .await
            .context("查询系统参数配置详情失败")?
            .ok_or_else(|| ApiErrors::NotFound("系统参数配置不存在".to_string()))?;

        let (config, group) = result;
        Ok(ConfigDetailVo::from_model(config, group))
    }

    pub async fn grouped(
        &self,
        config: ConfigQueryDto,
        group: ConfigGroupFilterQueryDto,
    ) -> ApiResult<Vec<ConfigGroupBlockVo>> {
        let has_config_filters = config.has_filters();

        let groups = sys_config_group::Entity::find()
            .filter(group)
            .order_by_asc(sys_config_group::Column::GroupSort)
            .order_by_asc(sys_config_group::Column::Id)
            .all(&self.db)
            .await
            .context("查询系统参数分组列表失败")?;
        if groups.is_empty() {
            return Ok(Vec::new());
        }
        let group_ids: Vec<i64> = groups.iter().map(|group| group.id).collect();
        let group_count = group_ids.len();

        let configs = sys_config::Entity::find()
            .filter(config)
            .filter(sys_config::Column::ConfigGroupId.is_in(group_ids))
            .order_by_asc(sys_config::Column::ConfigGroupId)
            .order_by_asc(sys_config::Column::ConfigSort)
            .order_by_asc(sys_config::Column::Id)
            .all(&self.db)
            .await
            .context("查询系统参数配置分组数据失败")?;

        let mut items_by_group = HashMap::<i64, Vec<ConfigGroupItemVo>>::with_capacity(group_count);
        for config in configs {
            items_by_group
                .entry(config.config_group_id)
                .or_default()
                .push(ConfigGroupItemVo::from(config));
        }

        let blocks = groups
            .into_iter()
            .filter_map(|group| {
                let items = items_by_group.remove(&group.id).unwrap_or_default();
                if has_config_filters && items.is_empty() {
                    None
                } else {
                    Some(ConfigGroupBlockVo::from_model(group, items))
                }
            })
            .collect();

        Ok(blocks)
    }

    pub async fn get_by_key(&self, config_key: &str) -> ApiResult<ConfigValueVo> {
        let (config, _) = self
            .find_enabled_configs_by_keys(vec![config_key.to_string()])
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| {
                ApiErrors::NotFound(format!("系统参数不存在或未启用: {}", config_key))
            })?;

        Ok(ConfigValueVo::from(config))
    }

    pub async fn get_by_keys(
        &self,
        dto: ConfigKeysDto,
    ) -> ApiResult<HashMap<String, ConfigValueVo>> {
        let configs = self.find_enabled_configs_by_keys(dto.config_keys).await?;
        let mut result = HashMap::with_capacity(configs.len());
        for (config, _) in configs {
            result.insert(config.config_key.clone(), ConfigValueVo::from(config));
        }
        Ok(result)
    }

    pub async fn create(&self, dto: CreateConfigDto, operator: &str) -> ApiResult<()> {
        self.ensure_config_group_exists(dto.config_group_id).await?;
        self.ensure_config_key_unique(&dto.config_key, None).await?;

        let mut active: sys_config::ActiveModel = dto.into();
        active.create_by = Set(operator.to_string());
        active.update_by = Set(operator.to_string());
        active
            .insert(&self.db)
            .await
            .context("创建系统参数配置失败")?;
        Ok(())
    }

    pub async fn update(&self, id: i64, dto: UpdateConfigDto, operator: &str) -> ApiResult<()> {
        if let Some(config_group_id) = dto.config_group_id {
            self.ensure_config_group_exists(config_group_id).await?;
        }
        if let Some(config_key) = dto.config_key.as_deref() {
            self.ensure_config_key_unique(config_key, Some(id)).await?;
        }

        let mut active: sys_config::ActiveModel = self.find_model_by_id(id).await?.into();
        dto.apply_to(&mut active);
        active.update_by = Set(operator.to_string());
        active
            .update(&self.db)
            .await
            .context("更新系统参数配置失败")?;
        Ok(())
    }

    pub async fn delete(&self, id: i64) -> ApiResult<()> {
        let model = self.find_model_by_id(id).await?;
        if model.is_system {
            return Err(ApiErrors::BadRequest("系统内置配置不允许删除".to_string()));
        }

        let result = sys_config::Entity::delete_by_id(id)
            .exec(&self.db)
            .await
            .context("删除系统参数配置失败")?;

        if result.rows_affected == 0 {
            return Err(ApiErrors::NotFound("系统参数配置不存在".to_string()));
        }

        Ok(())
    }

    async fn find_model_by_id(&self, id: i64) -> ApiResult<sys_config::Model> {
        sys_config::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询系统参数配置详情失败")?
            .ok_or_else(|| ApiErrors::NotFound("系统参数配置不存在".to_string()))
    }

    async fn ensure_config_group_exists(&self, group_id: i64) -> ApiResult<()> {
        let exists = sys_config_group::Entity::find_by_id(group_id)
            .one(&self.db)
            .await
            .context("查询系统参数分组失败")?;

        if exists.is_none() {
            return Err(ApiErrors::BadRequest(format!(
                "系统参数分组不存在: {}",
                group_id
            )));
        }

        Ok(())
    }

    async fn find_enabled_configs_by_keys(
        &self,
        config_keys: Vec<String>,
    ) -> ApiResult<Vec<(sys_config::Model, sys_config_group::Model)>> {
        if config_keys.is_empty() {
            return Ok(Vec::new());
        }

        let configs = sys_config::Entity::find()
            .filter(sys_config::Column::ConfigKey.is_in(config_keys))
            .filter(sys_config::Column::Enabled.eq(true))
            .find_also_related(sys_config_group::Entity)
            .all(&self.db)
            .await
            .context("根据配置键查询系统参数失败")?;

        Ok(configs
            .into_iter()
            .filter_map(|(config, group)| {
                group
                    .filter(|group| group.enabled)
                    .map(|group| (config, group))
            })
            .collect())
    }

    async fn ensure_config_key_unique(
        &self,
        config_key: &str,
        exclude_id: Option<i64>,
    ) -> ApiResult<()> {
        let mut query =
            sys_config::Entity::find().filter(sys_config::Column::ConfigKey.eq(config_key));
        if let Some(exclude_id) = exclude_id {
            query = query.filter(sys_config::Column::Id.ne(exclude_id));
        }

        let existing = query
            .one(&self.db)
            .await
            .context("检查配置键是否重复失败")?;

        if existing.is_some() {
            return Err(ApiErrors::Conflict(format!("配置键已存在: {}", config_key)));
        }

        Ok(())
    }
}
