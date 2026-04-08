use chrono::{DateTime, FixedOffset};
use schemars::JsonSchema;
use serde::Serialize;

use summer_ai_model::entity::channel::{self, ChannelLastHealthStatus, ChannelStatus, ChannelType};

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChannelListRes {
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

impl ChannelListRes {
    pub fn from_model(model: channel::Model) -> Self {
        Self {
            id: model.id,
            name: model.name,
            channel_type: model.channel_type,
            vendor_code: model.vendor_code,
            base_url: model.base_url,
            status: model.status,
            models: model.models,
            channel_group: model.channel_group,
            weight: model.weight,
            priority: model.priority,
            auto_ban: model.auto_ban,
            test_model: model.test_model,
            used_quota: model.used_quota,
            response_time: model.response_time,
            failure_streak: model.failure_streak,
            last_used_at: model.last_used_at,
            remark: model.remark,
            create_time: model.create_time,
            update_time: model.update_time,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChannelDetailRes {
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
    pub balance_updated_at: Option<DateTime<FixedOffset>>,
    pub response_time: i32,
    pub success_rate: String,
    pub failure_streak: i32,
    pub last_used_at: Option<DateTime<FixedOffset>>,
    pub last_error_at: Option<DateTime<FixedOffset>>,
    pub last_error_code: String,
    pub last_error_message: String,
    pub last_health_status: ChannelLastHealthStatus,
    pub remark: String,
    pub create_by: String,
    pub create_time: DateTime<FixedOffset>,
    pub update_by: String,
    pub update_time: DateTime<FixedOffset>,
}

impl ChannelDetailRes {
    pub fn from_model(model: channel::Model) -> Self {
        Self {
            id: model.id,
            name: model.name,
            channel_type: model.channel_type,
            vendor_code: model.vendor_code,
            base_url: model.base_url,
            status: model.status,
            models: model.models,
            model_mapping: model.model_mapping,
            channel_group: model.channel_group,
            endpoint_scopes: model.endpoint_scopes,
            capabilities: model.capabilities,
            weight: model.weight,
            priority: model.priority,
            config: model.config,
            auto_ban: model.auto_ban,
            test_model: model.test_model,
            used_quota: model.used_quota,
            balance: model.balance.to_string(),
            balance_updated_at: model.balance_updated_at,
            response_time: model.response_time,
            success_rate: model.success_rate.to_string(),
            failure_streak: model.failure_streak,
            last_used_at: model.last_used_at,
            last_error_at: model.last_error_at,
            last_error_code: model.last_error_code,
            last_error_message: model.last_error_message,
            last_health_status: model.last_health_status,
            remark: model.remark,
            create_by: model.create_by,
            create_time: model.create_time,
            update_by: model.update_by,
            update_time: model.update_time,
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::{ChannelDetailRes, ChannelLastHealthStatus, ChannelStatus, ChannelType};

    #[test]
    fn channel_detail_res_does_not_serialize_config() {
        let now = Utc::now().fixed_offset();
        let response = ChannelDetailRes {
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
            balance_updated_at: None,
            response_time: 120,
            success_rate: "99.9".into(),
            failure_streak: 0,
            last_used_at: None,
            last_error_at: None,
            last_error_code: String::new(),
            last_error_message: String::new(),
            last_health_status: ChannelLastHealthStatus::Healthy,
            remark: String::new(),
            create_by: "admin".into(),
            create_time: now,
            update_by: "admin".into(),
            update_time: now,
        };

        let json = serde_json::to_value(&response).expect("serialize channel detail response");
        assert!(json.get("config").is_none());
        assert_eq!(json["baseUrl"], "https://api.openai.com/v1");
    }
}
