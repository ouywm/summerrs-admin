//! AI 告警规则表实体

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 告警严重级别（1=信息 2=警告 3=严重 4=紧急）
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
pub enum AlertSeverity {
    #[sea_orm(num_value = 1)]
    Info = 1,
    #[sea_orm(num_value = 2)]
    Warning = 2,
    #[sea_orm(num_value = 3)]
    Critical = 3,
    #[sea_orm(num_value = 4)]
    Urgent = 4,
}

/// 规则状态（1=启用 2=禁用）
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
pub enum AlertRuleStatus {
    #[sea_orm(num_value = 1)]
    Enabled = 1,
    #[sea_orm(num_value = 2)]
    Disabled = 2,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "alert_rule")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub domain_code: String,
    pub rule_code: String,
    pub rule_name: String,
    pub severity: AlertSeverity,
    pub metric_key: String,
    #[sea_orm(column_type = "Text")]
    pub condition_expr: String,
    #[sea_orm(column_type = "JsonBinary")]
    pub threshold_config: serde_json::Value,
    #[sea_orm(column_type = "JsonBinary")]
    pub channel_config: serde_json::Value,
    pub silence_seconds: i32,
    pub status: AlertRuleStatus,
    pub create_by: String,
    pub create_time: DateTimeWithTimeZone,
    pub update_by: String,
    pub update_time: DateTimeWithTimeZone,
}
