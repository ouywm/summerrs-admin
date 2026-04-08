//! AI 托管对象表（批处理/微调/导入导出等异步对象）
//! 对应 sql/ai/managed_object.sql

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 状态：1=待提交 2=处理中 3=完成 4=失败 5=取消
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
pub enum ManagedObjectStatus {
    /// 待提交
    #[sea_orm(num_value = 1)]
    PendingSubmission = 1,
    /// 处理中
    #[sea_orm(num_value = 2)]
    Processing = 2,
    /// 完成
    #[sea_orm(num_value = 3)]
    Completed = 3,
    /// 失败
    #[sea_orm(num_value = 4)]
    Failed = 4,
    /// 取消
    #[sea_orm(num_value = 5)]
    Cancelled = 5,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "managed_object")]
pub struct Model {
    /// 对象ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 项目ID
    pub project_id: i64,
    /// 关联文件ID
    pub file_id: i64,
    /// 关联向量库ID
    pub vector_store_id: i64,
    /// 追踪ID
    pub trace_id: i64,
    /// 关联请求ID
    pub request_id: String,
    /// 对象类型：batch/fine_tune/import/export/vector_job 等
    pub object_type: String,
    /// 上游提供方编码
    pub provider_code: String,
    /// 统一对象键
    pub unified_object_key: String,
    /// 上游对象ID
    pub provider_object_id: String,
    /// 状态：1=待提交 2=处理中 3=完成 4=失败 5=取消
    pub status: ManagedObjectStatus,
    /// 提交载荷（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub payload: serde_json::Value,
    /// 处理结果（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub result: serde_json::Value,
    /// 扩展元数据（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub metadata: serde_json::Value,
    /// 提交时间
    pub submit_time: DateTimeWithTimeZone,
    /// 完成时间
    pub finish_time: Option<DateTimeWithTimeZone>,
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
