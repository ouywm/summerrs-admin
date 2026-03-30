use chrono::{DateTime, FixedOffset};
use schemars::JsonSchema;
use serde::Serialize;

use crate::entity::channel::{ChannelStatus, ChannelType};
use crate::entity::channel_account::AccountStatus;

#[derive(Debug, Serialize, JsonSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AiRuntimeAccountVo {
    pub id: i64,
    pub name: String,
    pub status: AccountStatus,
    pub schedulable: bool,
    pub priority: i32,
    pub weight: i32,
    pub response_time: i32,
    pub failure_streak: i32,
    pub available: bool,
    pub last_used_at: Option<DateTime<FixedOffset>>,
    pub last_error_at: Option<DateTime<FixedOffset>>,
    pub last_error_code: String,
    pub last_error_message: String,
    pub rate_limited_until: Option<DateTime<FixedOffset>>,
    pub overload_until: Option<DateTime<FixedOffset>>,
    pub expires_at: Option<DateTime<FixedOffset>>,
}

#[derive(Debug, Serialize, JsonSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AiRuntimeChannelHealthVo {
    pub id: i64,
    pub name: String,
    pub channel_type: ChannelType,
    pub channel_group: String,
    pub status: ChannelStatus,
    pub priority: i32,
    pub weight: i32,
    pub auto_ban: bool,
    pub response_time: i32,
    pub failure_streak: i32,
    pub last_health_status: i16,
    pub available: bool,
    pub available_account_count: usize,
    pub last_used_at: Option<DateTime<FixedOffset>>,
    pub last_error_at: Option<DateTime<FixedOffset>>,
    pub last_error_code: String,
    pub last_error_message: String,
    pub accounts: Vec<AiRuntimeAccountVo>,
}

#[derive(Debug, Serialize, JsonSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AiRuntimeRouteCandidateVo {
    pub channel_id: i64,
    pub channel_name: String,
    pub channel_type: ChannelType,
    pub channel_status: ChannelStatus,
    pub route_priority: i32,
    pub route_weight: i32,
    pub failure_streak: i32,
    pub last_health_status: i16,
    pub available: bool,
    pub available_account_count: usize,
    pub accounts: Vec<AiRuntimeAccountVo>,
}

#[derive(Debug, Serialize, JsonSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AiRuntimeRouteVo {
    pub channel_group: String,
    pub model: String,
    pub endpoint_scope: String,
    pub candidates: Vec<AiRuntimeRouteCandidateVo>,
}
