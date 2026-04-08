//! AI 向量库表（RAG 知识库容器）
//! 对应 sql/ai/vector_store.sql

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 向量库状态（1=可用 2=处理中 3=失败 4=归档）
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
pub enum VectorStoreStatus {
    /// 可用
    #[sea_orm(num_value = 1)]
    Available = 1,
    /// 处理中
    #[sea_orm(num_value = 2)]
    Processing = 2,
    /// 失败
    #[sea_orm(num_value = 3)]
    Failed = 3,
    /// 归档
    #[sea_orm(num_value = 4)]
    Archived = 4,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "vector_store")]
pub struct Model {
    /// 向量库ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 所属对象类型
    pub owner_type: String,
    /// 所属对象ID
    pub owner_id: i64,
    /// 项目ID
    pub project_id: i64,
    /// 向量库名称
    pub name: String,
    /// 描述
    #[sea_orm(column_type = "Text")]
    pub description: String,
    /// Embedding 模型
    pub embedding_model: String,
    /// Embedding 维度
    pub embedding_dimensions: i32,
    /// 向量存储后端：pgvector/qdrant/weaviate/milvus
    pub storage_backend: String,
    /// 上游向量库ID
    pub provider_vector_store_id: String,
    /// 状态：1=可用 2=处理中 3=失败 4=归档
    pub status: VectorStoreStatus,
    /// 占用字节数
    pub usage_bytes: i64,
    /// 文件统计（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub file_counts: serde_json::Value,
    /// 元数据（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub metadata: serde_json::Value,
    /// 过期策略（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub expires_after: serde_json::Value,
    /// 过期时间
    pub expires_at: Option<DateTimeWithTimeZone>,
    /// 最后活跃时间
    pub last_active_at: Option<DateTimeWithTimeZone>,
    /// 软删除时间
    pub deleted_at: Option<DateTimeWithTimeZone>,
    /// 创建人
    pub create_by: String,
    /// 创建时间
    pub create_time: DateTimeWithTimeZone,
    /// 更新人
    pub update_by: String,
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
