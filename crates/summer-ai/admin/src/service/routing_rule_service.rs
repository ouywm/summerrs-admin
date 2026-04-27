use anyhow::Context;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
};
use summer::plugin::Service;
use summer_ai_model::dto::routing_rule::{
    CreateRoutingRuleDto, RoutingRuleQueryDto, UpdateRoutingRuleDto,
};
use summer_ai_model::entity::routing::routing_rule::{self};
use summer_ai_model::entity::routing::routing_target;
use summer_ai_model::vo::routing_rule::RoutingRuleVo;
use summer_common::error::{ApiErrors, ApiResult};
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};

#[derive(Clone, Service)]
pub struct RoutingRuleService {
    #[inject(component)]
    db: DbConn,
}

impl RoutingRuleService {
    pub async fn list(
        &self,
        query: RoutingRuleQueryDto,
        pagination: Pagination,
    ) -> ApiResult<Page<RoutingRuleVo>> {
        let page: Page<routing_rule::Model> = routing_rule::Entity::find()
            .filter(query)
            .order_by_asc(routing_rule::Column::OrganizationId)
            .order_by_asc(routing_rule::Column::ProjectId)
            .order_by_desc(routing_rule::Column::Priority)
            .order_by_asc(routing_rule::Column::Id)
            .page(&self.db, &pagination)
            .await
            .context("查询路由规则列表失败")?;

        Ok(page.map(RoutingRuleVo::from_model))
    }

    pub async fn detail(&self, id: i64) -> ApiResult<RoutingRuleVo> {
        let model = self.find_model_by_id(id).await?;
        Ok(RoutingRuleVo::from_model(model))
    }

    pub async fn create(&self, dto: CreateRoutingRuleDto, operator: &str) -> ApiResult<()> {
        dto.validate_business_rules()
            .map_err(ApiErrors::BadRequest)?;
        self.ensure_unique_rule_key(dto.organization_id, dto.project_id, &dto.rule_code, None)
            .await?;
        dto.into_active_model(operator)
            .map_err(ApiErrors::BadRequest)?
            .insert(&self.db)
            .await
            .context("创建路由规则失败")?;
        Ok(())
    }

    pub async fn update(
        &self,
        id: i64,
        dto: UpdateRoutingRuleDto,
        operator: &str,
    ) -> ApiResult<()> {
        let model = self.find_model_by_id(id).await?;
        dto.validate_business_rules(&model)
            .map_err(ApiErrors::BadRequest)?;

        let next_organization_id = dto.organization_id.unwrap_or(model.organization_id);
        let next_project_id = dto.project_id.unwrap_or(model.project_id);
        let next_rule_code = dto.rule_code.as_deref().unwrap_or(&model.rule_code);
        self.ensure_unique_rule_key(
            next_organization_id,
            next_project_id,
            next_rule_code,
            Some(id),
        )
        .await?;

        let mut active: routing_rule::ActiveModel = model.into();
        dto.apply_to(&mut active, operator)
            .map_err(ApiErrors::BadRequest)?;
        active.update(&self.db).await.context("更新路由规则失败")?;
        Ok(())
    }

    pub async fn delete(&self, id: i64) -> ApiResult<()> {
        let _model = self.find_model_by_id(id).await?;
        let target_refs = routing_target::Entity::find()
            .filter(routing_target::Column::RoutingRuleId.eq(id))
            .count(&self.db)
            .await
            .context("检查路由目标引用失败")?;
        ensure_no_routing_rule_targets(target_refs).map_err(ApiErrors::Conflict)?;

        routing_rule::Entity::delete_by_id(id)
            .exec(&self.db)
            .await
            .context("删除路由规则失败")?;
        Ok(())
    }

    async fn ensure_unique_rule_key(
        &self,
        organization_id: i64,
        project_id: i64,
        rule_code: &str,
        exclude_id: Option<i64>,
    ) -> ApiResult<()> {
        let mut select = routing_rule::Entity::find()
            .filter(routing_rule::Column::OrganizationId.eq(organization_id))
            .filter(routing_rule::Column::ProjectId.eq(project_id))
            .filter(routing_rule::Column::RuleCode.eq(rule_code));
        if let Some(exclude_id) = exclude_id {
            select = select.filter(routing_rule::Column::Id.ne(exclude_id));
        }
        let exists = select
            .one(&self.db)
            .await
            .context("检查路由规则唯一性失败")?;
        if exists.is_some() {
            return Err(ApiErrors::Conflict(format!(
                "路由规则编码已存在: organization_id={organization_id}, project_id={project_id}, rule_code={rule_code}"
            )));
        }
        Ok(())
    }

    async fn find_model_by_id(&self, id: i64) -> ApiResult<routing_rule::Model> {
        routing_rule::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询路由规则详情失败")?
            .ok_or_else(|| ApiErrors::NotFound(format!("路由规则不存在: id={id}")))
    }
}

pub fn ensure_no_routing_rule_targets(target_refs: u64) -> Result<(), String> {
    if target_refs == 0 {
        return Ok(());
    }
    Err(format!(
        "路由规则仍被引用，不能删除: 路由目标={target_refs}"
    ))
}
