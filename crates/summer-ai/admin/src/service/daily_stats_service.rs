use anyhow::Context;
use sea_orm::{ColumnTrait, Condition, EntityTrait, QueryFilter, QueryOrder};
use summer::plugin::Service;
use summer_ai_model::dto::daily_stats::{DailyStatsQueryDto, DashboardQueryDto};
use summer_ai_model::entity::operations::daily_stats;
use summer_ai_model::vo::daily_stats::{
    DailyStatsDimensionVo, DailyStatsSummaryVo, DailyStatsVo, DashboardOverviewVo,
};
use summer_common::error::ApiResult;
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};

#[derive(Clone, Service)]
pub struct DailyStatsService {
    #[inject(component)]
    db: DbConn,
}

impl DailyStatsService {
    pub async fn list(
        &self,
        query: DailyStatsQueryDto,
        pagination: Pagination,
    ) -> ApiResult<Page<DailyStatsVo>> {
        let page: Page<daily_stats::Model> = daily_stats::Entity::find()
            .filter(query)
            .order_by_desc(daily_stats::Column::StatsDate)
            .order_by_desc(daily_stats::Column::Id)
            .page(&self.db, &pagination)
            .await
            .context("查询每日统计列表失败")?;

        Ok(page.map(DailyStatsVo::from_model))
    }

    pub async fn summary(&self, query: DailyStatsQueryDto) -> ApiResult<DailyStatsSummaryVo> {
        let rows = daily_stats::Entity::find()
            .filter(query)
            .all(&self.db)
            .await
            .context("查询每日统计汇总失败")?;

        Ok(DailyStatsSummaryVo::from_rows(&rows))
    }

    pub async fn dashboard(&self, query: DashboardQueryDto) -> ApiResult<DashboardOverviewVo> {
        let date_cond = dashboard_date_condition(&query);

        let summary_rows = daily_stats::Entity::find()
            .filter(date_cond.clone())
            .filter(global_summary_condition())
            .all(&self.db)
            .await
            .context("查询 dashboard 汇总失败")?;

        let by_channel_rows = daily_stats::Entity::find()
            .filter(date_cond.clone())
            .filter(channel_dimension_condition())
            .order_by_desc(daily_stats::Column::RequestCount)
            .all(&self.db)
            .await
            .context("查询 dashboard 渠道分布失败")?;

        let by_model_rows = daily_stats::Entity::find()
            .filter(date_cond)
            .filter(model_dimension_condition())
            .order_by_desc(daily_stats::Column::RequestCount)
            .all(&self.db)
            .await
            .context("查询 dashboard 模型分布失败")?;

        Ok(DashboardOverviewVo {
            summary: DailyStatsSummaryVo::from_rows(&summary_rows),
            by_channel: by_channel_rows
                .iter()
                .map(|row| DailyStatsDimensionVo::from_row(row.channel_id.to_string(), row))
                .collect(),
            by_model: by_model_rows
                .iter()
                .map(|row| DailyStatsDimensionVo::from_row(row.model_name.clone(), row))
                .collect(),
        })
    }
}

fn dashboard_date_condition(query: &DashboardQueryDto) -> Condition {
    let mut cond = Condition::all();
    if let Some(v) = query.start_date {
        cond = cond.add(daily_stats::Column::StatsDate.gte(v));
    }
    if let Some(v) = query.end_date {
        cond = cond.add(daily_stats::Column::StatsDate.lte(v));
    }
    cond
}

fn global_summary_condition() -> Condition {
    Condition::all()
        .add(daily_stats::Column::UserId.eq(0))
        .add(daily_stats::Column::ProjectId.eq(0))
        .add(daily_stats::Column::ChannelId.eq(0))
        .add(daily_stats::Column::AccountId.eq(0))
        .add(daily_stats::Column::ModelName.eq(""))
}

fn channel_dimension_condition() -> Condition {
    Condition::all()
        .add(daily_stats::Column::UserId.eq(0))
        .add(daily_stats::Column::ProjectId.eq(0))
        .add(daily_stats::Column::ChannelId.ne(0))
        .add(daily_stats::Column::AccountId.eq(0))
        .add(daily_stats::Column::ModelName.eq(""))
}

fn model_dimension_condition() -> Condition {
    Condition::all()
        .add(daily_stats::Column::UserId.eq(0))
        .add(daily_stats::Column::ProjectId.eq(0))
        .add(daily_stats::Column::ChannelId.eq(0))
        .add(daily_stats::Column::AccountId.eq(0))
        .add(daily_stats::Column::ModelName.ne(""))
}
