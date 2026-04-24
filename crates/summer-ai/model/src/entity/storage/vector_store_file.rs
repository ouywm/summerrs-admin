use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 向量库文件状态（1=处理中 2=完成 3=失败 4=取消）
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
pub enum VectorStoreFileStatus {
    /// 处理中
    #[sea_orm(num_value = 1)]
    Processing = 1,
    /// 完成
    #[sea_orm(num_value = 2)]
    Completed = 2,
    /// 失败
    #[sea_orm(num_value = 3)]
    Failed = 3,
    /// 取消
    #[sea_orm(num_value = 4)]
    Cancelled = 4,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "vector_store_file")]
pub struct Model {
    /// 关联ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 向量库ID
    pub vector_store_id: i64,
    /// 文件ID
    pub file_id: i64,
    /// 状态：1=处理中 2=完成 3=失败 4=取消
    pub status: VectorStoreFileStatus,
    /// 入库后占用字节数
    pub usage_bytes: i64,
    /// 最近错误信息（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub last_error: serde_json::Value,
    /// 分块策略（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub chunking_strategy: serde_json::Value,
    /// 检索过滤属性（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub attributes: serde_json::Value,
    /// 软删除时间
    pub deleted_at: Option<DateTimeWithTimeZone>,
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
