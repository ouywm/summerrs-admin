//! AI Guardrail 规则表实体

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 规则严重级别（1=低 2=中 3=高）
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
pub enum GuardrailSeverity {
    #[sea_orm(num_value = 1)]
    Low = 1,
    #[sea_orm(num_value = 2)]
    Medium = 2,
    #[sea_orm(num_value = 3)]
    High = 3,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "guardrail_rule")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub guardrail_config_id: i64,
    pub organization_id: i64,
    pub project_id: i64,
    pub team_id: i64,
    pub token_id: i64,
    pub service_account_id: i64,
    pub rule_code: String,
    pub rule_name: String,
    pub rule_type: String,
    pub phase: String,
    pub action: String,
    pub priority: i32,
    pub enabled: bool,
    pub severity: GuardrailSeverity,
    pub model_pattern: String,
    pub endpoint_pattern: String,
    #[sea_orm(column_type = "JsonBinary")]
    pub condition_json: serde_json::Value,
    #[sea_orm(column_type = "JsonBinary")]
    pub rule_config: serde_json::Value,
    #[sea_orm(column_type = "JsonBinary")]
    pub metadata: serde_json::Value,
    pub remark: String,
    pub create_by: String,
    pub create_time: DateTimeWithTimeZone,
    pub update_by: String,
    pub update_time: DateTimeWithTimeZone,
    /// 关联 Guardrail 配置（多对一，逻辑关联 ai.guardrail_config.id，不建立数据库外键）
    #[sea_orm(belongs_to, from = "guardrail_config_id", to = "id", skip_fk)]
    pub guardrail_config: Option<super::guardrail_config::Entity>,
}
