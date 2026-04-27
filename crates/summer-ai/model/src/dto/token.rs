use crate::entity::billing::token::{self, TokenStatus};
use schemars::JsonSchema;
use sea_orm::{ColumnTrait, Condition, Set};
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateTokenDto {
    pub user_id: i64,
    pub service_account_id: Option<i64>,
    pub project_id: Option<i64>,
    #[validate(length(min = 1, max = 128, message = "令牌名称长度必须在1-128之间"))]
    pub name: String,
    pub remain_quota: Option<i64>,
    pub unlimited_quota: Option<bool>,
    pub models: Option<Vec<String>>,
    pub endpoint_scopes: Option<Vec<String>>,
    pub ip_whitelist: Option<Vec<String>>,
    pub ip_blacklist: Option<Vec<String>>,
    #[validate(length(max = 64, message = "分组覆盖编码长度不能超过64"))]
    pub group_code_override: Option<String>,
    pub rpm_limit: Option<i32>,
    pub tpm_limit: Option<i64>,
    pub concurrency_limit: Option<i32>,
    pub daily_quota_limit: Option<i64>,
    pub monthly_quota_limit: Option<i64>,
    pub expire_time: Option<chrono::DateTime<chrono::FixedOffset>>,
    #[validate(length(max = 500, message = "备注长度不能超过500"))]
    pub remark: Option<String>,
    pub status: Option<TokenStatus>,
}

impl CreateTokenDto {
    pub fn into_active_model(
        self,
        operator: &str,
        _raw_key: &str,
        key_hash: &str,
        key_prefix: &str,
    ) -> token::ActiveModel {
        token::ActiveModel {
            user_id: Set(self.user_id),
            service_account_id: Set(self.service_account_id.unwrap_or(0)),
            project_id: Set(self.project_id.unwrap_or(0)),
            name: Set(self.name),
            key_hash: Set(key_hash.to_string()),
            key_prefix: Set(key_prefix.to_string()),
            status: Set(self.status.unwrap_or(TokenStatus::Enabled)),
            remain_quota: Set(self.remain_quota.unwrap_or(0)),
            used_quota: Set(0),
            unlimited_quota: Set(self.unlimited_quota.unwrap_or(false)),
            models: Set(string_list_json(self.models)),
            endpoint_scopes: Set(string_list_json(self.endpoint_scopes)),
            ip_whitelist: Set(string_list_json(self.ip_whitelist)),
            ip_blacklist: Set(string_list_json(self.ip_blacklist)),
            group_code_override: Set(trimmed_or_default(self.group_code_override)),
            rpm_limit: Set(self.rpm_limit.unwrap_or(0)),
            tpm_limit: Set(self.tpm_limit.unwrap_or(0)),
            concurrency_limit: Set(self.concurrency_limit.unwrap_or(0)),
            daily_quota_limit: Set(self.daily_quota_limit.unwrap_or(0)),
            monthly_quota_limit: Set(self.monthly_quota_limit.unwrap_or(0)),
            daily_used_quota: Set(0),
            monthly_used_quota: Set(0),
            daily_window_start: Set(None),
            monthly_window_start: Set(None),
            expire_time: Set(self.expire_time),
            access_time: Set(None),
            last_used_ip: Set(String::new()),
            last_user_agent: Set(String::new()),
            remark: Set(self.remark.unwrap_or_default()),
            create_by: Set(operator.to_string()),
            update_by: Set(operator.to_string()),
            ..Default::default()
        }
    }

