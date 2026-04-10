use anyhow::Context;
use sea_orm::{ActiveModelTrait, EntityTrait, QueryFilter, QueryOrder};
use summer::plugin::Service;
use summer_common::error::{ApiErrors, ApiResult};
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};

use crate::router::guardrail::req::{
    CreateGuardrailConfigReq, CreateGuardrailRuleReq, CreatePromptProtectionRuleReq,
    GuardrailRuleQuery, GuardrailViolationQuery, UpdateGuardrailConfigReq, UpdateGuardrailRuleReq,
    UpdatePromptProtectionRuleReq,
};
use crate::router::guardrail::res::{
    GuardrailConfigRes, GuardrailMetricDailyRes, GuardrailRuleRes, GuardrailViolationRes,
    PromptProtectionRuleRes,
};
use summer_ai_model::entity::guardrail_config;
use summer_ai_model::entity::guardrail_metric_daily;
use summer_ai_model::entity::guardrail_rule;
use summer_ai_model::entity::guardrail_violation;
use summer_ai_model::entity::prompt_protection_rule;

#[derive(Clone, Service)]
pub struct GuardrailService {
    #[inject(component)]
    db: DbConn,
}

impl GuardrailService {
    pub async fn list_configs(&self) -> ApiResult<Vec<GuardrailConfigRes>> {
        let items = guardrail_config::Entity::find()
            .order_by_asc(guardrail_config::Column::Id)
            .all(&self.db)
            .await
            .context("查询 Guardrail 配置失败")?;

        Ok(items
            .into_iter()
            .map(GuardrailConfigRes::from_model)
            .collect())
    }

    pub async fn get_config(&self, id: i64) -> ApiResult<GuardrailConfigRes> {
        let model = self.find_config_model(id).await?;
        Ok(GuardrailConfigRes::from_model(model))
    }

    pub async fn create_config(
        &self,
        req: CreateGuardrailConfigReq,
        operator: &str,
    ) -> ApiResult<GuardrailConfigRes> {
        let model = req
            .into_active_model(operator)
            .insert(&self.db)
            .await
            .context("创建 Guardrail 配置失败")?;

        Ok(GuardrailConfigRes::from_model(model))
    }

    pub async fn update_config(
        &self,
        id: i64,
        req: UpdateGuardrailConfigReq,
        operator: &str,
    ) -> ApiResult<GuardrailConfigRes> {
        let mut active: guardrail_config::ActiveModel = self.find_config_model(id).await?.into();
        req.apply_to(&mut active, operator);
        let model = active
            .update(&self.db)
            .await
            .context("更新 Guardrail 配置失败")?;
        Ok(GuardrailConfigRes::from_model(model))
    }

    pub async fn delete_config(&self, id: i64) -> ApiResult<()> {
        let result = guardrail_config::Entity::delete_by_id(id)
            .exec(&self.db)
            .await
            .context("删除 Guardrail 配置失败")?;

        if result.rows_affected == 0 {
            return Err(ApiErrors::NotFound("Guardrail 配置不存在".to_string()));
        }

        Ok(())
    }

    pub async fn list_rules(
        &self,
        query: GuardrailRuleQuery,
        pagination: Pagination,
    ) -> ApiResult<Page<GuardrailRuleRes>> {
        let page = guardrail_rule::Entity::find()
            .filter(query)
            .order_by_desc(guardrail_rule::Column::Priority)
            .order_by_desc(guardrail_rule::Column::Id)
            .page(&self.db, &pagination)
            .await
            .context("查询 Guardrail 规则失败")?;

        Ok(page.map(GuardrailRuleRes::from_model))
    }

    pub async fn get_rule(&self, id: i64) -> ApiResult<GuardrailRuleRes> {
        let model = self.find_rule_model(id).await?;
        Ok(GuardrailRuleRes::from_model(model))
    }

    pub async fn create_rule(
        &self,
        req: CreateGuardrailRuleReq,
        operator: &str,
    ) -> ApiResult<GuardrailRuleRes> {
        let model = req
            .into_active_model(operator)
            .insert(&self.db)
            .await
            .context("创建 Guardrail 规则失败")?;

        Ok(GuardrailRuleRes::from_model(model))
    }

    pub async fn update_rule(
        &self,
        id: i64,
        req: UpdateGuardrailRuleReq,
        operator: &str,
    ) -> ApiResult<GuardrailRuleRes> {
        let mut active: guardrail_rule::ActiveModel = self.find_rule_model(id).await?.into();
        req.apply_to(&mut active, operator);
        let model = active
            .update(&self.db)
            .await
            .context("更新 Guardrail 规则失败")?;
        Ok(GuardrailRuleRes::from_model(model))
    }

