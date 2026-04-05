use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::entity::config_entry;

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateConfigEntryDto {
    #[serde(default = "default_scope_type")]
    pub scope_type: String,
    #[serde(default)]
    pub scope_id: i64,
    #[serde(default)]
    pub category: String,
    pub config_key: String,
    #[serde(default)]
    pub config_value: serde_json::Value,
    #[serde(default)]
    pub secret_ref: String,
    #[serde(default)]
    pub remark: String,
}
fn default_scope_type() -> String {
    "system".into()
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateConfigEntryDto {
    pub config_value: Option<serde_json::Value>,
    pub secret_ref: Option<String>,
    pub status: Option<config_entry::ConfigEntryStatus>,
    pub remark: Option<String>,
}

#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct QueryConfigEntryDto {
    pub scope_type: Option<String>,
    pub scope_id: Option<i64>,
    pub category: Option<String>,
    pub config_key: Option<String>,
    pub status: Option<config_entry::ConfigEntryStatus>,
}

impl From<QueryConfigEntryDto> for sea_orm::Condition {
    fn from(dto: QueryConfigEntryDto) -> Self {
        use sea_orm::ColumnTrait;
        let mut c = sea_orm::Condition::all();
        if let Some(v) = dto.scope_type {
            c = c.add(config_entry::Column::ScopeType.eq(v));
        }
        if let Some(v) = dto.scope_id {
            c = c.add(config_entry::Column::ScopeId.eq(v));
        }
        if let Some(v) = dto.category {
            c = c.add(config_entry::Column::Category.eq(v));
        }
        if let Some(v) = dto.config_key {
            c = c.add(config_entry::Column::ConfigKey.contains(&v));
        }
        if let Some(v) = dto.status {
            c = c.add(config_entry::Column::Status.eq(v));
        }
        c
    }
}
