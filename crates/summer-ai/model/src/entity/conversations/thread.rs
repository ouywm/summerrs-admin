//! AI 线程表
//! 对应 sql/ai/thread.sql

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 状态：1=活跃 2=归档 3=关闭
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
pub enum ThreadStatus {
    /// 活跃
    #[sea_orm(num_value = 1)]
    Active = 1,
    /// 归档
    #[sea_orm(num_value = 2)]
    Archived = 2,
    /// 关闭
    #[sea_orm(num_value = 3)]
    Closed = 3,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "thread")]
pub struct Model {
    /// 线程ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 项目ID
    pub project_id: i64,
    /// 会话ID
    pub session_id: i64,
    /// 用户ID
    pub user_id: i64,
    /// 线程键
    pub thread_key: String,
    /// 线程名称
    pub thread_name: String,
    /// 状态：1=活跃 2=归档 3=关闭
    pub status: ThreadStatus,
    /// 扩展元数据（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub metadata: serde_json::Value,
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
