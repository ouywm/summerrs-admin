use anyhow::Context;
use sea_orm::{ActiveModelTrait, EntityTrait, QueryFilter, QueryOrder, Set};
use summer::plugin::Service;
use summer_common::error::{ApiErrors, ApiResult};
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};

use crate::router::alert_silence::req::{AlertSilenceQuery, CreateAlertSilenceReq};
use crate::router::alert_silence::res::AlertSilenceRes;
use summer_ai_model::entity::alerts::alert_silence::{self, AlertSilenceStatus};

#[derive(Clone, Service)]
pub struct AlertSilenceService {
    #[inject(component)]
    db: DbConn,
}

impl AlertSilenceService {
    pub async fn list_silences(
        &self,
        query: AlertSilenceQuery,
        pagination: Pagination,
    ) -> ApiResult<Page<AlertSilenceRes>> {
        let page = alert_silence::Entity::find()
            .filter(query)
            .order_by_desc(alert_silence::Column::EndTime)
            .order_by_desc(alert_silence::Column::Id)
            .page(&self.db, &pagination)
            .await
            .context("查询告警静默失败")?;
        Ok(page.map(AlertSilenceRes::from_model))
    }

    pub async fn create_silence(
        &self,
        req: CreateAlertSilenceReq,
        operator: &str,
    ) -> ApiResult<AlertSilenceRes> {
        let model = req
            .into_active_model(operator)
            .insert(&self.db)
            .await
            .context("创建告警静默失败")?;
        Ok(AlertSilenceRes::from_model(model))
    }

    pub async fn delete_silence(&self, id: i64) -> ApiResult<()> {
        let model = alert_silence::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询告警静默失败")?
            .ok_or_else(|| ApiErrors::NotFound("告警静默不存在".to_string()))?;

        let mut active: alert_silence::ActiveModel = model.into();
        active.status = Set(AlertSilenceStatus::Ended);
        active.update(&self.db).await.context("结束告警静默失败")?;
        Ok(())
    }
}
