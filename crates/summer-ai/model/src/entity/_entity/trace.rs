//! AI 追踪表实体

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 追踪状态（1=运行中 2=成功 3=失败 4=取消）
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
pub enum TraceStatus {
    #[sea_orm(num_value = 1)]
    Running = 1,
    #[sea_orm(num_value = 2)]
    Success = 2,
    #[sea_orm(num_value = 3)]
    Failed = 3,
    #[sea_orm(num_value = 4)]
    Cancelled = 4,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "trace")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub project_id: i64,
    pub session_id: i64,
    pub thread_id: i64,
    pub user_id: i64,
    #[sea_orm(unique)]
    pub trace_key: String,
    pub root_request_id: String,
    pub source_type: String,
    pub status: TraceStatus,
    #[sea_orm(column_type = "JsonBinary")]
    pub metadata: serde_json::Value,
    pub started_at: DateTimeWithTimeZone,
    pub finished_at: Option<DateTimeWithTimeZone>,
    pub create_time: DateTimeWithTimeZone,
    pub update_time: DateTimeWithTimeZone,
    /// 关联 Span 列表（一对多）
    #[sea_orm(has_many)]
    pub spans: HasMany<super::trace_span::Entity>,
}
