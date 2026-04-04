use anyhow::Context;
use sea_orm::{ActiveModelTrait, EntityTrait, QueryFilter, QueryOrder};
use summer::plugin::Service;
use summer_common::error::{ApiErrors, ApiResult};
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};

use summer_ai_model::dto::guardrail::{
    CreateGuardrailConfigDto, CreateGuardrailRuleDto, CreatePromptProtectionRuleDto,
    QueryGuardrailRuleDto, QueryGuardrailViolationDto, UpdateGuardrailConfigDto,
    UpdateGuardrailRuleDto, UpdatePromptProtectionRuleDto,
};
use summer_ai_model::entity::guardrail_config;
use summer_ai_model::entity::guardrail_rule;
use summer_ai_model::entity::guardrail_violation;
use summer_ai_model::entity::prompt_protection_rule;
use summer_ai_model::vo::guardrail::{
    GuardrailConfigVo, GuardrailRuleVo, GuardrailViolationVo, PromptProtectionRuleVo,
};

#[derive(Clone, Service)]
pub struct GuardrailService {
    #[inject(component)]
    db: DbConn,
}

impl GuardrailService {
    // ─── Config CRUD ───

    pub async fn list_configs(&self) -> ApiResult<Vec<GuardrailConfigVo>> {
        let configs = guardrail_config::Entity::find()
            .order_by_asc(guardrail_config::Column::Id)
            .all(&self.db)
            .await
            .context("查询 Guardrail 配置失败")
            .map_err(ApiErrors::Internal)?;
        Ok(configs
            .into_iter()
            .map(GuardrailConfigVo::from_model)
            .collect())
    }

    pub async fn get_config(&self, id: i64) -> ApiResult<GuardrailConfigVo> {
        let model = guardrail_config::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询配置失败")
            .map_err(ApiErrors::Internal)?
            .ok_or_else(|| ApiErrors::NotFound("配置不存在".to_string()))?;
        Ok(GuardrailConfigVo::from_model(model))
    }

    pub async fn create_config(
        &self,
        dto: CreateGuardrailConfigDto,
        operator: &str,
    ) -> ApiResult<GuardrailConfigVo> {
        let model = dto
            .into_active_model(operator)
            .insert(&self.db)
            .await
            .context("创建 Guardrail 配置失败")
            .map_err(ApiErrors::Internal)?;
        Ok(GuardrailConfigVo::from_model(model))
    }

    pub async fn update_config(
        &self,
        id: i64,
        dto: UpdateGuardrailConfigDto,
        operator: &str,
    ) -> ApiResult<GuardrailConfigVo> {
        let model = guardrail_config::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询配置失败")
            .map_err(ApiErrors::Internal)?
            .ok_or_else(|| ApiErrors::NotFound("配置不存在".to_string()))?;
        let mut active: guardrail_config::ActiveModel = model.into();
        dto.apply_to(&mut active, operator);
        let updated = active
            .update(&self.db)
            .await
            .context("更新配置失败")
            .map_err(ApiErrors::Internal)?;
        Ok(GuardrailConfigVo::from_model(updated))
    }

    pub async fn delete_config(&self, id: i64) -> ApiResult<()> {
        guardrail_config::Entity::delete_by_id(id)
            .exec(&self.db)
            .await
            .context("删除配置失败")
            .map_err(ApiErrors::Internal)?;
        Ok(())
    }

    // ─── Rule CRUD ───

    pub async fn list_rules(
        &self,
        query: QueryGuardrailRuleDto,
        pagination: Pagination,
    ) -> ApiResult<Page<GuardrailRuleVo>> {
        let page = guardrail_rule::Entity::find()
            .filter(query)
            .order_by_desc(guardrail_rule::Column::Priority)
            .order_by_desc(guardrail_rule::Column::Id)
            .page(&self.db, &pagination)
            .await
            .context("查询 Guardrail 规则失败")?;
        Ok(page.map(GuardrailRuleVo::from_model))
    }

