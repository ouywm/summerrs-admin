use num_traits::ToPrimitive;
use schemars::JsonSchema;
use sea_orm::prelude::DateTimeWithTimeZone;
use serde::Serialize;

use crate::entity::operations::daily_stats;

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DailyStatsVo {
    pub id: i64,
    pub stats_date: chrono::NaiveDate,
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
    pub create_time: DateTimeWithTimeZone,
}

impl DailyStatsVo {
    pub fn from_model(m: daily_stats::Model) -> Self {
        Self {
            id: m.id,
            stats_date: m.stats_date,
            user_id: m.user_id,
            project_id: m.project_id,
            channel_id: m.channel_id,
            account_id: m.account_id,
            model_name: m.model_name,
            request_count: m.request_count,
            success_count: m.success_count,
            fail_count: m.fail_count,
            prompt_tokens: m.prompt_tokens,
            completion_tokens: m.completion_tokens,
            total_tokens: m.total_tokens,
            cached_tokens: m.cached_tokens,
            reasoning_tokens: m.reasoning_tokens,
            quota: m.quota,
            cost_total: ToPrimitive::to_f64(&m.cost_total).unwrap_or(0.0),
            avg_elapsed_time: m.avg_elapsed_time,
            avg_first_token_time: m.avg_first_token_time,
            create_time: m.create_time,
        }
    }
}

#[derive(Debug, Default, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DailyStatsSummaryVo {
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
}

impl DailyStatsSummaryVo {
    pub fn from_rows(rows: &[daily_stats::Model]) -> Self {
        let mut summary = Self::default();
        let mut elapsed_weighted = 0i64;
        let mut first_token_weighted = 0i64;

        for row in rows {
            summary.request_count += row.request_count;
            summary.success_count += row.success_count;
            summary.fail_count += row.fail_count;
            summary.prompt_tokens += row.prompt_tokens;
            summary.completion_tokens += row.completion_tokens;
            summary.total_tokens += row.total_tokens;
            summary.cached_tokens += row.cached_tokens;
            summary.reasoning_tokens += row.reasoning_tokens;
            summary.quota += row.quota;
            summary.cost_total += ToPrimitive::to_f64(&row.cost_total).unwrap_or(0.0);
            elapsed_weighted += i64::from(row.avg_elapsed_time) * row.request_count;
            first_token_weighted += i64::from(row.avg_first_token_time) * row.request_count;
        }

        if summary.request_count > 0 {
            summary.avg_elapsed_time = (elapsed_weighted / summary.request_count) as i32;
            summary.avg_first_token_time = (first_token_weighted / summary.request_count) as i32;
        }

        summary
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DailyStatsDimensionVo {
    pub key: String,
    pub request_count: i64,
    pub success_count: i64,
    pub fail_count: i64,
    pub total_tokens: i64,
    pub quota: i64,
    pub cost_total: f64,
}

impl DailyStatsDimensionVo {
    pub fn from_row(key: String, row: &daily_stats::Model) -> Self {
        Self {
            key,
            request_count: row.request_count,
            success_count: row.success_count,
            fail_count: row.fail_count,
            total_tokens: row.total_tokens,
            quota: row.quota,
            cost_total: ToPrimitive::to_f64(&row.cost_total).unwrap_or(0.0),
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DashboardOverviewVo {
    pub summary: DailyStatsSummaryVo,
    pub by_channel: Vec<DailyStatsDimensionVo>,
    pub by_model: Vec<DailyStatsDimensionVo>,
}
