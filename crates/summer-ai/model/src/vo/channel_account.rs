use num_traits::ToPrimitive;
use schemars::JsonSchema;
use sea_orm::prelude::DateTimeWithTimeZone;
use serde::Serialize;

use crate::entity::routing::channel_account::{self, ChannelAccountStatus};

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChannelAccountVo {
    pub id: i64,
    pub channel_id: i64,
    pub name: String,
    pub credential_type: String,
    pub secret_ref: String,
    pub status: ChannelAccountStatus,
    pub schedulable: bool,
    pub priority: i32,
    pub weight: i32,
    pub rate_multiplier: f64,
    pub concurrency_limit: i32,
    pub quota_limit: f64,
    pub quota_used: f64,
    pub balance: f64,
    pub balance_updated_at: Option<DateTimeWithTimeZone>,
    pub response_time: i32,
    pub failure_streak: i32,
    pub last_used_at: Option<DateTimeWithTimeZone>,
    pub last_error_at: Option<DateTimeWithTimeZone>,
    pub last_error_code: String,
    pub rate_limited_until: Option<DateTimeWithTimeZone>,
    pub overload_until: Option<DateTimeWithTimeZone>,
    pub expires_at: Option<DateTimeWithTimeZone>,
    pub test_model: String,
    pub remark: String,
    pub create_by: String,
    pub create_time: DateTimeWithTimeZone,
    pub update_by: String,
    pub update_time: DateTimeWithTimeZone,
}

impl ChannelAccountVo {
    pub fn from_model(m: channel_account::Model) -> Self {
        Self {
            id: m.id,
            channel_id: m.channel_id,
            name: m.name,
            credential_type: m.credential_type,
            secret_ref: m.secret_ref,
            status: m.status,
            schedulable: m.schedulable,
            priority: m.priority,
            weight: m.weight,
            rate_multiplier: ToPrimitive::to_f64(&m.rate_multiplier).unwrap_or(1.0),
            concurrency_limit: m.concurrency_limit,
            quota_limit: ToPrimitive::to_f64(&m.quota_limit).unwrap_or(0.0),
            quota_used: ToPrimitive::to_f64(&m.quota_used).unwrap_or(0.0),
            balance: ToPrimitive::to_f64(&m.balance).unwrap_or(0.0),
            balance_updated_at: m.balance_updated_at,
            response_time: m.response_time,
            failure_streak: m.failure_streak,
            last_used_at: m.last_used_at,
            last_error_at: m.last_error_at,
            last_error_code: m.last_error_code,
            rate_limited_until: m.rate_limited_until,
            overload_until: m.overload_until,
            expires_at: m.expires_at,
            test_model: m.test_model,
            remark: m.remark,
            create_by: m.create_by,
            create_time: m.create_time,
            update_by: m.update_by,
            update_time: m.update_time,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChannelAccountDetailVo {
    #[serde(flatten)]
    pub base: ChannelAccountVo,
    pub credentials: serde_json::Value,
    pub extra: serde_json::Value,
    pub disabled_api_keys: serde_json::Value,
    pub last_error_message: String,
}