    pub async fn get_rule(&self, id: i64) -> ApiResult<GuardrailRuleVo> {
        let model = guardrail_rule::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询规则失败")
            .map_err(ApiErrors::Internal)?
            .ok_or_else(|| ApiErrors::NotFound("规则不存在".to_string()))?;
        Ok(GuardrailRuleVo::from_model(model))
    }

    pub async fn create_rule(
        &self,
        dto: CreateGuardrailRuleDto,
        operator: &str,
    ) -> ApiResult<GuardrailRuleVo> {
        let model = dto
            .into_active_model(operator)
            .insert(&self.db)
            .await
            .context("创建规则失败")
            .map_err(ApiErrors::Internal)?;
        Ok(GuardrailRuleVo::from_model(model))
    }

    pub async fn update_rule(
        &self,
        id: i64,
        dto: UpdateGuardrailRuleDto,
        operator: &str,
    ) -> ApiResult<GuardrailRuleVo> {
        let model = guardrail_rule::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询规则失败")
            .map_err(ApiErrors::Internal)?
            .ok_or_else(|| ApiErrors::NotFound("规则不存在".to_string()))?;
        let mut active: guardrail_rule::ActiveModel = model.into();
        dto.apply_to(&mut active, operator);
        let updated = active
            .update(&self.db)
            .await
            .context("更新规则失败")
            .map_err(ApiErrors::Internal)?;
        Ok(GuardrailRuleVo::from_model(updated))
    }

    pub async fn delete_rule(&self, id: i64) -> ApiResult<()> {
        guardrail_rule::Entity::delete_by_id(id)
            .exec(&self.db)
            .await
            .context("删除规则失败")
            .map_err(ApiErrors::Internal)?;
        Ok(())
    }

    // ─── Violation 查询 ───

    pub async fn list_violations(
        &self,
        query: QueryGuardrailViolationDto,
        pagination: Pagination,
    ) -> ApiResult<Page<GuardrailViolationVo>> {
        let page = guardrail_violation::Entity::find()
            .filter(query)
            .order_by_desc(guardrail_violation::Column::CreateTime)
            .page(&self.db, &pagination)
            .await
            .context("查询违规记录失败")?;
        Ok(page.map(GuardrailViolationVo::from_model))
    }

    // ─── PromptProtection CRUD ───

    pub async fn list_prompt_rules(
        &self,
        pagination: Pagination,
    ) -> ApiResult<Page<PromptProtectionRuleVo>> {
        let page = prompt_protection_rule::Entity::find()
            .order_by_desc(prompt_protection_rule::Column::Priority)
            .page(&self.db, &pagination)
            .await
            .context("查询 Prompt 防护规则失败")?;
        Ok(page.map(PromptProtectionRuleVo::from_model))
    }

    pub async fn create_prompt_rule(
        &self,
        dto: CreatePromptProtectionRuleDto,
        operator: &str,
    ) -> ApiResult<PromptProtectionRuleVo> {
        let model = dto
            .into_active_model(operator)
            .insert(&self.db)
            .await
            .context("创建 Prompt 防护规则失败")
            .map_err(ApiErrors::Internal)?;
        Ok(PromptProtectionRuleVo::from_model(model))
    }

    pub async fn update_prompt_rule(
        &self,
        id: i64,
        dto: UpdatePromptProtectionRuleDto,
        operator: &str,
    ) -> ApiResult<PromptProtectionRuleVo> {
        let model = prompt_protection_rule::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询规则失败")
            .map_err(ApiErrors::Internal)?
            .ok_or_else(|| ApiErrors::NotFound("规则不存在".to_string()))?;
        let mut active: prompt_protection_rule::ActiveModel = model.into();
        dto.apply_to(&mut active, operator);
        let updated = active
            .update(&self.db)
            .await
            .context("更新规则失败")
            .map_err(ApiErrors::Internal)?;
        Ok(PromptProtectionRuleVo::from_model(updated))
    }

    pub async fn delete_prompt_rule(&self, id: i64) -> ApiResult<()> {
        prompt_protection_rule::Entity::delete_by_id(id)
            .exec(&self.db)
            .await
            .context("删除规则失败")
            .map_err(ApiErrors::Internal)?;
        Ok(())
    }
}
