use crate::entity::billing::group_ratio::{self};
use schemars::JsonSchema;
use sea_orm::{ColumnTrait, Condition, NotSet, Set};
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateGroupRatioDto {
    #[validate(length(min = 1, max = 64, message = "分组编码长度必须在1-64之间"))]
    pub group_code: String,
    #[validate(length(min = 1, max = 128, message = "分组名称长度必须在1-128之间"))]
    pub group_name: String,
    pub ratio: f64,
    pub enabled: Option<bool>,
    pub model_whitelist: Option<Vec<String>>,
    pub model_blacklist: Option<Vec<String>>,
    pub endpoint_scopes: Option<Vec<String>>,
    #[validate(length(max = 64, message = "fallback 分组编码长度不能超过64"))]
    pub fallback_group_code: Option<String>,
    pub policy: Option<serde_json::Value>,
    #[validate(length(max = 500, message = "备注长度不能超过500"))]
    pub remark: Option<String>,
}

impl CreateGroupRatioDto {
    pub fn into_active_model(self, operator: &str) -> group_ratio::ActiveModel {
        group_ratio::ActiveModel {
            id: NotSet,
            group_code: Set(self.group_code),
            group_name: Set(self.group_name),
            ratio: Set(
                decimal_from_f64(self.ratio).unwrap_or_else(|| bigdecimal::BigDecimal::from(0))
            ),
            enabled: Set(self.enabled.unwrap_or(true)),
            model_whitelist: Set(string_list_json(self.model_whitelist)),
            model_blacklist: Set(string_list_json(self.model_blacklist)),
            endpoint_scopes: Set(string_list_json(self.endpoint_scopes)),
            fallback_group_code: Set(self.fallback_group_code.unwrap_or_default()),
            policy: Set(self.policy.unwrap_or_else(|| serde_json::json!({}))),
            remark: Set(self.remark.unwrap_or_default()),
            create_by: Set(operator.to_string()),
            create_time: NotSet,
            update_by: Set(operator.to_string()),
            update_time: NotSet,
        }
    }

    pub fn validate_business_rules(&self) -> Result<(), String> {
        validate_ratio(self.ratio)
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateGroupRatioDto {
    #[validate(length(min = 1, max = 128, message = "分组名称长度必须在1-128之间"))]
    pub group_name: Option<String>,
    pub ratio: Option<f64>,
    pub enabled: Option<bool>,
    pub model_whitelist: Option<Vec<String>>,
    pub model_blacklist: Option<Vec<String>>,
    pub endpoint_scopes: Option<Vec<String>>,
    #[validate(length(max = 64, message = "fallback 分组编码长度不能超过64"))]
    pub fallback_group_code: Option<String>,
    pub policy: Option<serde_json::Value>,
    #[validate(length(max = 500, message = "备注长度不能超过500"))]
    pub remark: Option<String>,
}

impl UpdateGroupRatioDto {
    pub fn apply_to(self, active: &mut group_ratio::ActiveModel, operator: &str) {
        active.update_by = Set(operator.to_string());
        if let Some(v) = self.group_name {
            active.group_name = Set(v);
        }
        if let Some(v) = self.ratio.and_then(decimal_from_f64) {
            active.ratio = Set(v);
        }
        if let Some(v) = self.enabled {
            active.enabled = Set(v);
        }
        if let Some(v) = self.model_whitelist {
            active.model_whitelist = Set(string_list_json(Some(v)));
        }
        if let Some(v) = self.model_blacklist {
            active.model_blacklist = Set(string_list_json(Some(v)));
        }
        if let Some(v) = self.endpoint_scopes {
            active.endpoint_scopes = Set(string_list_json(Some(v)));
        }
        if let Some(v) = self.fallback_group_code {
            active.fallback_group_code = Set(v);
        }
        if let Some(v) = self.policy {
            active.policy = Set(v);
        }
        if let Some(v) = self.remark {
            active.remark = Set(v);
        }
    }

    pub fn validate_business_rules(&self) -> Result<(), String> {
        if let Some(v) = self.ratio {
            validate_ratio(v)?;
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GroupRatioQueryDto {
    pub group_code: Option<String>,
    pub enabled: Option<bool>,
    pub keyword: Option<String>,
}

impl From<GroupRatioQueryDto> for Condition {
    fn from(query: GroupRatioQueryDto) -> Self {
        let mut cond = Condition::all();
        if let Some(v) = query.group_code {
            cond = cond.add(group_ratio::Column::GroupCode.eq(v));
        }
        if let Some(v) = query.enabled {
            cond = cond.add(group_ratio::Column::Enabled.eq(v));
        }
        if let Some(v) = query.keyword {
            let keyword = v.trim().to_string();
            if !keyword.is_empty() {
                cond = cond.add(
                    Condition::any()
                        .add(group_ratio::Column::GroupCode.contains(&keyword))
                        .add(group_ratio::Column::GroupName.contains(&keyword))
                        .add(group_ratio::Column::Remark.contains(&keyword)),
                );
            }
        }
        cond
    }
}

fn validate_ratio(value: f64) -> Result<(), String> {
    if !value.is_finite() || value < 0.0 {
        return Err("ratio 必须是大于等于 0 的有限数".to_string());
    }
    Ok(())
}

fn string_list_json(values: Option<Vec<String>>) -> serde_json::Value {
    serde_json::Value::Array(
        values
            .unwrap_or_default()
            .into_iter()
            .filter_map(|item| {
                let trimmed = item.trim();
                (!trimmed.is_empty()).then(|| serde_json::Value::String(trimmed.to_string()))
            })
            .collect(),
    )
}

fn decimal_from_f64(value: f64) -> Option<bigdecimal::BigDecimal> {
    if !value.is_finite() {
        return None;
    }
    bigdecimal::BigDecimal::try_from(value).ok()
}
