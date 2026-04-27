use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 状态：1=待发送 2=已发送 3=失败 4=取消
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
pub enum SchedulerOutboxStatus {
    /// 待发送
    #[sea_orm(num_value = 1)]
    PendingSend = 1,
    /// 已发送
    #[sea_orm(num_value = 2)]
    Sent = 2,
    /// 失败
    #[sea_orm(num_value = 3)]
    Failed = 3,
    /// 取消
    #[sea_orm(num_value = 4)]
    Cancelled = 4,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "scheduler_outbox")]
pub struct Model {
    /// 外发任务ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 事件编码
    pub event_code: String,
    /// 聚合类型
    pub aggregate_type: String,
    /// 聚合ID
    pub aggregate_id: String,
    /// 事件载荷（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub payload: serde_json::Value,
    /// 附加头（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub headers: serde_json::Value,
    /// 状态：1=待发送 2=已发送 3=失败 4=取消
    pub status: SchedulerOutboxStatus,
    /// 计划发送时间
    pub scheduled_time: DateTimeWithTimeZone,
    /// 实际发送时间
    pub published_time: Option<DateTimeWithTimeZone>,
    /// 重试次数
    pub retry_count: i32,
    /// 错误信息
    #[sea_orm(column_type = "Text")]
    pub error_message: String,
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
