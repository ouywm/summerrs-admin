//! 系统文件 DTO

use crate::entity::sys_file;
use schemars::JsonSchema;
use sea_orm::{ColumnTrait, Condition};
use serde::{Deserialize, Serialize};
use validator::Validate;

/// 文件列表查询
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FileQueryDto {
    /// 原始文件名（模糊搜索）
    pub original_name: Option<String>,
    /// 文件后缀（精确匹配）
    pub file_suffix: Option<String>,
    /// 存储桶（精确匹配）
    pub bucket: Option<String>,
    /// 上传人（模糊搜索）
    pub upload_by: Option<String>,
}

impl From<FileQueryDto> for Condition {
    fn from(query: FileQueryDto) -> Self {
        let mut cond = Condition::all();
        if let Some(name) = query.original_name {
            cond = cond.add(sys_file::Column::OriginalName.contains(name));
        }
        if let Some(suffix) = query.file_suffix {
            let suffix = suffix.strip_prefix('.').map(String::from).unwrap_or(suffix);
            cond = cond.add(sys_file::Column::FileSuffix.eq(suffix));
        }
        if let Some(bucket) = query.bucket {
            cond = cond.add(sys_file::Column::Bucket.eq(bucket));
        }
        if let Some(upload_by) = query.upload_by {
            cond = cond.add(sys_file::Column::UploadBy.contains(upload_by));
        }
        cond
    }
}

/// Pre-signed URL 上传请求
#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct PresignUploadDto {
    /// 原始文件名
    #[validate(length(min = 1, message = "文件名不能为空"))]
    pub file_name: String,
    /// 文件大小（用于预校验）
    pub file_size: i64,
    /// 文件 MD5（可选，传入时触发秒传检查）
    #[validate(length(equal = 32, message = "MD5 必须为 32 位"))]
    pub file_md5: Option<String>,
}

/// Pre-signed URL 上传完成回调
#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct PresignUploadCallbackDto {
    /// 上传时返回的 object key
    pub file_path: String,
    /// 原始文件名
    #[validate(length(min = 1, message = "文件名不能为空"))]
    pub original_name: String,
    /// 文件大小
    pub file_size: i64,
    /// 前端计算的 MD5（可选）
    pub file_md5: Option<String>,
}

// ─── 分片上传 DTO ────────────────────────────────────────────────────────────

/// 分片上传初始化
#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct MultipartInitDto {
    #[validate(length(min = 1, message = "文件名不能为空"))]
    pub file_name: String,
    pub file_size: i64,
    #[validate(length(equal = 32, message = "MD5 必须为 32 位"))]
    pub file_md5: String,
}

/// 查询已上传分片（GET query string）
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MultipartListPartsDto {
    pub upload_id: String,
    pub file_path: String,
    pub file_size: i64,
}

/// 完成分片上传
#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct MultipartCompleteDto {
    pub upload_id: String,
    pub file_path: String,
    #[validate(length(min = 1, message = "文件名不能为空"))]
    pub original_name: String,
    /// 文件总大小（用于校验分片完整性）
    pub file_size: i64,
    pub file_md5: Option<String>,
}

/// 取消分片上传
#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct MultipartAbortDto {
    pub upload_id: String,
    pub file_path: String,
}
