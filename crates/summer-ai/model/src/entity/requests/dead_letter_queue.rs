//! AI 死信队列表
//! 对应 sql/ai/dead_letter_queue.sql

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 状态：1=待处理 2=重试中 3=已解决 4=放弃
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
pub enum DeadLetterQueueStatus {
    /// 待处理
    #[sea_orm(num_value = 1)]
    Pending = 1,
    /// 重试中
    #[sea_orm(num_value = 2)]
    Retrying = 2,
    /// 已解决
    #[sea_orm(num_value = 3)]
    Resolved = 3,
    /// 放弃
    #[sea_orm(num_value = 4)]
    Abandoned = 4,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "dead_letter_queue")]
pub struct Model {
    /// 死信ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 死信类型
    pub entry_type: String,
    /// 来源域：relay/guardrail/file/payment/webhook/scheduler
    pub source_domain: String,
    /// 来源对象标识
    pub reference_id: String,
    /// 原始载荷（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub payload: serde_json::Value,
    /// 失败原因
    #[sea_orm(column_type = "Text")]
    pub error_message: String,
    /// 扩展元数据（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub metadata: serde_json::Value,
    /// 重试次数
    pub retry_count: i32,
    /// 状态：1=待处理 2=重试中 3=已解决 4=放弃
    pub status: DeadLetterQueueStatus,
    /// 下次可处理时间
    pub available_at: DateTimeWithTimeZone,
    /// 最近重试时间
    pub last_retry_at: Option<DateTimeWithTimeZone>,
    /// 解决时间
    pub resolved_at: Option<DateTimeWithTimeZone>,
    /// 创建时间
    pub create_time: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
