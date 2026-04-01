use chrono::{DateTime, FixedOffset};
use schemars::JsonSchema;
use serde::Serialize;

use crate::entity::log;

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DashboardOverviewVo {
    pub today_request_count: i64,
    pub today_total_quota: i64,
    pub today_total_tokens: i64,
    pub active_user_count: i64,
    pub active_token_count: i64,
    pub success_request_count: i64,
    pub failed_request_count: i64,
    pub stream_request_count: i64,
    pub upstream_request_id_coverage_count: i64,
    pub avg_elapsed_time: f64,
    pub avg_stream_first_token_time: f64,
    pub total_channel_count: i64,
    pub enabled_channel_count: i64,
    pub available_channel_count: i64,
    pub auto_disabled_channel_count: i64,
    pub total_account_count: i64,
    pub enabled_account_count: i64,
    pub available_account_count: i64,
    pub rate_limited_account_count: i64,
    pub overloaded_account_count: i64,
    pub disabled_account_count: i64,
    pub unschedulable_account_count: i64,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RecentFailureVo {
    pub request_id: String,
    pub upstream_request_id: String,
    pub endpoint: String,
    pub requested_model: String,
    pub upstream_model: String,
    pub model_name: String,
    pub channel_name: String,
    pub account_name: String,
    pub status_code: i32,
    pub is_stream: bool,
    pub content: String,
    pub create_time: DateTime<FixedOffset>,
}

impl RecentFailureVo {
    pub fn from_model(model: log::Model) -> Self {
        Self {
            request_id: model.request_id,
            upstream_request_id: model.upstream_request_id,
            endpoint: model.endpoint,
            requested_model: model.requested_model,
            upstream_model: model.upstream_model,
            model_name: model.model_name,
            channel_name: model.channel_name,
            account_name: model.account_name,
            status_code: model.status_code,
            is_stream: model.is_stream,
            content: model.content,
            create_time: model.create_time,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FailureHotspotVo {
    pub group_key: String,
    pub failed_request_count: i64,
    pub stream_failure_count: i64,
    pub auth_failure_count: i64,
    pub rate_limit_failure_count: i64,
    pub overload_failure_count: i64,
    pub invalid_request_failure_count: i64,
    pub other_failure_count: i64,
    pub avg_elapsed_time: f64,
    pub latest_failure_at: DateTime<FixedOffset>,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DashboardTrendPointVo {
    pub bucket_start: DateTime<FixedOffset>,
    pub request_count: i64,
    pub success_request_count: i64,
    pub failed_request_count: i64,
    pub stream_request_count: i64,
    pub auth_failure_count: i64,
    pub rate_limit_failure_count: i64,
    pub overload_failure_count: i64,
    pub invalid_request_failure_count: i64,
    pub other_failure_count: i64,
    pub avg_elapsed_time: f64,
    pub avg_first_token_time: f64,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TopRequestVo {
    pub request_id: String,
    pub endpoint: String,
    pub requested_model: String,
    pub upstream_model: String,
    pub model_name: String,
    pub channel_name: String,
    pub account_name: String,
    pub status_code: i32,
    pub is_stream: bool,
    pub elapsed_time: i32,
    pub first_token_time: i32,
    pub total_tokens: i32,
    pub quota: i64,
    pub cost_total: String,
    pub create_time: DateTime<FixedOffset>,
}

impl TopRequestVo {
    pub fn from_model(model: log::Model) -> Self {
        Self {
            request_id: model.request_id,
            endpoint: model.endpoint,
            requested_model: model.requested_model,
            upstream_model: model.upstream_model,
            model_name: model.model_name,
            channel_name: model.channel_name,
            account_name: model.account_name,
            status_code: model.status_code,
            is_stream: model.is_stream,
            elapsed_time: model.elapsed_time,
            first_token_time: model.first_token_time,
            total_tokens: model.total_tokens,
            quota: model.quota,
            cost_total: model.cost_total.to_string(),
            create_time: model.create_time,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DashboardBreakdownVo {
    pub group_key: String,
    pub request_count: i64,
    pub success_request_count: i64,
    pub failed_request_count: i64,
    pub success_rate: f64,
    pub failure_rate: f64,
    pub avg_elapsed_time: f64,
    pub avg_first_token_time: f64,
    pub total_tokens: i64,
    pub total_quota: i64,
}
