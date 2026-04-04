//! AI 告警静默表实体

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 静默状态（1=生效中 2=已结束）
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
pub enum SilenceStatus {
    #[sea_orm(num_value = 1)]
    Active = 1,
    #[sea_orm(num_value = 2)]
    Ended = 2,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "alert_silence")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub alert_rule_id: i64,
    pub scope_type: String,
    pub scope_key: String,
    pub reason: String,
    pub status: SilenceStatus,
    #[sea_orm(column_type = "JsonBinary")]
    pub metadata: serde_json::Value,
    pub create_by: String,
    pub start_time: DateTimeWithTimeZone,
    pub end_time: DateTimeWithTimeZone,
    pub create_time: DateTimeWithTimeZone,
}
