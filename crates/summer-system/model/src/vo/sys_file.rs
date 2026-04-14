//! 系统文件 VO

use chrono::NaiveDateTime;
use schemars::JsonSchema;
use serde::Serialize;
use summer_common::serde_utils::datetime_format;

use crate::entity::sys_file;

/// 文件下载 URL 响应
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FileDownloadUrlVo {
    pub url: String,
    #[serde(serialize_with = "datetime_format::serialize_option")]
    pub expires_at: Option<NaiveDateTime>,
}

/// 文件上传成功响应
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FileUploadVo {
    pub file_id: i64,
    pub file_no: String,
    pub original_name: String,
    pub size: i64,
    #[serde(flatten)]
    pub download: FileDownloadUrlVo,
}

/// 文件夹摘要（用于文件返回中的 folder 字段）
#[derive(Debug, Serialize, JsonSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FileFolderSummaryVo {
    pub id: i64,
    pub parent_id: i64,
    pub name: String,
    pub slug: String,
    pub visibility: String,
    pub sort: i32,
}

/// 创建人摘要（用于文件返回中的 creator 字段）
#[derive(Debug, Serialize, JsonSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FileCreatorSummaryVo {
    pub id: i64,
    pub user_name: String,
    pub nick_name: String,
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
    pub folder: Option<FileFolderSummaryVo>,
    pub creator: Option<FileCreatorSummaryVo>,
    #[serde(serialize_with = "datetime_format::serialize")]
    pub created_at: NaiveDateTime,
    #[serde(serialize_with = "datetime_format::serialize")]
    pub updated_at: NaiveDateTime,
}

impl FileVo {
    pub fn from_model(model: sys_file::Model) -> Self {
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
            folder: None,
            creator: None,
            created_at: model.create_time,
            updated_at: model.update_time,
        }
    }
}

/// 文件列表统计摘要
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FileListSummaryVo {
    pub total: u64,
    pub private_count: u64,
    pub public_count: u64,
}

/// 文件列表分页响应（对齐参考项目返回结构）
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FilePageVo {
    pub current: u64,
    pub size: u64,
    pub total: u64,
    pub records: Vec<FileVo>,
    pub summary: FileListSummaryVo,
}

/// 公开分享链接生成响应
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FilePublicLinkVo {
    pub token: String,
    pub visibility: String,
    pub public_url: String,
    #[serde(serialize_with = "datetime_format::serialize_option")]
    pub expires_at: Option<NaiveDateTime>,
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
    #[serde(flatten)]
    pub download: FileDownloadUrlVo,
    /// 有效期（秒）
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
