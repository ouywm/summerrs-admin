use anyhow::Context;
use sea_orm::{EntityTrait, QueryFilter, QueryOrder};
use summer::plugin::Service;
use summer_common::error::ApiResult;
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};

use crate::router::daily_stats::req::DailyStatsQuery;
use crate::router::daily_stats::res::DailyStatsRes;
use summer_ai_model::entity::alerts::daily_stats;

#[derive(Clone, Service)]
pub struct DailyStatsAdminService {
    #[inject(component)]
    db: DbConn,
}

impl DailyStatsAdminService {
    pub async fn list_daily_stats(
        &self,
        query: DailyStatsQuery,
        pagination: Pagination,
    ) -> ApiResult<Page<DailyStatsRes>> {
        let page = daily_stats::Entity::find()
            .filter(query)
            .order_by_desc(daily_stats::Column::StatsDate)
            .order_by_desc(daily_stats::Column::Id)
            .page(&self.db, &pagination)
            .await
            .context("查询日度统计失败")?;

        Ok(page.map(DailyStatsRes::from_model))
    }
}
