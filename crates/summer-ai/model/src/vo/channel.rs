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

/// 渠道列表 VO 别名。
///
/// Step 1.2 文档中使用 `ChannelListVo` 命名；
/// 当前项目沿用 `*Vo` 作为列表项惯例，因此这里保留一个兼容别名。
pub type ChannelListVo = ChannelVo;

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
    #[serde(skip_serializing)]
    #[schemars(skip)]
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
    pub last_error_message: Option<String>,
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
    pub success: bool,
    pub status_code: i32,
    pub elapsed_ms: i64,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn channel_detail_vo_does_not_serialize_config() {
        let now = Utc::now().fixed_offset();
        let vo = ChannelDetailVo {
            id: 1,
            name: "openai-primary".into(),
            channel_type: ChannelType::OpenAi,
            vendor_code: "openai".into(),
            base_url: "https://api.openai.com/v1".into(),
            status: ChannelStatus::Enabled,
            models: serde_json::json!(["gpt-5.4"]),
            model_mapping: serde_json::json!({}),
            channel_group: "default".into(),
            endpoint_scopes: serde_json::json!(["chat"]),
            capabilities: serde_json::json!(["stream"]),
            weight: 100,
            priority: 1,
            config: serde_json::json!({"api_key":"sk-secret"}),
            auto_ban: true,
            test_model: "gpt-5.4".into(),
            used_quota: 0,
            balance: "0".into(),
            response_time: 120,
            success_rate: "99.9".into(),
            failure_streak: 0,
            last_used_at: None,
            last_error_at: None,
            last_error_code: String::new(),
            last_error_message: None,
            remark: String::new(),
            create_by: "admin".into(),
            create_time: now,
            update_by: "admin".into(),
            update_time: now,
        };

        let json = serde_json::to_value(&vo).expect("serialize channel detail vo");
        assert!(json.get("config").is_none());
        assert_eq!(json["baseUrl"], "https://api.openai.com/v1");
    }
}
