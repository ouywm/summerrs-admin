use anyhow::Context;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set};
use summer::plugin::Service;
use summer_common::error::{ApiErrors, ApiResult};
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};

use summer_ai_model::dto::alert::{
    CreateAlertRuleDto, CreateAlertSilenceDto, QueryAlertEventDto, QueryAlertRuleDto,
    QueryDailyStatsDto, UpdateAlertRuleDto,
};
use summer_ai_model::entity::alert_event::{self, AlertEventStatus};
use summer_ai_model::entity::alert_rule;
use summer_ai_model::entity::alert_silence::{self, SilenceStatus};
use summer_ai_model::entity::daily_stats;
use summer_ai_model::vo::alert::{AlertEventVo, AlertRuleVo, AlertSilenceVo, DailyStatsVo};

#[derive(Clone, Service)]
pub struct AlertService {
    #[inject(component)]
    db: DbConn,
}

impl AlertService {
    // ─── 告警规则 CRUD ───

    pub async fn list_rules(
        &self,
        query: QueryAlertRuleDto,
        pagination: Pagination,
    ) -> ApiResult<Page<AlertRuleVo>> {
        let page = alert_rule::Entity::find()
            .filter(query)
            .order_by_desc(alert_rule::Column::UpdateTime)
            .page(&self.db, &pagination)
            .await
            .context("查询告警规则失败")?;
        Ok(page.map(AlertRuleVo::from_model))
    }

    pub async fn get_rule(&self, id: i64) -> ApiResult<AlertRuleVo> {
        let model = alert_rule::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询告警规则失败")
            .map_err(ApiErrors::Internal)?
            .ok_or_else(|| ApiErrors::NotFound("告警规则不存在".to_string()))?;
        Ok(AlertRuleVo::from_model(model))
    }

    pub async fn create_rule(
        &self,
        dto: CreateAlertRuleDto,
        operator: &str,
    ) -> ApiResult<AlertRuleVo> {
        let model = dto
            .into_active_model(operator)
            .insert(&self.db)
            .await
            .context("创建告警规则失败")
            .map_err(ApiErrors::Internal)?;
        Ok(AlertRuleVo::from_model(model))
    }

    pub async fn update_rule(
        &self,
        id: i64,
        dto: UpdateAlertRuleDto,
        operator: &str,
    ) -> ApiResult<AlertRuleVo> {
        let model = alert_rule::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询告警规则失败")
            .map_err(ApiErrors::Internal)?
            .ok_or_else(|| ApiErrors::NotFound("告警规则不存在".to_string()))?;

        let mut active: alert_rule::ActiveModel = model.into();
        dto.apply_to(&mut active, operator);

        let updated = active
            .update(&self.db)
            .await
            .context("更新告警规则失败")
            .map_err(ApiErrors::Internal)?;
        Ok(AlertRuleVo::from_model(updated))
    }

    pub async fn delete_rule(&self, id: i64) -> ApiResult<()> {
        alert_rule::Entity::delete_by_id(id)
            .exec(&self.db)
            .await
            .context("删除告警规则失败")
            .map_err(ApiErrors::Internal)?;
        Ok(())
    }

    // ─── 告警事件 ───

    pub async fn list_events(
        &self,
        query: QueryAlertEventDto,
        pagination: Pagination,
    ) -> ApiResult<Page<AlertEventVo>> {
        let page = alert_event::Entity::find()
            .filter(query)
            .order_by_desc(alert_event::Column::LastTriggeredAt)
            .page(&self.db, &pagination)
            .await
            .context("查询告警事件失败")?;
        Ok(page.map(AlertEventVo::from_model))
    }

