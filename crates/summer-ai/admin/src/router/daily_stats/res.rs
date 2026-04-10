use chrono::{DateTime, FixedOffset, NaiveDate};
use num_traits::ToPrimitive;
use schemars::JsonSchema;
use serde::Serialize;

use summer_ai_model::entity::alerts::daily_stats;

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DailyStatsRes {
    pub id: i64,
    pub stats_date: NaiveDate,
    pub user_id: i64,
    pub project_id: i64,
    pub channel_id: i64,
    pub account_id: i64,
    pub model_name: String,
    pub request_count: i64,
    pub success_count: i64,
    pub fail_count: i64,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
    pub cached_tokens: i64,
    pub reasoning_tokens: i64,
    pub quota: i64,
    pub cost_total: f64,
    pub avg_elapsed_time: i32,
    pub avg_first_token_time: i32,
    pub create_time: DateTime<FixedOffset>,
}

impl DailyStatsRes {
    pub fn from_model(model: daily_stats::Model) -> Self {
        Self {
            id: model.id,
            stats_date: model.stats_date,
            user_id: model.user_id,
            project_id: model.project_id,
            channel_id: model.channel_id,
            account_id: model.account_id,
            model_name: model.model_name,
            request_count: model.request_count,
            success_count: model.success_count,
            fail_count: model.fail_count,
            prompt_tokens: model.prompt_tokens,
            completion_tokens: model.completion_tokens,
            total_tokens: model.total_tokens,
            cached_tokens: model.cached_tokens,
            reasoning_tokens: model.reasoning_tokens,
            quota: model.quota,
            cost_total: model.cost_total.to_f64().unwrap_or(0.0),
            avg_elapsed_time: model.avg_elapsed_time,
            avg_first_token_time: model.avg_first_token_time,
            create_time: model.create_time,
        }
    }
}
