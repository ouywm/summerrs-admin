use chrono::{DateTime, FixedOffset};
use schemars::JsonSchema;
use sea_orm::Set;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::entity::channel_account::{self, AccountStatus};

/// 创建渠道账号
#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateChannelAccountDto {
    pub channel_id: i64,
    #[validate(length(min = 1, max = 128, message = "账号名称长度 1-128"))]
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

fn default_true() -> bool {
    true
}

fn default_weight() -> i32 {
    1
}

fn default_rate_multiplier() -> f64 {
    1.0
}

impl CreateChannelAccountDto {
    pub fn into_active_model(self, operator: &str) -> channel_account::ActiveModel {
        use sea_orm::prelude::BigDecimal;
        use std::str::FromStr;

        let decimal =
            |value: f64| BigDecimal::from_str(&value.to_string()).unwrap_or_else(|_| 0.into());

        channel_account::ActiveModel {
            channel_id: Set(self.channel_id),
            name: Set(self.name),
            credential_type: Set(self.credential_type),
            credentials: Set(self.credentials),
            secret_ref: Set(self.secret_ref),
            status: Set(AccountStatus::Enabled),
            schedulable: Set(self.schedulable),
            priority: Set(self.priority),
            weight: Set(self.weight),
            rate_multiplier: Set(decimal(self.rate_multiplier)),
            concurrency_limit: Set(self.concurrency_limit),
            quota_limit: Set(decimal(self.quota_limit)),
            quota_used: Set(decimal(0.0)),
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

/// 更新渠道账号
#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateChannelAccountDto {
    #[validate(length(min = 1, max = 128))]
    pub name: Option<String>,
    pub credential_type: Option<String>,
    pub credentials: Option<serde_json::Value>,
    pub secret_ref: Option<String>,
    pub status: Option<AccountStatus>,
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

impl UpdateChannelAccountDto {
    pub fn apply_to(self, active: &mut channel_account::ActiveModel, operator: &str) {
        use sea_orm::prelude::BigDecimal;
        use std::str::FromStr;

        let decimal =
            |value: f64| BigDecimal::from_str(&value.to_string()).unwrap_or_else(|_| 0.into());

        if let Some(value) = self.name {
            active.name = Set(value);
        }
        if let Some(value) = self.credential_type {
            active.credential_type = Set(value);
        }
        if let Some(value) = self.credentials {
            active.credentials = Set(value);
        }
        if let Some(value) = self.secret_ref {
            active.secret_ref = Set(value);
        }
        if let Some(value) = self.status {
            active.status = Set(value);
        }
        if let Some(value) = self.schedulable {
            active.schedulable = Set(value);
        }
        if let Some(value) = self.priority {
            active.priority = Set(value);
        }
        if let Some(value) = self.weight {
            active.weight = Set(value);
        }
        if let Some(value) = self.rate_multiplier {
            active.rate_multiplier = Set(decimal(value));
        }
        if let Some(value) = self.concurrency_limit {
            active.concurrency_limit = Set(value);
        }
        if let Some(value) = self.quota_limit {
            active.quota_limit = Set(decimal(value));
        }
        if let Some(value) = self.balance {
            active.balance = Set(decimal(value));
        }
        if let Some(value) = self.expires_at {
            active.expires_at = Set(Some(value));
        }
        if let Some(value) = self.test_model {
            active.test_model = Set(value);
        }
        if let Some(value) = self.extra {
            active.extra = Set(value);
        }
        if let Some(value) = self.remark {
            active.remark = Set(value);
        }
        active.update_by = Set(operator.to_string());
    }
}

/// 查询渠道账号
#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct QueryChannelAccountDto {
    pub channel_id: Option<i64>,
    pub name: Option<String>,
    pub status: Option<AccountStatus>,
    pub schedulable: Option<bool>,
}

impl From<QueryChannelAccountDto> for sea_orm::Condition {
    fn from(dto: QueryChannelAccountDto) -> Self {
        use sea_orm::ColumnTrait;

        let mut cond = sea_orm::Condition::all().add(channel_account::Column::DeletedAt.is_null());
        if let Some(value) = dto.channel_id {
            cond = cond.add(channel_account::Column::ChannelId.eq(value));
        }
        if let Some(value) = dto.name {
            cond = cond.add(channel_account::Column::Name.contains(&value));
        }
        if let Some(value) = dto.status {
            cond = cond.add(channel_account::Column::Status.eq(value));
        }
        if let Some(value) = dto.schedulable {
            cond = cond.add(channel_account::Column::Schedulable.eq(value));
        }
        cond
    }
}
