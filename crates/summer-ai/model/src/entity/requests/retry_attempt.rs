//! AI 重试记录表
//! 对应 sql/ai/retry_attempt.sql

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 状态：1=待重试 2=成功 3=失败 4=放弃
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
pub enum RetryAttemptStatus {
    /// 待重试
    #[sea_orm(num_value = 1)]
    PendingRetry = 1,
    /// 成功
    #[sea_orm(num_value = 2)]
    Succeeded = 2,
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
    /// 重试记录ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 域编码
    pub domain_code: String,
    /// 任务类型
    pub task_type: String,
    /// 关联对象标识
    pub reference_id: String,
    /// 关联请求ID
    pub request_id: String,
    /// 第几次重试
    pub attempt_no: i32,
    /// 状态：1=待重试 2=成功 3=失败 4=放弃
    pub status: RetryAttemptStatus,
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

#[sea_orm::entity::prelude::async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C>(mut self, _db: &C, insert: bool) -> Result<Self, sea_orm::DbErr>
    where
        C: sea_orm::ConnectionTrait,
    {
        let now = chrono::Utc::now().fixed_offset();
        self.update_time = sea_orm::Set(now);
        if insert {
            self.create_time = sea_orm::Set(now);
        }
        Ok(self)
    }
}
