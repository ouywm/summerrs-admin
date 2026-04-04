//! AI Prompt 防护规则表实体

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 状态（1=启用 2=禁用）
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    EnumIter,
    DeriveActiveEnum,
    Serialize_repr,
    Deserialize_repr,
    JsonSchema,
)]
#[sea_orm(rs_type = "i16", db_type = "SmallInteger")]
#[repr(i16)]
pub enum PromptProtectionStatus {
    #[sea_orm(num_value = 1)]
    Enabled = 1,
    #[sea_orm(num_value = 2)]
    Disabled = 2,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "prompt_protection_rule")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub organization_id: i64,
    pub project_id: i64,
    pub rule_code: String,
    pub rule_name: String,
    pub pattern_type: String,
    pub phase: String,
    pub action: String,
    pub priority: i32,
    #[sea_orm(column_type = "JsonBinary")]
    pub pattern_config: serde_json::Value,
    #[sea_orm(column_type = "Text")]
    pub rewrite_template: String,
    pub status: PromptProtectionStatus,
    #[sea_orm(column_type = "JsonBinary")]
    pub metadata: serde_json::Value,
    pub create_by: String,
    pub create_time: DateTimeWithTimeZone,
    pub update_by: String,
    pub update_time: DateTimeWithTimeZone,
}