    pub async fn delete_rule(&self, id: i64) -> ApiResult<()> {
        let result = guardrail_rule::Entity::delete_by_id(id)
            .exec(&self.db)
            .await
            .context("删除 Guardrail 规则失败")?;

        if result.rows_affected == 0 {
            return Err(ApiErrors::NotFound("Guardrail 规则不存在".to_string()));
        }

        Ok(())
    }

    pub async fn list_violations(
        &self,
        query: GuardrailViolationQuery,
        pagination: Pagination,
    ) -> ApiResult<Page<GuardrailViolationRes>> {
        let page = guardrail_violation::Entity::find()
            .filter(query)
            .order_by_desc(guardrail_violation::Column::CreateTime)
            .order_by_desc(guardrail_violation::Column::Id)
            .page(&self.db, &pagination)
            .await
            .context("查询 Guardrail 命中记录失败")?;

        Ok(page.map(GuardrailViolationRes::from_model))
    }

    pub async fn list_prompt_rules(
        &self,
        pagination: Pagination,
    ) -> ApiResult<Page<PromptProtectionRuleRes>> {
        let page = prompt_protection_rule::Entity::find()
            .order_by_desc(prompt_protection_rule::Column::Priority)
            .order_by_desc(prompt_protection_rule::Column::Id)
            .page(&self.db, &pagination)
            .await
            .context("查询 Prompt 防护规则失败")?;

        Ok(page.map(PromptProtectionRuleRes::from_model))
    }

    pub async fn create_prompt_rule(
        &self,
        req: CreatePromptProtectionRuleReq,
        operator: &str,
    ) -> ApiResult<PromptProtectionRuleRes> {
        let model = req
            .into_active_model(operator)
            .insert(&self.db)
            .await
            .context("创建 Prompt 防护规则失败")?;

        Ok(PromptProtectionRuleRes::from_model(model))
    }

    pub async fn update_prompt_rule(
        &self,
        id: i64,
        req: UpdatePromptProtectionRuleReq,
        operator: &str,
    ) -> ApiResult<PromptProtectionRuleRes> {
        let mut active: prompt_protection_rule::ActiveModel =
            self.find_prompt_rule_model(id).await?.into();
        req.apply_to(&mut active, operator);
        let model = active
            .update(&self.db)
            .await
            .context("更新 Prompt 防护规则失败")?;

        Ok(PromptProtectionRuleRes::from_model(model))
    }

    pub async fn delete_prompt_rule(&self, id: i64) -> ApiResult<()> {
        let result = prompt_protection_rule::Entity::delete_by_id(id)
            .exec(&self.db)
            .await
            .context("删除 Prompt 防护规则失败")?;

        if result.rows_affected == 0 {
            return Err(ApiErrors::NotFound("Prompt 防护规则不存在".to_string()));
        }

        Ok(())
    }

    pub async fn list_metric_daily(
        &self,
        pagination: Pagination,
    ) -> ApiResult<Page<GuardrailMetricDailyRes>> {
        let page = guardrail_metric_daily::Entity::find()
            .order_by_desc(guardrail_metric_daily::Column::StatsDate)
            .order_by_desc(guardrail_metric_daily::Column::Id)
            .page(&self.db, &pagination)
            .await
            .context("查询 Guardrail 日统计失败")?;

        Ok(page.map(GuardrailMetricDailyRes::from_model))
    }

    async fn find_config_model(&self, id: i64) -> ApiResult<guardrail_config::Model> {
        guardrail_config::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询 Guardrail 配置详情失败")?
            .ok_or_else(|| ApiErrors::NotFound("Guardrail 配置不存在".to_string()))
    }

    async fn find_rule_model(&self, id: i64) -> ApiResult<guardrail_rule::Model> {
        guardrail_rule::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询 Guardrail 规则详情失败")?
            .ok_or_else(|| ApiErrors::NotFound("Guardrail 规则不存在".to_string()))
    }

    async fn find_prompt_rule_model(&self, id: i64) -> ApiResult<prompt_protection_rule::Model> {
        prompt_protection_rule::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询 Prompt 防护规则详情失败")?
            .ok_or_else(|| ApiErrors::NotFound("Prompt 防护规则不存在".to_string()))
    }
}