    pub fn validate_business_rules(&self) -> Result<(), String> {
        validate_owner(self.user_id)?;
        validate_non_negative_i64("remainQuota", self.remain_quota)?;
        validate_non_negative_i64("tpmLimit", self.tpm_limit)?;
        validate_non_negative_i64("dailyQuotaLimit", self.daily_quota_limit)?;
        validate_non_negative_i64("monthlyQuotaLimit", self.monthly_quota_limit)?;
        validate_non_negative_i32("rpmLimit", self.rpm_limit)?;
        validate_non_negative_i32("concurrencyLimit", self.concurrency_limit)?;
        Ok(())
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateTokenDto {
    #[validate(length(min = 1, max = 128, message = "令牌名称长度必须在1-128之间"))]
    pub name: Option<String>,
    pub status: Option<TokenStatus>,
    pub remain_quota: Option<i64>,
    pub unlimited_quota: Option<bool>,
    pub models: Option<Vec<String>>,
    pub endpoint_scopes: Option<Vec<String>>,
    pub ip_whitelist: Option<Vec<String>>,
    pub ip_blacklist: Option<Vec<String>>,
    #[validate(length(max = 64, message = "分组覆盖编码长度不能超过64"))]
    pub group_code_override: Option<String>,
    pub rpm_limit: Option<i32>,
    pub tpm_limit: Option<i64>,
    pub concurrency_limit: Option<i32>,
    pub daily_quota_limit: Option<i64>,
    pub monthly_quota_limit: Option<i64>,
    pub expire_time: Option<chrono::DateTime<chrono::FixedOffset>>,
    #[validate(length(max = 500, message = "备注长度不能超过500"))]
    pub remark: Option<String>,
}

impl UpdateTokenDto {
    pub fn apply_to(self, active: &mut token::ActiveModel, operator: &str) {
        active.update_by = Set(operator.to_string());
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
            active.models = Set(string_list_json(Some(v)));
        }
        if let Some(v) = self.endpoint_scopes {
            active.endpoint_scopes = Set(string_list_json(Some(v)));
        }
        if let Some(v) = self.ip_whitelist {
            active.ip_whitelist = Set(string_list_json(Some(v)));
        }
        if let Some(v) = self.ip_blacklist {
            active.ip_blacklist = Set(string_list_json(Some(v)));
        }
        if let Some(v) = self.group_code_override {
            active.group_code_override = Set(v.trim().to_string());
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
    }

    pub fn validate_business_rules(&self) -> Result<(), String> {
        validate_non_negative_i64("remainQuota", self.remain_quota)?;
        validate_non_negative_i64("tpmLimit", self.tpm_limit)?;
        validate_non_negative_i64("dailyQuotaLimit", self.daily_quota_limit)?;
        validate_non_negative_i64("monthlyQuotaLimit", self.monthly_quota_limit)?;
        validate_non_negative_i32("rpmLimit", self.rpm_limit)?;
        validate_non_negative_i32("concurrencyLimit", self.concurrency_limit)?;
        Ok(())
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateTokenStatusDto {
    pub status: TokenStatus,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TokenQueryDto {
    pub user_id: Option<i64>,
    pub service_account_id: Option<i64>,
    pub project_id: Option<i64>,
    pub status: Option<TokenStatus>,
    pub keyword: Option<String>,
    pub key_prefix: Option<String>,
    pub group_code_override: Option<String>,
}

impl From<TokenQueryDto> for Condition {
    fn from(query: TokenQueryDto) -> Self {
        let mut cond = Condition::all();
        if let Some(v) = query.user_id {
            cond = cond.add(token::Column::UserId.eq(v));
        }
        if let Some(v) = query.service_account_id {
            cond = cond.add(token::Column::ServiceAccountId.eq(v));
        }
        if let Some(v) = query.project_id {
            cond = cond.add(token::Column::ProjectId.eq(v));
        }
        if let Some(v) = query.status {
            cond = cond.add(token::Column::Status.eq(v));
        }
        if let Some(v) = query.group_code_override {
            cond = cond.add(token::Column::GroupCodeOverride.eq(v));
        }
        if let Some(v) = query.key_prefix {
            cond = cond.add(token::Column::KeyPrefix.contains(v.trim()));
        }
        if let Some(v) = query.keyword {
            let keyword = v.trim().to_string();
            if !keyword.is_empty() {
                cond = cond.add(
                    Condition::any()
                        .add(token::Column::Name.contains(&keyword))
                        .add(token::Column::KeyPrefix.contains(&keyword))
                        .add(token::Column::Remark.contains(&keyword)),
                );
            }
        }
        cond
    }
}

fn string_list_json(values: Option<Vec<String>>) -> serde_json::Value {
    serde_json::Value::Array(
        values
            .unwrap_or_default()
            .into_iter()
            .filter_map(|value| {
                let trimmed = value.trim();
                (!trimmed.is_empty()).then(|| serde_json::Value::String(trimmed.to_string()))
            })
            .collect(),
    )
}

fn trimmed_or_default(value: Option<String>) -> String {
    value.map(|v| v.trim().to_string()).unwrap_or_default()
}

fn validate_owner(user_id: i64) -> Result<(), String> {
    if user_id <= 0 {
        return Err("userId 必须大于 0".to_string());
    }
    Ok(())
}

fn validate_non_negative_i64(name: &str, value: Option<i64>) -> Result<(), String> {
    if value.is_some_and(|v| v < 0) {
        return Err(format!("{name} 不能为负数"));
    }
    Ok(())
}

fn validate_non_negative_i32(name: &str, value: Option<i32>) -> Result<(), String> {
    if value.is_some_and(|v| v < 0) {
        return Err(format!("{name} 不能为负数"));
    }
    Ok(())
}
