use schemars::JsonSchema;
use serde::Serialize;

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
}
