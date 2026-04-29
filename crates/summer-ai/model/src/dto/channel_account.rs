use crate::entity::routing::channel_account::{self, ChannelAccountStatus};
use schemars::JsonSchema;
use sea_orm::{ColumnTrait, Condition, NotSet, Set};
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateChannelAccountDto {
    pub channel_id: i64,
    #[validate(length(min = 1, max = 128, message = "账号名称长度必须在1-128之间"))]
    pub name: String,
    #[validate(length(min = 1, max = 32, message = "凭证类型长度必须在1-32之间"))]
    pub credential_type: String,
    pub credentials: serde_json::Value,
    pub secret_ref: Option<String>,
    pub status: Option<ChannelAccountStatus>,
    pub schedulable: Option<bool>,
    pub priority: Option<i32>,
    pub weight: Option<i32>,
    pub rate_multiplier: Option<f64>,
    pub concurrency_limit: Option<i32>,
    pub quota_limit: Option<f64>,
    pub test_model: Option<String>,
    pub extra: Option<serde_json::Value>,
    #[validate(length(max = 500, message = "备注长度不能超过500"))]
    pub remark: Option<String>,
}

impl CreateChannelAccountDto {
    pub fn into_active_model(self, operator: &str) -> channel_account::ActiveModel {
        channel_account::ActiveModel {
            id: NotSet,
            channel_id: Set(self.channel_id),
            name: Set(self.name),
            credential_type: Set(self.credential_type),
            credentials: Set(self.credentials),
            secret_ref: Set(self.secret_ref.unwrap_or_default()),
            status: Set(self.status.unwrap_or(ChannelAccountStatus::Enabled)),
            schedulable: Set(self.schedulable.unwrap_or(true)),
            priority: Set(self.priority.unwrap_or(0)),
            weight: Set(self.weight.unwrap_or(1)),
            rate_multiplier: Set(self
                .rate_multiplier
                .and_then(decimal_from_f64)
                .unwrap_or_else(|| bigdecimal::BigDecimal::from(1))),
            concurrency_limit: Set(self.concurrency_limit.unwrap_or(0)),
            quota_limit: Set(self
                .quota_limit
                .and_then(decimal_from_f64)
                .unwrap_or_else(|| bigdecimal::BigDecimal::from(0))),
            quota_used: Set(bigdecimal::BigDecimal::from(0)),
            balance: Set(bigdecimal::BigDecimal::from(0)),
            balance_updated_at: Set(None),
            response_time: Set(0),
            failure_streak: Set(0),
            last_used_at: Set(None),
            last_error_at: Set(None),
            last_error_code: Set(String::new()),
            last_error_message: Set(String::new()),
            rate_limited_until: Set(None),
            overload_until: Set(None),
            expires_at: Set(None),
            test_model: Set(self.test_model.unwrap_or_default()),
            test_time: Set(None),
            extra: Set(self.extra.unwrap_or(serde_json::json!({}))),
            deleted_at: Set(None),
            remark: Set(self.remark.unwrap_or_default()),
            create_by: Set(operator.to_string()),
            create_time: NotSet,
            update_by: Set(operator.to_string()),
            update_time: NotSet,
            disabled_api_keys: Set(serde_json::json!([])),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateChannelAccountDto {
    #[validate(length(min = 1, max = 128, message = "账号名称长度必须在1-128之间"))]
    pub name: Option<String>,
    #[validate(length(min = 1, max = 32, message = "凭证类型长度必须在1-32之间"))]
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
    pub test_model: Option<String>,
    pub extra: Option<serde_json::Value>,
    #[validate(length(max = 500, message = "备注长度不能超过500"))]
    pub remark: Option<String>,
}

impl UpdateChannelAccountDto {
    pub fn apply_to(self, active: &mut channel_account::ActiveModel, operator: &str) {
        active.update_by = Set(operator.to_string());
        if let Some(v) = self.name {
            active.name = Set(v);
        }
        if let Some(v) = self.credential_type {
            active.credential_type = Set(v);
        }
        if let Some(v) = self.credentials {
            active.credentials = Set(v);
        }
        if let Some(v) = self.secret_ref {
            active.secret_ref = Set(v);
        }
        if let Some(v) = self.status {
            active.status = Set(v);
        }
        if let Some(v) = self.schedulable {
            active.schedulable = Set(v);
        }
        if let Some(v) = self.priority {
            active.priority = Set(v);
        }
        if let Some(v) = self.weight {
            active.weight = Set(v);
        }
        if let Some(v) = self.rate_multiplier.and_then(decimal_from_f64) {
            active.rate_multiplier = Set(v);
        }
        if let Some(v) = self.concurrency_limit {
            active.concurrency_limit = Set(v);
        }
        if let Some(v) = self.quota_limit.and_then(decimal_from_f64) {
            active.quota_limit = Set(v);
        }
        if let Some(v) = self.test_model {
            active.test_model = Set(v);
        }
        if let Some(v) = self.extra {
            active.extra = Set(v);
        }
        if let Some(v) = self.remark {
            active.remark = Set(v);
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChannelAccountQueryDto {
    pub channel_id: Option<i64>,
    pub status: Option<ChannelAccountStatus>,
    pub credential_type: Option<String>,
    pub keyword: Option<String>,
}

impl From<ChannelAccountQueryDto> for Condition {
    fn from(query: ChannelAccountQueryDto) -> Self {
        let mut cond = Condition::all();
        cond = cond.add(channel_account::Column::DeletedAt.is_null());
        if let Some(v) = query.channel_id {
            cond = cond.add(channel_account::Column::ChannelId.eq(v));
        }
        if let Some(v) = query.status {
            cond = cond.add(channel_account::Column::Status.eq(v));
        }
        if let Some(ref v) = query.credential_type {
            cond = cond.add(channel_account::Column::CredentialType.eq(v.clone()));
        }
        if let Some(ref v) = query.keyword {
            let kw = Condition::any()
                .add(channel_account::Column::Name.contains(v))
                .add(channel_account::Column::Remark.contains(v));
            cond = cond.add(kw);
        }
        cond
    }
}

fn decimal_from_f64(value: f64) -> Option<bigdecimal::BigDecimal> {
    if !value.is_finite() {
        return None;
    }
    bigdecimal::BigDecimal::try_from(value).ok()
}
