use chrono::{DateTime, FixedOffset};
use schemars::JsonSchema;
use serde::Serialize;

use crate::entity::channel::{self, ChannelStatus, ChannelType};

/// 渠道列表 VO
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChannelVo {
    pub id: i64,
    pub name: String,
    pub channel_type: ChannelType,
    pub vendor_code: String,
    pub base_url: String,
    pub status: ChannelStatus,
    pub models: serde_json::Value,
    pub channel_group: String,
    pub weight: i32,
    pub priority: i32,
    pub auto_ban: bool,
    pub test_model: String,
    pub used_quota: i64,
    pub response_time: i32,
    pub failure_streak: i32,
    pub last_used_at: Option<DateTime<FixedOffset>>,
    pub remark: String,
    pub create_time: DateTime<FixedOffset>,
    pub update_time: DateTime<FixedOffset>,
}

impl ChannelVo {
    pub fn from_model(m: channel::Model) -> Self {
        Self {
            id: m.id,
            name: m.name,
            channel_type: m.channel_type,
            vendor_code: m.vendor_code,
            base_url: m.base_url,
            status: m.status,
            models: m.models,
            channel_group: m.channel_group,
            weight: m.weight,
            priority: m.priority,
            auto_ban: m.auto_ban,
            test_model: m.test_model,
            used_quota: m.used_quota,
            response_time: m.response_time,
            failure_streak: m.failure_streak,
            last_used_at: m.last_used_at,
            remark: m.remark,
            create_time: m.create_time,
            update_time: m.update_time,
        }
    }
}

/// 渠道详情 VO
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChannelDetailVo {
    pub id: i64,
    pub name: String,
    pub channel_type: ChannelType,
    pub vendor_code: String,
    pub base_url: String,
    pub status: ChannelStatus,
    pub models: serde_json::Value,
    pub model_mapping: serde_json::Value,
    pub channel_group: String,
    pub endpoint_scopes: serde_json::Value,
    pub capabilities: serde_json::Value,
    pub weight: i32,
    pub priority: i32,
    pub config: serde_json::Value,
    pub auto_ban: bool,
    pub test_model: String,
    pub used_quota: i64,
    pub balance: String,
    pub response_time: i32,
    pub success_rate: String,
    pub failure_streak: i32,
    pub last_used_at: Option<DateTime<FixedOffset>>,
    pub last_error_at: Option<DateTime<FixedOffset>>,
    pub last_error_code: String,
    pub last_error_message: String,
    pub remark: String,
    pub create_by: String,
    pub create_time: DateTime<FixedOffset>,
    pub update_by: String,
    pub update_time: DateTime<FixedOffset>,
}

impl ChannelDetailVo {
    pub fn from_model(m: channel::Model) -> Self {
        Self {
            id: m.id,
            name: m.name,
            channel_type: m.channel_type,
            vendor_code: m.vendor_code,
            base_url: m.base_url,
            status: m.status,
            models: m.models,
            model_mapping: m.model_mapping,
            channel_group: m.channel_group,
            endpoint_scopes: m.endpoint_scopes,
            capabilities: m.capabilities,
            weight: m.weight,
            priority: m.priority,
            config: m.config,
            auto_ban: m.auto_ban,
            test_model: m.test_model,
            used_quota: m.used_quota,
            balance: m.balance.to_string(),
            response_time: m.response_time,
            success_rate: m.success_rate.to_string(),
            failure_streak: m.failure_streak,
            last_used_at: m.last_used_at,
            last_error_at: m.last_error_at,
            last_error_code: m.last_error_code,
            last_error_message: m.last_error_message,
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
pub struct ChannelTestVo {
    pub channel_id: i64,
    pub success: bool,
    pub status_code: u16,
    pub response_time: i32,
    pub model: String,
    pub message: String,
}
