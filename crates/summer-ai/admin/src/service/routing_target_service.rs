use anyhow::Context;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder};
use summer::plugin::Service;
use summer_ai_model::dto::routing_target::{
    CreateRoutingTargetDto, NormalizedRoutingTargetBinding, RoutingTargetQueryDto,
    UpdateRoutingTargetDto,
};
use summer_ai_model::entity::platform::plugin;
use summer_ai_model::entity::routing::{channel, channel_account, routing_rule, routing_target};
use summer_ai_model::vo::routing_target::RoutingTargetVo;
use summer_common::error::{ApiErrors, ApiResult};
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};

#[derive(Clone, Service)]
pub struct RoutingTargetService {
    #[inject(component)]
    db: DbConn,
}

impl RoutingTargetService {
    pub async fn list(
        &self,
        query: RoutingTargetQueryDto,
        pagination: Pagination,
    ) -> ApiResult<Page<RoutingTargetVo>> {
        let page: Page<routing_target::Model> = routing_target::Entity::find()
            .filter(query)
            .order_by_asc(routing_target::Column::RoutingRuleId)
            .order_by_desc(routing_target::Column::Priority)
            .order_by_desc(routing_target::Column::Weight)
            .order_by_asc(routing_target::Column::Id)
            .page(&self.db, &pagination)
            .await
            .context("查询路由目标列表失败")?;

        Ok(page.map(RoutingTargetVo::from_model))
    }

    pub async fn detail(&self, id: i64) -> ApiResult<RoutingTargetVo> {
        let model = self.find_model_by_id(id).await?;
        Ok(RoutingTargetVo::from_model(model))
    }

    pub async fn create(&self, dto: CreateRoutingTargetDto) -> ApiResult<()> {
        dto.validate_business_rules()
            .map_err(ApiErrors::BadRequest)?;
        let binding = dto.normalized_binding().map_err(ApiErrors::BadRequest)?;
        self.ensure_routing_rule_exists(dto.routing_rule_id).await?;
        self.ensure_target_exists(&binding).await?;
        self.ensure_unique_binding(dto.routing_rule_id, &binding, None)
            .await?;

        dto.into_active_model()
            .map_err(ApiErrors::BadRequest)?
            .insert(&self.db)
            .await
            .context("创建路由目标失败")?;
        Ok(())
    }

    pub async fn update(&self, id: i64, dto: UpdateRoutingTargetDto) -> ApiResult<()> {
        let model = self.find_model_by_id(id).await?;
        dto.validate_business_rules(&model)
            .map_err(ApiErrors::BadRequest)?;
        let binding = dto.merged_binding(&model).map_err(ApiErrors::BadRequest)?;
        let routing_rule_id = dto.routing_rule_id.unwrap_or(model.routing_rule_id);
        self.ensure_routing_rule_exists(routing_rule_id).await?;
        self.ensure_target_exists(&binding).await?;
        self.ensure_unique_binding(routing_rule_id, &binding, Some(id))
            .await?;

        let mut active: routing_target::ActiveModel = model.into();
        dto.apply_to(&mut active).map_err(ApiErrors::BadRequest)?;
        active.update(&self.db).await.context("更新路由目标失败")?;
        Ok(())
    }

    pub async fn delete(&self, id: i64) -> ApiResult<()> {
        let _model = self.find_model_by_id(id).await?;
        routing_target::Entity::delete_by_id(id)
            .exec(&self.db)
            .await
            .context("删除路由目标失败")?;
        Ok(())
    }

    async fn ensure_routing_rule_exists(&self, id: i64) -> ApiResult<()> {
        let exists = routing_rule::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("检查路由规则是否存在失败")?;
        if exists.is_none() {
            return Err(ApiErrors::BadRequest(format!(
                "路由规则不存在: routing_rule_id={id}"
            )));
        }
        Ok(())
    }

    async fn ensure_target_exists(
        &self,
        binding: &NormalizedRoutingTargetBinding,
    ) -> ApiResult<()> {
        match binding.target_type.as_str() {
            "channel" => {
                let exists = channel::Entity::find_by_id(binding.channel_id)
                    .filter(channel::Column::DeletedAt.is_null())
                    .one(&self.db)
                    .await
                    .context("检查渠道目标是否存在失败")?;
                if exists.is_none() {
                    return Err(ApiErrors::BadRequest(format!(
                        "渠道不存在: channel_id={}",
                        binding.channel_id
                    )));
                }
            }
            "account" => {
                let exists = channel_account::Entity::find_by_id(binding.account_id)
                    .filter(channel_account::Column::DeletedAt.is_null())
                    .one(&self.db)
                    .await
                    .context("检查账号目标是否存在失败")?;
                if exists.is_none() {
                    return Err(ApiErrors::BadRequest(format!(
                        "渠道账号不存在: account_id={}",
                        binding.account_id
                    )));
                }
            }
            "plugin" => {
                let exists = plugin::Entity::find_by_id(binding.plugin_id)
                    .one(&self.db)
                    .await
                    .context("检查插件目标是否存在失败")?;
                if exists.is_none() {
                    return Err(ApiErrors::BadRequest(format!(
                        "插件不存在: plugin_id={}",
                        binding.plugin_id
                    )));
                }
            }
            "channel_group" | "pipeline" => {}
            _ => unreachable!(),
        }
        Ok(())
    }

    async fn ensure_unique_binding(
        &self,
        routing_rule_id: i64,
        binding: &NormalizedRoutingTargetBinding,
        exclude_id: Option<i64>,
    ) -> ApiResult<()> {
        let mut select = routing_target::Entity::find()
            .filter(routing_target::Column::RoutingRuleId.eq(routing_rule_id))
            .filter(routing_target::Column::TargetType.eq(binding.target_type.clone()))
            .filter(routing_target::Column::ChannelId.eq(binding.channel_id))
            .filter(routing_target::Column::AccountId.eq(binding.account_id))
            .filter(routing_target::Column::PluginId.eq(binding.plugin_id))
            .filter(routing_target::Column::TargetKey.eq(binding.target_key.clone()));
        if let Some(exclude_id) = exclude_id {
            select = select.filter(routing_target::Column::Id.ne(exclude_id));
        }
        let exists = select
            .one(&self.db)
            .await
            .context("检查路由目标唯一性失败")?;
        if exists.is_some() {
            return Err(ApiErrors::Conflict(format!(
                "路由目标已存在: routing_rule_id={routing_rule_id}, target_type={}, channel_id={}, account_id={}, plugin_id={}, target_key={}",
                binding.target_type,
                binding.channel_id,
                binding.account_id,
                binding.plugin_id,
                binding.target_key
            )));
        }
        Ok(())
    }

    async fn find_model_by_id(&self, id: i64) -> ApiResult<routing_target::Model> {
        routing_target::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询路由目标详情失败")?
            .ok_or_else(|| ApiErrors::NotFound(format!("路由目标不存在: id={id}")))
    }
}
