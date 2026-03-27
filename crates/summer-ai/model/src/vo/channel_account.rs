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
        use std::str::FromStr;

        let to_f64 =
            |value: sea_orm::prelude::BigDecimal| f64::from_str(&value.to_string()).unwrap_or(0.0);

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
