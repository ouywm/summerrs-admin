//! AI Span 表实体
//! 记录 Trace 中每个步骤

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// Span 状态（1=运行中 2=成功 3=失败 4=跳过）
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
pub enum SpanStatus {
    #[sea_orm(num_value = 1)]
    Running = 1,
    #[sea_orm(num_value = 2)]
    Success = 2,
    #[sea_orm(num_value = 3)]
    Failed = 3,
    #[sea_orm(num_value = 4)]
    Skipped = 4,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "trace_span")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub trace_id: i64,
    pub parent_span_id: i64,
    pub span_key: String,
    pub span_name: String,
    pub span_type: String,
    pub target_kind: String,
    pub target_ref: String,
    pub status: SpanStatus,
    #[sea_orm(column_type = "JsonBinary")]
    pub input_payload: serde_json::Value,
    #[sea_orm(column_type = "JsonBinary")]
    pub output_payload: serde_json::Value,
    #[sea_orm(column_type = "Text")]
    pub error_message: String,
    #[sea_orm(column_type = "JsonBinary")]
    pub metadata: serde_json::Value,
    pub started_at: DateTimeWithTimeZone,
    pub finished_at: Option<DateTimeWithTimeZone>,
    pub create_time: DateTimeWithTimeZone,
}
