//! AI 告警事件表实体

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 事件状态（1=打开 2=已确认 3=已解决 4=忽略）
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
pub enum AlertEventStatus {
    #[sea_orm(num_value = 1)]
    Open = 1,
    #[sea_orm(num_value = 2)]
    Acknowledged = 2,
    #[sea_orm(num_value = 3)]
    Resolved = 3,
    #[sea_orm(num_value = 4)]
    Ignored = 4,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "alert_event")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub alert_rule_id: i64,
    pub event_code: String,
    pub severity: super::alert_rule::AlertSeverity,
    pub status: AlertEventStatus,
    pub source_domain: String,
    pub source_ref: String,
    pub title: String,
    #[sea_orm(column_type = "Text")]
    pub detail: String,
    #[sea_orm(column_type = "JsonBinary")]
    pub payload: serde_json::Value,
    pub first_triggered_at: DateTimeWithTimeZone,
    pub last_triggered_at: DateTimeWithTimeZone,
    pub ack_by: String,
    pub ack_time: Option<DateTimeWithTimeZone>,
    pub resolved_by: String,
    pub resolved_time: Option<DateTimeWithTimeZone>,
    pub create_time: DateTimeWithTimeZone,
}
