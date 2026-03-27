use chrono::{DateTime, FixedOffset};
use schemars::JsonSchema;
use sea_orm::Set;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::dto::endpoint_scope::{
    default_endpoint_scope_array, normalize_endpoint_scope_value,
};
use crate::entity::token::{self, TokenStatus};

/// 创建令牌
#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateTokenDto {
    pub user_id: i64,
    #[validate(length(min = 1, max = 128, message = "令牌名称长度 1-128"))]
    pub name: String,
    #[serde(default)]
    pub remain_quota: i64,
    #[serde(default)]
    pub unlimited_quota: bool,
    #[serde(default)]
    pub models: serde_json::Value,
    #[serde(default = "default_endpoint_scope_array")]
    pub endpoint_scopes: serde_json::Value,
    #[serde(default)]
    pub ip_whitelist: serde_json::Value,
    #[serde(default)]
    pub ip_blacklist: serde_json::Value,
    #[serde(default)]
    pub group_code_override: String,
    #[serde(default)]
    pub rpm_limit: i32,
    #[serde(default)]
    pub tpm_limit: i64,
    #[serde(default)]
    pub concurrency_limit: i32,
    #[serde(default)]
    pub daily_quota_limit: i64,
    #[serde(default)]
    pub monthly_quota_limit: i64,
    pub expire_time: Option<DateTime<FixedOffset>>,
    #[serde(default)]
    pub remark: String,
}

impl CreateTokenDto {
    /// 创建 ActiveModel（key_hash、key_prefix 由 service 层生成后填入）
    pub fn into_active_model(
        self,
        key_hash: String,
        key_prefix: String,
        operator: &str,
    ) -> Result<token::ActiveModel, String> {
        let now = chrono::Utc::now().fixed_offset();
        let endpoint_scopes =
            normalize_endpoint_scope_value(self.endpoint_scopes, "endpointScopes")?;

        Ok(token::ActiveModel {
            user_id: Set(self.user_id),
            service_account_id: Set(0),
            project_id: Set(0),
            name: Set(self.name),
            key_hash: Set(key_hash),
            key_prefix: Set(key_prefix),
            status: Set(TokenStatus::Enabled),
            remain_quota: Set(self.remain_quota),
            used_quota: Set(0),
            unlimited_quota: Set(self.unlimited_quota),
            models: Set(self.models),
            endpoint_scopes: Set(endpoint_scopes),
            ip_whitelist: Set(self.ip_whitelist),
            ip_blacklist: Set(self.ip_blacklist),
            group_code_override: Set(self.group_code_override),
            rpm_limit: Set(self.rpm_limit),
            tpm_limit: Set(self.tpm_limit),
            concurrency_limit: Set(self.concurrency_limit),
            daily_quota_limit: Set(self.daily_quota_limit),
            monthly_quota_limit: Set(self.monthly_quota_limit),
            daily_used_quota: Set(0),
            monthly_used_quota: Set(0),
            daily_window_start: Set(None),
            monthly_window_start: Set(None),
            expire_time: Set(self.expire_time),
            access_time: Set(None),
            last_used_ip: Set(String::new()),
            last_user_agent: Set(String::new()),
            remark: Set(self.remark),
            create_by: Set(operator.to_string()),
            update_by: Set(operator.to_string()),
            create_time: Set(now),
            update_time: Set(now),
            ..Default::default()
        })
    }
}

/// 更新令牌
#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateTokenDto {
    #[validate(length(min = 1, max = 128))]
    pub name: Option<String>,
    pub status: Option<TokenStatus>,
    pub remain_quota: Option<i64>,
    pub unlimited_quota: Option<bool>,
    pub models: Option<serde_json::Value>,
    pub endpoint_scopes: Option<serde_json::Value>,
    pub ip_whitelist: Option<serde_json::Value>,
    pub ip_blacklist: Option<serde_json::Value>,
    pub group_code_override: Option<String>,
    pub rpm_limit: Option<i32>,
    pub tpm_limit: Option<i64>,
    pub concurrency_limit: Option<i32>,
    pub daily_quota_limit: Option<i64>,
    pub monthly_quota_limit: Option<i64>,
    pub expire_time: Option<DateTime<FixedOffset>>,
    pub remark: Option<String>,
}

impl UpdateTokenDto {
    pub fn apply_to(self, active: &mut token::ActiveModel, operator: &str) -> Result<(), String> {
        if let Some(v) = self.name {
            active.name = Set(v);
        }
        if let Some(v) = self.status {
            active.status = Set(v);
        }
        if let Some(v) = self.remain_quota {
            active.remain_quota = Set(v);
        }
        if let Some(v) = self.unlimited_quota {
            active.unlimited_quota = Set(v);
        }
        if let Some(v) = self.models {
            active.models = Set(v);
        }
        if let Some(v) = self.endpoint_scopes {
            active.endpoint_scopes = Set(normalize_endpoint_scope_value(v, "endpointScopes")?);
        }
        if let Some(v) = self.ip_whitelist {
            active.ip_whitelist = Set(v);
        }
        if let Some(v) = self.ip_blacklist {
            active.ip_blacklist = Set(v);
        }
        if let Some(v) = self.group_code_override {
            active.group_code_override = Set(v);
        }
        if let Some(v) = self.rpm_limit {
            active.rpm_limit = Set(v);
        }
        if let Some(v) = self.tpm_limit {
            active.tpm_limit = Set(v);
        }
        if let Some(v) = self.concurrency_limit {
            active.concurrency_limit = Set(v);
        }
        if let Some(v) = self.daily_quota_limit {
            active.daily_quota_limit = Set(v);
        }
        if let Some(v) = self.monthly_quota_limit {
            active.monthly_quota_limit = Set(v);
        }
        if let Some(v) = self.expire_time {
            active.expire_time = Set(Some(v));
        }
        if let Some(v) = self.remark {
            active.remark = Set(v);
        }
        active.update_by = Set(operator.to_string());
        Ok(())
    }
}

/// 查询令牌
#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct QueryTokenDto {
    pub name: Option<String>,
    pub user_id: Option<i64>,
    pub status: Option<TokenStatus>,
    pub key_prefix: Option<String>,
}

impl From<QueryTokenDto> for sea_orm::Condition {
    fn from(dto: QueryTokenDto) -> Self {
        use sea_orm::ColumnTrait;
        let mut cond = sea_orm::Condition::all();
        if let Some(name) = dto.name {
            cond = cond.add(token::Column::Name.contains(&name));
        }
        if let Some(user_id) = dto.user_id {
            cond = cond.add(token::Column::UserId.eq(user_id));
        }
        if let Some(status) = dto.status {
            cond = cond.add(token::Column::Status.eq(status));
        }
        if let Some(prefix) = dto.key_prefix {
            cond = cond.add(token::Column::KeyPrefix.starts_with(&prefix));
        }
        cond
    }
}

/// 充值令牌配额
#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct RechargeTokenDto {
    #[validate(range(min = 1, message = "充值额度必须大于 0"))]
    pub quota: i64,
    #[serde(default)]
    pub remark: String,
}
