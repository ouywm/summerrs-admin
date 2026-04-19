//! AI 数据存储索引表
//! 对应 sql/ai/data_storage.sql

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 状态：1=可用 2=归档 3=删除
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
pub enum DataStorageStatus {
    /// 可用
    #[sea_orm(num_value = 1)]
    Available = 1,
    /// 归档
    #[sea_orm(num_value = 2)]
    Archived = 2,
    /// 删除
    #[sea_orm(num_value = 3)]
    Deleted = 3,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "data_storage")]
pub struct Model {
    /// 数据索引ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 项目ID
    pub project_id: i64,
    /// 会话ID
    pub session_id: i64,
    /// 线程ID
    pub thread_id: i64,
    /// 追踪ID
    pub trace_id: i64,
    /// 数据键
    pub data_key: String,
    /// 数据类型：json/text/binary/pointer
    pub data_type: String,
    /// 存储后端
    pub storage_backend: String,
    /// 存储路径
    pub storage_path: String,
    /// JSON 内容
    #[sea_orm(column_type = "JsonBinary")]
    pub content_json: serde_json::Value,
    /// 文本内容
    #[sea_orm(column_type = "Text")]
    pub content_text: String,
    /// 内容哈希
    pub content_hash: String,
    /// 扩展元数据（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub metadata: serde_json::Value,
    /// 状态：1=可用 2=归档 3=删除
    pub status: DataStorageStatus,
    /// 过期时间
    pub expire_time: Option<DateTimeWithTimeZone>,
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
