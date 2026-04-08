use chrono::{DateTime, FixedOffset};
use num_traits::FromPrimitive;
use schemars::JsonSchema;
use sea_orm::prelude::BigDecimal;
use sea_orm::{ColumnTrait, Condition, Set};
use serde::{Deserialize, Serialize};
use validator::Validate;

use summer_ai_model::entity::channel_account::{self, ChannelAccountStatus};

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChannelAccountQuery {
    pub channel_id: Option<i64>,
    pub name: Option<String>,
    pub status: Option<ChannelAccountStatus>,
    pub schedulable: Option<bool>,
}

impl From<ChannelAccountQuery> for Condition {
    fn from(req: ChannelAccountQuery) -> Self {
        let mut condition = Condition::all().add(channel_account::Column::DeletedAt.is_null());
        if let Some(channel_id) = req.channel_id {
            condition = condition.add(channel_account::Column::ChannelId.eq(channel_id));
        }
        if let Some(name) = req.name {
            condition = condition.add(channel_account::Column::Name.contains(&name));
        }
        if let Some(status) = req.status {
            condition = condition.add(channel_account::Column::Status.eq(status));
        }
        if let Some(schedulable) = req.schedulable {
            condition = condition.add(channel_account::Column::Schedulable.eq(schedulable));
        }
        condition
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateChannelAccountReq {
    pub channel_id: i64,
    #[validate(length(min = 1, max = 128))]
    pub name: String,
    #[validate(length(min = 1, max = 64))]
    pub credential_type: String,
    #[serde(default)]
    pub credentials: serde_json::Value,
    #[serde(default)]
    pub secret_ref: String,
    #[serde(default = "default_true")]
    pub schedulable: bool,
    #[serde(default)]
    pub priority: i32,
    #[serde(default = "default_weight")]
    pub weight: i32,
    #[serde(default = "default_rate_multiplier")]
    pub rate_multiplier: f64,
    #[serde(default)]
    pub concurrency_limit: i32,
    #[serde(default)]
    pub quota_limit: f64,
    #[serde(default)]
    pub balance: f64,
    pub expires_at: Option<DateTime<FixedOffset>>,
    #[serde(default)]
    pub test_model: String,
    #[serde(default)]
    pub extra: serde_json::Value,
    #[serde(default)]
    pub remark: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateChannelAccountReq {
    pub channel_id: Option<i64>,
    #[validate(length(min = 1, max = 128))]
    pub name: Option<String>,
    pub credential_type: Option<String>,
    pub credentials: Option<serde_json::Value>,
    pub secret_ref: Option<String>,
    pub status: Option<ChannelAccountStatus>,
    pub schedulable: Option<bool>,
    pub priority: Option<i32>,
    pub weight: Option<i32>,
    pub rate_multiplier: Option<f64>,
    pub concurrency_limit: Option<i32>,
    pub quota_limit: Option<f64>,
    pub balance: Option<f64>,
    pub expires_at: Option<DateTime<FixedOffset>>,
    pub test_model: Option<String>,
    pub extra: Option<serde_json::Value>,
    pub remark: Option<String>,
}

fn default_true() -> bool {
    true
}

fn default_weight() -> i32 {
    1
}

fn default_rate_multiplier() -> f64 {
    1.0
}

fn decimal(value: f64) -> BigDecimal {
    BigDecimal::from_f64(value).unwrap_or_else(|| BigDecimal::from(0))
}

impl CreateChannelAccountReq {
    pub fn into_active_model(self, operator: &str) -> channel_account::ActiveModel {
        channel_account::ActiveModel {
            channel_id: Set(self.channel_id),
            name: Set(self.name),
            credential_type: Set(self.credential_type),
            credentials: Set(self.credentials),
            secret_ref: Set(self.secret_ref),
            status: Set(ChannelAccountStatus::Enabled),
            schedulable: Set(self.schedulable),
            priority: Set(self.priority),
            weight: Set(self.weight),
            rate_multiplier: Set(decimal(self.rate_multiplier)),
            concurrency_limit: Set(self.concurrency_limit),
            quota_limit: Set(decimal(self.quota_limit)),
            quota_used: Set(BigDecimal::from(0)),
            balance: Set(decimal(self.balance)),
            balance_updated_at: Set(None),
            response_time: Set(0),
            failure_streak: Set(0),
            last_used_at: Set(None),
            last_error_at: Set(None),
            last_error_code: Set(String::new()),
            last_error_message: Set(String::new()),
            rate_limited_until: Set(None),
            overload_until: Set(None),
            expires_at: Set(self.expires_at),
            test_model: Set(self.test_model),
            test_time: Set(None),
            extra: Set(self.extra),
            deleted_at: Set(None),
            remark: Set(self.remark),
            create_by: Set(operator.to_string()),
            update_by: Set(operator.to_string()),
            ..Default::default()
        }
    }
}

impl UpdateChannelAccountReq {
    pub fn apply_to(self, active: &mut channel_account::ActiveModel, operator: &str) {
        if let Some(channel_id) = self.channel_id {
            active.channel_id = Set(channel_id);
        }
        if let Some(name) = self.name {
            active.name = Set(name);
        }
        if let Some(credential_type) = self.credential_type {
            active.credential_type = Set(credential_type);
        }
        if let Some(credentials) = self.credentials {
            active.credentials = Set(credentials);
        }
        if let Some(secret_ref) = self.secret_ref {
            active.secret_ref = Set(secret_ref);
        }
        if let Some(status) = self.status {
            active.status = Set(status);
        }
        if let Some(schedulable) = self.schedulable {
            active.schedulable = Set(schedulable);
        }
        if let Some(priority) = self.priority {
            active.priority = Set(priority);
        }
        if let Some(weight) = self.weight {
            active.weight = Set(weight);
        }
        if let Some(rate_multiplier) = self.rate_multiplier {
            active.rate_multiplier = Set(decimal(rate_multiplier));
        }
        if let Some(concurrency_limit) = self.concurrency_limit {
            active.concurrency_limit = Set(concurrency_limit);
        }
        if let Some(quota_limit) = self.quota_limit {
            active.quota_limit = Set(decimal(quota_limit));
        }
        if let Some(balance) = self.balance {
            active.balance = Set(decimal(balance));
        }
        if let Some(expires_at) = self.expires_at {
            active.expires_at = Set(Some(expires_at));
        }
        if let Some(test_model) = self.test_model {
            active.test_model = Set(test_model);
        }
        if let Some(extra) = self.extra {
            active.extra = Set(extra);
        }
        if let Some(remark) = self.remark {
            active.remark = Set(remark);
        }
        active.update_by = Set(operator.to_string());
    }
}
