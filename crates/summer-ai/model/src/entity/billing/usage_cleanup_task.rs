//! AI 使用记录清理任务表
//! 对应 sql/ai/usage_cleanup_task.sql

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 状态：1=待执行 2=执行中 3=成功 4=失败 5=取消
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
pub enum UsageCleanupTaskStatus {
    /// 待执行
    #[sea_orm(num_value = 1)]
    PendingExecution = 1,
    /// 执行中
    #[sea_orm(num_value = 2)]
    Running = 2,
    /// 成功
    #[sea_orm(num_value = 3)]
    Succeeded = 3,
    /// 失败
    #[sea_orm(num_value = 4)]
    Failed = 4,
    /// 取消
    #[sea_orm(num_value = 5)]
    Cancelled = 5,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "usage_cleanup_task")]
pub struct Model {
    /// 任务ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 任务编号
    pub task_no: String,
    /// 状态：1=待执行 2=执行中 3=成功 4=失败 5=取消
    pub status: UsageCleanupTaskStatus,
    /// 目标清理表
    pub target_table: String,
    /// 清理过滤条件（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub filters: serde_json::Value,
    /// 已删除行数
    pub deleted_rows: i64,
    /// 错误信息
    #[sea_orm(column_type = "Text")]
    pub error_message: String,
    /// 执行人
    pub started_by: i64,
    /// 取消人
    pub canceled_by: i64,
    /// 取消时间
    pub canceled_at: Option<DateTimeWithTimeZone>,
    /// 开始时间
    pub started_at: Option<DateTimeWithTimeZone>,
    /// 结束时间
    pub finished_at: Option<DateTimeWithTimeZone>,
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
