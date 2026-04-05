//! AI Guardrail 配置表实体

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "guardrail_config")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub scope_type: String,
    pub organization_id: i64,
    pub project_id: i64,
    pub enabled: bool,
    pub mode: String,
    #[sea_orm(column_type = "JsonBinary")]
    pub system_rules: serde_json::Value,
    #[sea_orm(column_type = "JsonBinary")]
    pub allowed_file_types: serde_json::Value,
    pub max_file_size_mb: i32,
    pub pii_action: String,
    pub secret_action: String,
    #[sea_orm(column_type = "JsonBinary")]
    pub metadata: serde_json::Value,
    pub remark: String,
    pub create_by: String,
    pub create_time: DateTimeWithTimeZone,
    pub update_by: String,
    pub update_time: DateTimeWithTimeZone,
    /// 关联 Guardrail 规则（一对多）
    #[sea_orm(has_many)]
    pub rules: HasMany<super::guardrail_rule::Entity>,
}
