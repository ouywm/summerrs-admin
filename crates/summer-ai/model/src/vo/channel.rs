use num_traits::ToPrimitive;
use schemars::JsonSchema;
use sea_orm::prelude::DateTimeWithTimeZone;
use serde::Serialize;

use crate::entity::routing::channel::{self, ChannelLastHealthStatus, ChannelStatus, ChannelType};

// ---------------------------------------------------------------------------
// ChannelVo —— 列表项
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChannelVo {
    pub id: i64,
    pub name: String,
    pub channel_type: ChannelType,
    pub vendor_code: String,
    pub base_url: String,
    pub status: ChannelStatus,
    pub models: Vec<String>,
    pub channel_group: String,
    pub endpoint_scopes: Vec<String>,
    pub weight: i32,
    pub priority: i32,
    pub auto_ban: bool,
    pub test_model: String,
    pub used_quota: i64,
    pub response_time: i32,
    pub success_rate: f64,
    pub failure_streak: i32,
    pub last_health_status: ChannelLastHealthStatus,
    pub last_used_at: Option<DateTimeWithTimeZone>,
    pub last_error_at: Option<DateTimeWithTimeZone>,
    pub last_error_code: String,
    pub remark: String,
    pub create_by: String,
    pub create_time: DateTimeWithTimeZone,
    pub update_by: String,
    pub update_time: DateTimeWithTimeZone,
}

impl ChannelVo {
    pub fn from_model(m: channel::Model) -> Self {
        let models: Vec<String> = serde_json::from_value(m.models.clone()).unwrap_or_default();
        let endpoint_scopes: Vec<String> =
            serde_json::from_value(m.endpoint_scopes.clone()).unwrap_or_default();
        Self {
            id: m.id,
            name: m.name,
            channel_type: m.channel_type,
            vendor_code: m.vendor_code,
            base_url: m.base_url,
            status: m.status,
            models,
            channel_group: m.channel_group,
            endpoint_scopes,
            weight: m.weight,
            priority: m.priority,
            auto_ban: m.auto_ban,
            test_model: m.test_model,
            used_quota: m.used_quota,
            response_time: m.response_time,
            success_rate: ToPrimitive::to_f64(&m.success_rate).unwrap_or(0.0),
            failure_streak: m.failure_streak,
            last_health_status: m.last_health_status,
            last_used_at: m.last_used_at,
            last_error_at: m.last_error_at,
            last_error_code: m.last_error_code,
            remark: m.remark,
            create_by: m.create_by,
            create_time: m.create_time,
            update_by: m.update_by,
            update_time: m.update_time,
        }
    }
}

// ---------------------------------------------------------------------------
// ChannelDetailVo —— 详情（含 JSONB 展开）
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChannelDetailVo {
    #[serde(flatten)]
    pub base: ChannelVo,
    pub model_mapping: serde_json::Value,
    pub capabilities: serde_json::Value,
    pub config: serde_json::Value,
    pub last_error_message: String,
    pub balance: f64,
    pub balance_updated_at: Option<DateTimeWithTimeZone>,
}

impl ChannelDetailVo {
    pub fn from_model(m: channel::Model) -> Self {
        let base = ChannelVo::from_model(m.clone());
        Self {
            base,
            model_mapping: m.model_mapping,
            capabilities: m.capabilities,
            config: m.config,
            last_error_message: m.last_error_message,
            balance: ToPrimitive::to_f64(&m.balance).unwrap_or(0.0),
            balance_updated_at: m.balance_updated_at,
        }
    }
}

// ---------------------------------------------------------------------------
// ChannelStatusCountsVo —— 按类型聚合统计
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChannelStatusCountsVo {
    pub enabled: i64,
    pub manual_disabled: i64,
    pub auto_disabled: i64,
    pub archived: i64,
    pub total: i64,
}
