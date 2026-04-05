//! AI 文件表实体
//! 上传文件、知识库文件、批处理输入文件等

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 文件状态（1=已上传 2=处理中 3=可用 4=失败 5=删除）
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
pub enum FileStatus {
    /// 已上传
    #[sea_orm(num_value = 1)]
    Uploaded = 1,
    /// 处理中
    #[sea_orm(num_value = 2)]
    Processing = 2,
    /// 可用
    #[sea_orm(num_value = 3)]
    Available = 3,
    /// 失败
    #[sea_orm(num_value = 4)]
    Failed = 4,
    /// 删除
    #[sea_orm(num_value = 5)]
    Deleted = 5,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "file")]
pub struct Model {
    /// 文件ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 所属对象类型：platform/organization/project/user/session/trace/conversation/message
    pub owner_type: String,
    /// 所属对象ID
    pub owner_id: i64,
    /// 项目ID
    pub project_id: i64,
    /// 会话ID
    pub session_id: i64,
    /// 追踪ID
    pub trace_id: i64,
    /// 关联请求ID
    pub request_id: String,
    /// 文件名
    pub filename: String,
    /// 文件用途：assistants/input/output/knowledge/batch/finetune
    pub purpose: String,
    /// 文件 MIME 类型
    pub content_type: String,
    /// 文件大小（字节）
    pub size_bytes: i64,
    /// 文件内容哈希
    pub content_hash: String,
    /// 存储后端：database/s3/minio/gcs/local
    pub storage_backend: String,
    /// 存储路径
    pub storage_path: String,
    /// 上游返回的文件ID
    pub provider_file_id: String,
    /// 状态
    pub status: FileStatus,
    /// 状态明细
    #[sea_orm(column_type = "Text")]
    pub status_detail: String,
    /// 扩展元数据（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub metadata: serde_json::Value,
    /// 过期时间
    pub expires_at: Option<DateTimeWithTimeZone>,
    /// 创建人
    pub create_by: String,
    /// 创建时间
    pub create_time: DateTimeWithTimeZone,
    /// 更新人
    pub update_by: String,
    /// 更新时间
    pub update_time: DateTimeWithTimeZone,
}
