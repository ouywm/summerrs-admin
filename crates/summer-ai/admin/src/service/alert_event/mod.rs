use anyhow::Context;
use sea_orm::{ActiveModelTrait, EntityTrait, QueryFilter, QueryOrder, Set};
use summer::plugin::Service;
use summer_common::error::{ApiErrors, ApiResult};
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};

use crate::router::alert_event::req::AlertEventQuery;
use crate::router::alert_event::res::AlertEventRes;
use summer_ai_model::entity::alerts::alert_event::{self, AlertEventStatus};

#[derive(Clone, Service)]
pub struct AlertEventService {
    #[inject(component)]
    db: DbConn,
}

impl AlertEventService {
    pub async fn list_events(
        &self,
        query: AlertEventQuery,
        pagination: Pagination,
    ) -> ApiResult<Page<AlertEventRes>> {
        let page = alert_event::Entity::find()
            .filter(query)
            .order_by_desc(alert_event::Column::LastTriggeredAt)
            .order_by_desc(alert_event::Column::Id)
            .page(&self.db, &pagination)
            .await
            .context("查询告警事件失败")?;
        Ok(page.map(AlertEventRes::from_model))
    }

    pub async fn get_event(&self, id: i64) -> ApiResult<AlertEventRes> {
        let model = alert_event::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询告警事件失败")?
            .ok_or_else(|| ApiErrors::NotFound("告警事件不存在".to_string()))?;
        Ok(AlertEventRes::from_model(model))
    }

    pub async fn ack_event(&self, id: i64, operator: &str) -> ApiResult<AlertEventRes> {
        self.update_event_status(id, AlertEventStatus::Acknowledged, operator)
            .await
    }

    pub async fn resolve_event(&self, id: i64, operator: &str) -> ApiResult<AlertEventRes> {
        self.update_event_status(id, AlertEventStatus::Resolved, operator)
            .await
    }

    pub async fn ignore_event(&self, id: i64, operator: &str) -> ApiResult<AlertEventRes> {
        self.update_event_status(id, AlertEventStatus::Ignored, operator)
            .await
    }

    async fn update_event_status(
        &self,
        id: i64,
        status: AlertEventStatus,
        operator: &str,
    ) -> ApiResult<AlertEventRes> {
        let model = alert_event::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询告警事件失败")?
            .ok_or_else(|| ApiErrors::NotFound("告警事件不存在".to_string()))?;

        let mut active: alert_event::ActiveModel = model.into();
        let now = chrono::Utc::now().fixed_offset();
        active.status = Set(status);
        match status {
            AlertEventStatus::Acknowledged => {
                active.ack_by = Set(operator.to_string());
                active.ack_time = Set(Some(now));
            }
            AlertEventStatus::Resolved | AlertEventStatus::Ignored => {
                active.resolved_by = Set(operator.to_string());
                active.resolved_time = Set(Some(now));
            }
            AlertEventStatus::Open => {}
        }

        let updated = active.update(&self.db).await.context("更新告警事件失败")?;
        Ok(AlertEventRes::from_model(updated))
    }
}
