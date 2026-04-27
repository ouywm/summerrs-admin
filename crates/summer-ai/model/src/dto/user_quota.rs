use crate::entity::billing::user_quota::{self, UserQuotaStatus};
use schemars::JsonSchema;
use sea_orm::{ColumnTrait, Condition, Set};
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateUserQuotaDto {
    pub user_id: i64,
    #[validate(length(max = 64, message = "渠道分组长度不能超过64"))]
    pub channel_group: Option<String>,
    pub status: Option<UserQuotaStatus>,
    pub quota: i64,
    pub daily_quota_limit: Option<i64>,
    pub monthly_quota_limit: Option<i64>,
    #[validate(length(max = 500, message = "备注长度不能超过500"))]
    pub remark: Option<String>,
}

impl CreateUserQuotaDto {
    pub fn into_active_model(self, operator: &str) -> user_quota::ActiveModel {
        user_quota::ActiveModel {
            user_id: Set(self.user_id),
            channel_group: Set(self.channel_group.unwrap_or_else(|| "default".to_string())),
            status: Set(self.status.unwrap_or(UserQuotaStatus::Normal)),
            quota: Set(self.quota),
            used_quota: Set(0),
            request_count: Set(0),
            daily_quota_limit: Set(self.daily_quota_limit.unwrap_or(0)),
            monthly_quota_limit: Set(self.monthly_quota_limit.unwrap_or(0)),
            daily_used_quota: Set(0),
            monthly_used_quota: Set(0),
            daily_window_start: Set(None),
            monthly_window_start: Set(None),
            last_request_time: Set(None),
            remark: Set(self.remark.unwrap_or_default()),
            create_by: Set(operator.to_string()),
            update_by: Set(operator.to_string()),
            ..Default::default()
        }
    }

    pub fn validate_business_rules(&self) -> Result<(), String> {
        if self.user_id <= 0 {
            return Err("userId 必须大于 0".to_string());
        }
        if self.quota < 0 {
            return Err("quota 不能为负数".to_string());
        }
        validate_non_negative("dailyQuotaLimit", self.daily_quota_limit)?;
        validate_non_negative("monthlyQuotaLimit", self.monthly_quota_limit)?;
        Ok(())
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateUserQuotaDto {
    #[validate(length(max = 64, message = "渠道分组长度不能超过64"))]
    pub channel_group: Option<String>,
    pub status: Option<UserQuotaStatus>,
    pub quota: Option<i64>,
    pub daily_quota_limit: Option<i64>,
    pub monthly_quota_limit: Option<i64>,
    #[validate(length(max = 500, message = "备注长度不能超过500"))]
    pub remark: Option<String>,
}

impl UpdateUserQuotaDto {
    pub fn apply_to(self, active: &mut user_quota::ActiveModel, operator: &str) {
        active.update_by = Set(operator.to_string());
        if let Some(v) = self.channel_group {
            active.channel_group = Set(v);
        }
        if let Some(v) = self.status {
            active.status = Set(v);
        }
        if let Some(v) = self.quota {
            active.quota = Set(v);
        }
        if let Some(v) = self.daily_quota_limit {
            active.daily_quota_limit = Set(v);
        }
        if let Some(v) = self.monthly_quota_limit {
            active.monthly_quota_limit = Set(v);
        }
        if let Some(v) = self.remark {
            active.remark = Set(v);
        }
    }

    pub fn validate_business_rules(&self) -> Result<(), String> {
        validate_non_negative("quota", self.quota)?;
        validate_non_negative("dailyQuotaLimit", self.daily_quota_limit)?;
        validate_non_negative("monthlyQuotaLimit", self.monthly_quota_limit)?;
        Ok(())
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct AdjustUserQuotaDto {
    pub quota_delta: i64,
    #[validate(length(max = 128, message = "参考号长度不能超过128"))]
    pub reference_no: Option<String>,
    #[validate(length(max = 500, message = "原因长度不能超过500"))]
    pub reason: Option<String>,
}

impl AdjustUserQuotaDto {
    pub fn validate_business_rules(&self) -> Result<(), String> {
        if self.quota_delta == 0 {
            return Err("quotaDelta 不能为 0".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserQuotaQueryDto {
    pub user_id: Option<i64>,
    pub status: Option<UserQuotaStatus>,
    pub channel_group: Option<String>,
    pub keyword: Option<String>,
}

impl From<UserQuotaQueryDto> for Condition {
    fn from(query: UserQuotaQueryDto) -> Self {
        let mut cond = Condition::all();
        if let Some(v) = query.user_id {
            cond = cond.add(user_quota::Column::UserId.eq(v));
        }
        if let Some(v) = query.status {
            cond = cond.add(user_quota::Column::Status.eq(v));
        }
        if let Some(v) = query.channel_group {
            cond = cond.add(user_quota::Column::ChannelGroup.eq(v));
        }
        if let Some(v) = query.keyword {
            let keyword = v.trim().to_string();
            if !keyword.is_empty() {
                cond = cond.add(
                    Condition::any()
                        .add(user_quota::Column::ChannelGroup.contains(&keyword))
                        .add(user_quota::Column::Remark.contains(&keyword)),
                );
            }
        }
        cond
    }
}

fn validate_non_negative(name: &str, value: Option<i64>) -> Result<(), String> {
    if value.is_some_and(|v| v < 0) {
        return Err(format!("{name} 不能为负数"));
    }
    Ok(())
}
