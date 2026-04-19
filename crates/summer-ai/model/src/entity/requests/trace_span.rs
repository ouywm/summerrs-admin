//! AI Span 表
//! 对应 sql/ai/trace_span.sql

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 状态：1=运行中 2=成功 3=失败 4=跳过
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
pub enum TraceSpanStatus {
    /// 运行中
    #[sea_orm(num_value = 1)]
    Running = 1,
    /// 成功
    #[sea_orm(num_value = 2)]
    Succeeded = 2,
    /// 失败
    #[sea_orm(num_value = 3)]
    Failed = 3,
    /// 跳过
    #[sea_orm(num_value = 4)]
    Skipped = 4,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "trace_span")]
pub struct Model {
    /// Span ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 追踪ID
    pub trace_id: i64,
    /// 父 Span ID
    pub parent_span_id: i64,
    /// Span 键
    pub span_key: String,
    /// Span 名称
    pub span_name: String,
    /// Span 类型：llm/tool/plugin/retrieval/guardrail/router
    pub span_type: String,
    /// 目标类型
    pub target_kind: String,
    /// 目标引用
    pub target_ref: String,
    /// 状态：1=运行中 2=成功 3=失败 4=跳过
    pub status: TraceSpanStatus,
    /// 输入载荷（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub input_payload: serde_json::Value,
    /// 输出载荷（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub output_payload: serde_json::Value,
    /// 错误信息
    #[sea_orm(column_type = "Text")]
    pub error_message: String,
    /// 扩展元数据（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub metadata: serde_json::Value,
    /// 开始时间
    pub started_at: DateTimeWithTimeZone,
    /// 结束时间
    pub finished_at: Option<DateTimeWithTimeZone>,
    /// 创建时间
    pub create_time: DateTimeWithTimeZone,

    /// 关联 Trace（多对一，逻辑关联 ai.trace.id，不建立数据库外键）
    #[sea_orm(belongs_to, from = "trace_id", to = "id", skip_fk)]
    /// trace
    pub trace: Option<super::trace::Entity>,
}

#[sea_orm::entity::prelude::async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C>(mut self, _db: &C, insert: bool) -> Result<Self, sea_orm::DbErr>
    where
        C: sea_orm::ConnectionTrait,
    {
        if insert {
            let now = chrono::Utc::now().fixed_offset();
            self.create_time = sea_orm::Set(now);
        }
        Ok(self)
    }
}
