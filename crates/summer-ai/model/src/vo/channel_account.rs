use chrono::{DateTime, FixedOffset};
use schemars::JsonSchema;
use serde::Serialize;

use crate::entity::channel_account::{self, AccountStatus};

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChannelAccountVo {
    pub id: i64,
    pub channel_id: i64,
    pub name: String,
    pub credential_type: String,
    #[serde(skip_serializing)]
    #[schemars(skip)]
    pub credentials: serde_json::Value,
    pub secret_ref: String,
    pub status: AccountStatus,
    pub schedulable: bool,
    pub priority: i32,
    pub weight: i32,
    pub rate_multiplier: f64,
    pub concurrency_limit: i32,
    pub quota_limit: String,
    pub quota_used: String,
    pub balance: String,
    pub response_time: i32,
    pub failure_streak: i32,
    pub last_used_at: Option<DateTime<FixedOffset>>,
    pub expires_at: Option<DateTime<FixedOffset>>,
    pub test_model: String,
    pub remark: String,
    pub create_time: DateTime<FixedOffset>,
    pub update_time: DateTime<FixedOffset>,
}

impl ChannelAccountVo {
    pub fn from_model(model: channel_account::Model) -> Self {
        use num_traits::ToPrimitive;

        let to_f64 = |value: sea_orm::prelude::BigDecimal| value.to_f64().unwrap_or(0.0);

        Self {
            id: model.id,
            channel_id: model.channel_id,
            name: model.name,
            credential_type: model.credential_type,
            credentials: model.credentials,
            secret_ref: model.secret_ref,
            status: model.status,
            schedulable: model.schedulable,
            priority: model.priority,
            weight: model.weight,
            rate_multiplier: to_f64(model.rate_multiplier),
            concurrency_limit: model.concurrency_limit,
            quota_limit: model.quota_limit.to_string(),
            quota_used: model.quota_used.to_string(),
            balance: model.balance.to_string(),
            response_time: model.response_time,
            failure_streak: model.failure_streak,
            last_used_at: model.last_used_at,
            expires_at: model.expires_at,
            test_model: model.test_model,
            remark: model.remark,
            create_time: model.create_time,
            update_time: model.update_time,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn channel_account_vo_does_not_serialize_credentials() {
        let now = Utc::now().fixed_offset();
        let vo = ChannelAccountVo {
            id: 1,
            channel_id: 10,
            name: "primary".into(),
            credential_type: "api_key".into(),
            credentials: serde_json::json!({"api_key":"sk-secret"}),
            secret_ref: "vault://ai/openai".into(),
            status: AccountStatus::Enabled,
            schedulable: true,
            priority: 1,
            weight: 100,
            rate_multiplier: 1.0,
            concurrency_limit: 10,
            quota_limit: "100".into(),
            quota_used: "1".into(),
            balance: "99".into(),
            response_time: 120,
            failure_streak: 0,
            last_used_at: None,
            expires_at: None,
            test_model: "gpt-5.4".into(),
            remark: String::new(),
            create_time: now,
            update_time: now,
        };

        let json = serde_json::to_value(&vo).expect("serialize channel account vo");
        assert!(json.get("credentials").is_none());
        assert_eq!(json["secretRef"], "vault://ai/openai");
    }
}
