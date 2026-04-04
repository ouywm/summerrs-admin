//! AI 重试记录实体
//! 记录失败任务的每次重试行为

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 重试状态（1=待重试, 2=成功, 3=失败, 4=放弃）
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
pub enum RetryStatus {
    /// 待重试
    #[sea_orm(num_value = 1)]
    Pending = 1,
    /// 成功
    #[sea_orm(num_value = 2)]
    Success = 2,
    /// 失败
    #[sea_orm(num_value = 3)]
    Failed = 3,
    /// 放弃
    #[sea_orm(num_value = 4)]
    Abandoned = 4,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "retry_attempt")]
pub struct Model {
    /// 重试记录 ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 域编码
    pub domain_code: String,
    /// 任务类型
    pub task_type: String,
    /// 关联对象标识
    pub reference_id: String,
    /// 关联请求 ID
    pub request_id: String,
    /// 第几次重试
    pub attempt_no: i32,
    /// 重试状态
    pub status: RetryStatus,
    /// 退避秒数
    pub backoff_seconds: i32,
    /// 错误信息
    #[sea_orm(column_type = "Text")]
    pub error_message: String,
    /// 重试载荷（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub payload: serde_json::Value,
    /// 下次重试时间
    pub next_retry_at: Option<DateTimeWithTimeZone>,
    /// 创建时间
    pub create_time: DateTimeWithTimeZone,
    /// 更新时间
    pub update_time: DateTimeWithTimeZone,
}
