use anyhow::Context;
use sea_orm::{ActiveModelTrait, EntityTrait, QueryFilter, QueryOrder};
use summer::plugin::Service;
use summer_common::error::{ApiErrors, ApiResult};
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};

use crate::router::alert_rule::req::{AlertRuleQuery, CreateAlertRuleReq, UpdateAlertRuleReq};
use crate::router::alert_rule::res::AlertRuleRes;
use summer_ai_model::entity::alerts::alert_rule;

#[derive(Clone, Service)]
pub struct AlertRuleService {
    #[inject(component)]
    db: DbConn,
}

impl AlertRuleService {
    pub async fn list_rules(
        &self,
        query: AlertRuleQuery,
        pagination: Pagination,
    ) -> ApiResult<Page<AlertRuleRes>> {
        let page = alert_rule::Entity::find()
            .filter(query)
            .order_by_desc(alert_rule::Column::UpdateTime)
            .order_by_desc(alert_rule::Column::Id)
            .page(&self.db, &pagination)
            .await
            .context("查询告警规则失败")?;
        Ok(page.map(AlertRuleRes::from_model))
    }

    pub async fn get_rule(&self, id: i64) -> ApiResult<AlertRuleRes> {
        let model = alert_rule::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询告警规则失败")?
            .ok_or_else(|| ApiErrors::NotFound("告警规则不存在".to_string()))?;
        Ok(AlertRuleRes::from_model(model))
    }

    pub async fn create_rule(
        &self,
        req: CreateAlertRuleReq,
        operator: &str,
    ) -> ApiResult<AlertRuleRes> {
        let model = req
            .into_active_model(operator)
            .insert(&self.db)
            .await
            .context("创建告警规则失败")?;
        Ok(AlertRuleRes::from_model(model))
    }

    pub async fn update_rule(
        &self,
        id: i64,
        req: UpdateAlertRuleReq,
        operator: &str,
    ) -> ApiResult<AlertRuleRes> {
        let model = alert_rule::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询告警规则失败")?
            .ok_or_else(|| ApiErrors::NotFound("告警规则不存在".to_string()))?;

        let mut active: alert_rule::ActiveModel = model.into();
        req.apply_to(&mut active, operator);
        let updated = active.update(&self.db).await.context("更新告警规则失败")?;
        Ok(AlertRuleRes::from_model(updated))
    }

    pub async fn delete_rule(&self, id: i64) -> ApiResult<()> {
        alert_rule::Entity::delete_by_id(id)
            .exec(&self.db)
            .await
            .context("删除告警规则失败")?;
        Ok(())
    }
}
