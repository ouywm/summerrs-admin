use crate::entity::platform::config_entry::{self, ConfigEntryStatus};
use schemars::JsonSchema;
use sea_orm::{ColumnTrait, Condition, Set};
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateConfigEntryDto {
    #[validate(length(min = 1, max = 32, message = "作用域类型长度必须在1-32之间"))]
    pub scope_type: String,
    pub scope_id: i64,
    #[validate(length(min = 1, max = 64, message = "配置分类长度必须在1-64之间"))]
    pub category: String,
    #[validate(length(min = 1, max = 128, message = "配置键长度必须在1-128之间"))]
    pub config_key: String,
    pub config_value: serde_json::Value,
    #[validate(length(max = 512, message = "secretRef 长度不能超过512"))]
    pub secret_ref: Option<String>,
    pub status: Option<ConfigEntryStatus>,
    #[validate(length(max = 500, message = "备注长度不能超过500"))]
    pub remark: Option<String>,
}

impl CreateConfigEntryDto {
    pub fn validate_business_rules(&self) -> Result<(), String> {
        validate_scope_binding(&self.scope_type, self.scope_id)
    }

    pub fn into_active_model(self, operator: &str) -> Result<config_entry::ActiveModel, String> {
        self.validate_business_rules()?;
        Ok(config_entry::ActiveModel {
            scope_type: Set(normalize_scope_type(&self.scope_type)?),
            scope_id: Set(self.scope_id),
            category: Set(self.category),
            config_key: Set(self.config_key),
            config_value: Set(self.config_value),
            secret_ref: Set(self.secret_ref.unwrap_or_default()),
            status: Set(self.status.unwrap_or(ConfigEntryStatus::Enabled)),
            version_no: Set(1),
            remark: Set(self.remark.unwrap_or_default()),
            create_by: Set(operator.to_string()),
            update_by: Set(operator.to_string()),
            ..Default::default()
        })
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateConfigEntryDto {
    #[validate(length(min = 1, max = 32, message = "作用域类型长度必须在1-32之间"))]
    pub scope_type: Option<String>,
    pub scope_id: Option<i64>,
    #[validate(length(min = 1, max = 64, message = "配置分类长度必须在1-64之间"))]
    pub category: Option<String>,
    #[validate(length(min = 1, max = 128, message = "配置键长度必须在1-128之间"))]
    pub config_key: Option<String>,
    pub config_value: Option<serde_json::Value>,
    #[validate(length(max = 512, message = "secretRef 长度不能超过512"))]
    pub secret_ref: Option<String>,
    pub status: Option<ConfigEntryStatus>,
    #[validate(length(max = 500, message = "备注长度不能超过500"))]
    pub remark: Option<String>,
}

impl UpdateConfigEntryDto {
    pub fn validate_business_rules(&self, current: &config_entry::Model) -> Result<(), String> {
        validate_scope_binding(
            self.scope_type.as_deref().unwrap_or(&current.scope_type),
            self.scope_id.unwrap_or(current.scope_id),
        )
    }

    pub fn has_mutations(&self) -> bool {
        self.scope_type.is_some()
            || self.scope_id.is_some()
            || self.category.is_some()
            || self.config_key.is_some()
            || self.config_value.is_some()
            || self.secret_ref.is_some()
            || self.status.is_some()
            || self.remark.is_some()
    }

    pub fn apply_to(
        self,
        active: &mut config_entry::ActiveModel,
        operator: &str,
        next_version_no: i32,
    ) -> Result<(), String> {
        active.update_by = Set(operator.to_string());
        active.version_no = Set(next_version_no);
        if let Some(v) = self.scope_type {
            active.scope_type = Set(normalize_scope_type(&v)?);
        }
        if let Some(v) = self.scope_id {
            active.scope_id = Set(v);
        }
        if let Some(v) = self.category {
            active.category = Set(v);
        }
        if let Some(v) = self.config_key {
            active.config_key = Set(v);
        }
        if let Some(v) = self.config_value {
            active.config_value = Set(v);
        }
        if let Some(v) = self.secret_ref {
            active.secret_ref = Set(v);
        }
        if let Some(v) = self.status {
            active.status = Set(v);
        }
        if let Some(v) = self.remark {
            active.remark = Set(v);
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConfigEntryQueryDto {
    pub scope_type: Option<String>,
    pub scope_id: Option<i64>,
    pub category: Option<String>,
    pub config_key: Option<String>,
    pub status: Option<ConfigEntryStatus>,
    pub keyword: Option<String>,
}

impl From<ConfigEntryQueryDto> for Condition {
    fn from(query: ConfigEntryQueryDto) -> Self {
        let mut cond = Condition::all();
        if let Some(v) = query.scope_type
            && let Ok(v) = normalize_scope_type(&v)
        {
            cond = cond.add(config_entry::Column::ScopeType.eq(v));
        }
        if let Some(v) = query.scope_id {
            cond = cond.add(config_entry::Column::ScopeId.eq(v));
        }
        if let Some(v) = query.category {
            cond = cond.add(config_entry::Column::Category.eq(v));
        }
        if let Some(v) = query.config_key {
            cond = cond.add(config_entry::Column::ConfigKey.eq(v));
        }
        if let Some(v) = query.status {
            cond = cond.add(config_entry::Column::Status.eq(v));
        }
        if let Some(v) = query.keyword {
            let keyword = v.trim().to_string();
            if !keyword.is_empty() {
                cond = cond.add(
                    Condition::any()
                        .add(config_entry::Column::Category.contains(&keyword))
                        .add(config_entry::Column::ConfigKey.contains(&keyword))
                        .add(config_entry::Column::Remark.contains(&keyword)),
                );
            }
        }
        cond
    }
}

fn validate_scope_binding(scope_type: &str, scope_id: i64) -> Result<(), String> {
    let normalized = normalize_scope_type(scope_type)?;
    match normalized.as_str() {
        "system" => {
            if scope_id != 0 {
                return Err("system 作用域的 scopeId 必须为 0".to_string());
            }
        }
        "organization" | "project" | "provider" | "model" | "plugin" => {
            if scope_id <= 0 {
                return Err(format!("{normalized} 作用域的 scopeId 必须大于 0"));
            }
        }
        _ => unreachable!(),
    }
    Ok(())
}

fn normalize_scope_type(scope_type: &str) -> Result<String, String> {
    let normalized = scope_type.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "system" | "organization" | "project" | "provider" | "model" | "plugin" => Ok(normalized),
        _ => Err("scopeType 仅支持 system/organization/project/provider/model/plugin".to_string()),
    }
}
