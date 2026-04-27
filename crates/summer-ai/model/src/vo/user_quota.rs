use schemars::JsonSchema;
use sea_orm::prelude::DateTimeWithTimeZone;
use serde::Serialize;

use crate::entity::billing::user_quota::{self, UserQuotaStatus};

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserQuotaVo {
    pub id: i64,
    pub user_id: i64,
    pub channel_group: String,
    pub status: UserQuotaStatus,
    pub quota: i64,
    pub used_quota: i64,
    pub remaining_quota: i64,
    pub request_count: i64,
    pub daily_quota_limit: i64,
    pub monthly_quota_limit: i64,
    pub daily_used_quota: i64,
    pub monthly_used_quota: i64,
    pub daily_window_start: Option<DateTimeWithTimeZone>,
    pub monthly_window_start: Option<DateTimeWithTimeZone>,
    pub last_request_time: Option<DateTimeWithTimeZone>,
    pub remark: String,
    pub create_by: String,
    pub create_time: DateTimeWithTimeZone,
    pub update_by: String,
    pub update_time: DateTimeWithTimeZone,
}

impl UserQuotaVo {
    pub fn from_model(m: user_quota::Model) -> Self {
        Self {
            id: m.id,
            user_id: m.user_id,
            channel_group: m.channel_group,
            status: m.status,
            quota: m.quota,
            used_quota: m.used_quota,
            remaining_quota: m.quota - m.used_quota,
            request_count: m.request_count,
            daily_quota_limit: m.daily_quota_limit,
            monthly_quota_limit: m.monthly_quota_limit,
            daily_used_quota: m.daily_used_quota,
            monthly_used_quota: m.monthly_used_quota,
            daily_window_start: m.daily_window_start,
            monthly_window_start: m.monthly_window_start,
            last_request_time: m.last_request_time,
            remark: m.remark,
            create_by: m.create_by,
            create_time: m.create_time,
            update_by: m.update_by,
            update_time: m.update_time,
        }
    }
}
