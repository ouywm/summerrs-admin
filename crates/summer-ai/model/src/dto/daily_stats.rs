use crate::entity::operations::daily_stats;
use schemars::JsonSchema;
use sea_orm::{ColumnTrait, Condition};
use serde::Deserialize;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DailyStatsQueryDto {
    pub start_date: Option<chrono::NaiveDate>,
    pub end_date: Option<chrono::NaiveDate>,
    pub user_id: Option<i64>,
    pub project_id: Option<i64>,
    pub channel_id: Option<i64>,
    pub account_id: Option<i64>,
    pub model_name: Option<String>,
}

impl From<DailyStatsQueryDto> for Condition {
    fn from(query: DailyStatsQueryDto) -> Self {
        let mut cond = Condition::all();
        if let Some(v) = query.start_date {
            cond = cond.add(daily_stats::Column::StatsDate.gte(v));
        }
        if let Some(v) = query.end_date {
            cond = cond.add(daily_stats::Column::StatsDate.lte(v));
        }
        if let Some(v) = query.user_id {
            cond = cond.add(daily_stats::Column::UserId.eq(v));
        }
        if let Some(v) = query.project_id {
            cond = cond.add(daily_stats::Column::ProjectId.eq(v));
        }
        if let Some(v) = query.channel_id {
            cond = cond.add(daily_stats::Column::ChannelId.eq(v));
        }
        if let Some(v) = query.account_id {
            cond = cond.add(daily_stats::Column::AccountId.eq(v));
        }
        if let Some(v) = query.model_name {
            cond = cond.add(daily_stats::Column::ModelName.eq(v));
        }
        cond
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DashboardQueryDto {
    pub start_date: Option<chrono::NaiveDate>,
    pub end_date: Option<chrono::NaiveDate>,
}
