use chrono::NaiveDate;
use schemars::JsonSchema;
use sea_orm::{ColumnTrait, Condition};
use serde::{Deserialize, Serialize};

use summer_ai_model::entity::alerts::daily_stats;

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DailyStatsQuery {
    pub stats_date_start: Option<NaiveDate>,
    pub stats_date_end: Option<NaiveDate>,
    pub user_id: Option<i64>,
    pub project_id: Option<i64>,
    pub channel_id: Option<i64>,
    pub account_id: Option<i64>,
    pub model_name: Option<String>,
}

impl From<DailyStatsQuery> for Condition {
    fn from(req: DailyStatsQuery) -> Self {
        let mut condition = Condition::all();
        if let Some(stats_date_start) = req.stats_date_start {
            condition = condition.add(daily_stats::Column::StatsDate.gte(stats_date_start));
        }
        if let Some(stats_date_end) = req.stats_date_end {
            condition = condition.add(daily_stats::Column::StatsDate.lte(stats_date_end));
        }
        if let Some(user_id) = req.user_id {
            condition = condition.add(daily_stats::Column::UserId.eq(user_id));
        }
        if let Some(project_id) = req.project_id {
            condition = condition.add(daily_stats::Column::ProjectId.eq(project_id));
        }
        if let Some(channel_id) = req.channel_id {
            condition = condition.add(daily_stats::Column::ChannelId.eq(channel_id));
        }
        if let Some(account_id) = req.account_id {
            condition = condition.add(daily_stats::Column::AccountId.eq(account_id));
        }
        if let Some(model_name) = req.model_name {
            condition = condition.add(daily_stats::Column::ModelName.contains(&model_name));
        }
        condition
    }
}
