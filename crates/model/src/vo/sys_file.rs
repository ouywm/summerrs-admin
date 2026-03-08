//! 系统文件 VO

use chrono::NaiveDateTime;
use common::serde_utils::datetime_format;
use schemars::JsonSchema;
use serde::Serialize;

use crate::entity::sys_file;

/// 文件上传成功响应
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FileUploadVo {
    pub file_id: i64,
    pub original_name: String,
    /// 文件访问 URL（公开桶直链）
    pub url: String,
    pub file_size: i64,
}

/// 文件详情
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FileVo {
    pub id: i64,
    pub file_name: String,
    pub original_name: String,
    pub file_path: String,
    pub file_size: i64,
    pub file_suffix: String,
    pub mime_type: String,
    pub bucket: String,
    /// 文件访问 URL（公开桶直链）
    pub url: String,
    pub upload_by: String,
    #[serde(serialize_with = "datetime_format::serialize")]
    pub create_time: NaiveDateTime,
}

impl FileVo {
    /// 从 Model 构建，需要传入 URL 构建函数
    pub fn from_model_with_url(model: sys_file::Model, url: String) -> Self {
        Self {
            id: model.id,
            file_name: model.file_name,
            original_name: model.original_name,
            file_path: model.file_path,
            file_size: model.file_size,
            file_suffix: model.file_suffix,
            mime_type: model.mime_type,
            bucket: model.bucket,
            url,
            upload_by: model.upload_by,
            create_time: model.create_time,
        }
    }
}

/// Pre-signed URL 上传响应（前端直传用）
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PresignedUploadVo {
    /// 是否秒传命中
    pub fast_uploaded: bool,
    /// 秒传成功时直接返回文件信息
    pub file: Option<FileUploadVo>,
    /// presigned PUT URL
    pub upload_url: Option<String>,
    /// 生成的 object key，回调时需要
    pub file_path: Option<String>,
    /// 有效期（秒）
    pub expires_in: Option<u64>,
}

/// Pre-signed 下载 URL 响应
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PresignedDownloadVo {
    pub download_url: String,
    pub expires_in: u64,
}

/// 批量上传响应
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BatchUploadVo {
    pub success: Vec<FileUploadVo>,
    pub failed: Vec<UploadFailureVo>,
}

/// 上传失败项
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UploadFailureVo {
    pub original_name: String,
    pub reason: String,
}

// ─── 分片上传 VO ─────────────────────────────────────────────────────────────

/// 分片上传初始化响应
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MultipartInitVo {
    pub fast_uploaded: bool,
    pub file: Option<FileUploadVo>,
    pub upload_id: Option<String>,
    pub file_path: Option<String>,
    pub chunk_size: Option<u64>,
    pub total_parts: Option<i32>,
    pub part_urls: Option<Vec<PartPresignedUrl>>,
    pub expires_in: Option<u64>,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PartPresignedUrl {
    pub part_number: i32,
    pub upload_url: String,
}

/// 断点续传查询响应
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MultipartListPartsVo {
    pub uploaded_parts: Vec<UploadedPartVo>,
    pub pending_part_urls: Vec<PartPresignedUrl>,
    pub expires_in: u64,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UploadedPartVo {
    pub part_number: i32,
    pub e_tag: String,
    pub size: i64,
}
