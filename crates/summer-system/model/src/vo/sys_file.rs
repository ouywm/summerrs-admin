//! 系统文件 VO

use chrono::NaiveDateTime;
use schemars::JsonSchema;
use serde::Serialize;
use summer_common::serde_utils::datetime_format;

use crate::entity::sys_file;

/// 文件上传成功响应
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FileUploadVo {
    pub file_id: i64,
    pub file_no: String,
    pub original_name: String,
    /// 文件访问 URL（公开桶直链）
    pub url: String,
    pub size: i64,
}

/// 文件详情
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FileVo {
    pub id: i64,
    pub file_no: String,
    pub provider: String,
    pub bucket: String,
    pub object_key: String,
    pub etag: String,
    pub original_name: String,
    pub display_name: String,
    pub extension: String,
    pub mime_type: String,
    pub kind: String,
    pub size: i64,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub duration: Option<i32>,
    pub page_count: Option<i32>,
    pub visibility: String,
    pub status: String,
    pub public_token: String,
    #[serde(serialize_with = "datetime_format::serialize_option")]
    pub public_url_expires_at: Option<NaiveDateTime>,
    pub tags: sea_orm::prelude::Json,
    pub remark: String,
    pub metadata: sea_orm::prelude::Json,
    #[serde(serialize_with = "datetime_format::serialize_option")]
    pub deleted_at: Option<NaiveDateTime>,
    pub deleted_by: Option<i64>,
    pub purge_status: String,
    #[serde(serialize_with = "datetime_format::serialize_option")]
    pub purged_at: Option<NaiveDateTime>,
    pub purge_error: Option<String>,
    pub folder_id: Option<i64>,
    pub creator_id: Option<i64>,
    /// 文件访问 URL（公开桶直链）
    pub url: String,
    #[serde(serialize_with = "datetime_format::serialize")]
    pub create_time: NaiveDateTime,
    #[serde(serialize_with = "datetime_format::serialize")]
    pub update_time: NaiveDateTime,
}

impl FileVo {
    /// 从 Model 构建，需要传入 URL 构建函数
    pub fn from_model_with_url(model: sys_file::Model, url: String) -> Self {
        Self {
            id: model.id,
            file_no: model.file_no,
            provider: model.provider,
            bucket: model.bucket,
            object_key: model.object_key,
            etag: model.etag,
            original_name: model.original_name,
            display_name: model.display_name,
            extension: model.extension,
            mime_type: model.mime_type,
            kind: model.kind,
            size: model.size,
            width: model.width,
            height: model.height,
            duration: model.duration,
            page_count: model.page_count,
            visibility: model.visibility,
            status: model.status,
            public_token: model.public_token,
            public_url_expires_at: model.public_url_expires_at,
            tags: model.tags,
            remark: model.remark,
            metadata: model.metadata,
            deleted_at: model.deleted_at,
            deleted_by: model.deleted_by,
            purge_status: model.purge_status,
            purged_at: model.purged_at,
            purge_error: model.purge_error,
            folder_id: model.folder_id,
            creator_id: model.creator_id,
            url,
            create_time: model.create_time,
            update_time: model.update_time,
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
    pub object_key: Option<String>,
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
    pub object_key: Option<String>,
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
