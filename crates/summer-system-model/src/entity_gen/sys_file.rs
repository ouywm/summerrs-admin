//! 系统文件实体

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "sys", table_name = "file")]
pub struct Model {
    /// 主键 ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 存储文件名
    pub file_name: String,
    /// 用户上传的原始文件名
    pub original_name: String,
    /// S3 对象 key
    pub file_path: String,
    /// 文件大小（字节）
    pub file_size: i64,
    /// 文件后缀
    pub file_suffix: String,
    /// MIME 类型
    pub mime_type: String,
    /// 存储桶名称
    pub bucket: String,
    /// 文件 MD5 摘要
    pub file_md5: String,
    /// 上传人昵称
    pub upload_by: String,
    /// 上传人 ID
    pub upload_by_id: Option<i64>,
    /// 创建时间
    pub create_time: DateTime,
}