    pub async fn ack_event(&self, id: i64, operator: &str) -> ApiResult<AlertEventVo> {
        let model = alert_event::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询告警事件失败")
            .map_err(ApiErrors::Internal)?
            .ok_or_else(|| ApiErrors::NotFound("告警事件不存在".to_string()))?;

        let mut active: alert_event::ActiveModel = model.into();
        active.status = Set(AlertEventStatus::Acknowledged);
        active.ack_by = Set(operator.to_string());
        active.ack_time = Set(Some(chrono::Utc::now().fixed_offset()));

        let updated = active
            .update(&self.db)
            .await
            .context("确认告警事件失败")
            .map_err(ApiErrors::Internal)?;
        Ok(AlertEventVo::from_model(updated))
    }

    pub async fn resolve_event(&self, id: i64, operator: &str) -> ApiResult<AlertEventVo> {
        let model = alert_event::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询告警事件失败")
            .map_err(ApiErrors::Internal)?
            .ok_or_else(|| ApiErrors::NotFound("告警事件不存在".to_string()))?;

        let mut active: alert_event::ActiveModel = model.into();
        active.status = Set(AlertEventStatus::Resolved);
        active.resolved_by = Set(operator.to_string());
        active.resolved_time = Set(Some(chrono::Utc::now().fixed_offset()));

        let updated = active
            .update(&self.db)
            .await
            .context("解决告警事件失败")
            .map_err(ApiErrors::Internal)?;
        Ok(AlertEventVo::from_model(updated))
    }

    // ─── 告警静默 ───

    pub async fn list_silences(&self, rule_id: Option<i64>) -> ApiResult<Vec<AlertSilenceVo>> {
        let mut query = alert_silence::Entity::find()
            .filter(alert_silence::Column::Status.eq(SilenceStatus::Active));
        if let Some(rid) = rule_id {
            query = query.filter(alert_silence::Column::AlertRuleId.eq(rid));
        }
        let silences = query
            .order_by_desc(alert_silence::Column::EndTime)
            .all(&self.db)
            .await
            .context("查询告警静默失败")
            .map_err(ApiErrors::Internal)?;
        Ok(silences
            .into_iter()
            .map(AlertSilenceVo::from_model)
            .collect())
    }

    pub async fn create_silence(
        &self,
        dto: CreateAlertSilenceDto,
        operator: &str,
    ) -> ApiResult<AlertSilenceVo> {
        let now = chrono::Utc::now().fixed_offset();
        let active = alert_silence::ActiveModel {
            alert_rule_id: Set(dto.alert_rule_id),
            scope_type: Set(dto.scope_type),
            scope_key: Set(dto.scope_key),
            reason: Set(dto.reason),
            status: Set(SilenceStatus::Active),
            metadata: Set(serde_json::json!({})),
            create_by: Set(operator.to_string()),
            start_time: Set(now),
            end_time: Set(dto.end_time),
            create_time: Set(now),
            ..Default::default()
        };
        let model = active
            .insert(&self.db)
            .await
            .context("创建告警静默失败")
            .map_err(ApiErrors::Internal)?;
        Ok(AlertSilenceVo::from_model(model))
    }

    pub async fn delete_silence(&self, id: i64) -> ApiResult<()> {
        let model = alert_silence::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询告警静默失败")
            .map_err(ApiErrors::Internal)?
            .ok_or_else(|| ApiErrors::NotFound("告警静默不存在".to_string()))?;

        let mut active: alert_silence::ActiveModel = model.into();
        active.status = Set(SilenceStatus::Ended);
        active
            .update(&self.db)
            .await
            .context("结束告警静默失败")
            .map_err(ApiErrors::Internal)?;
        Ok(())
    }

    // ─── 日度统计 ───

    pub async fn list_daily_stats(
        &self,
        query: QueryDailyStatsDto,
        pagination: Pagination,
    ) -> ApiResult<Page<DailyStatsVo>> {
        let page = daily_stats::Entity::find()
            .filter(query)
            .order_by_desc(daily_stats::Column::StatsDate)
            .page(&self.db, &pagination)
            .await
            .context("查询日度统计失败")?;
        Ok(page.map(DailyStatsVo::from_model))
    }
}
