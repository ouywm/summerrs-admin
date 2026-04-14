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
    /// 业务编号（精确匹配）
    pub file_no: Option<String>,
    /// 原始文件名（模糊搜索）
    pub original_name: Option<String>,
    /// 展示名（模糊搜索）
    pub display_name: Option<String>,
    /// 文件扩展名（精确匹配）
    pub extension: Option<String>,
    /// 存储桶（精确匹配）
    pub bucket: Option<String>,
    /// 存储提供方（精确匹配）
    pub provider: Option<String>,
    /// 文件分类（精确匹配）
    pub kind: Option<String>,
    /// 可见性（精确匹配）
    pub visibility: Option<String>,
    /// 状态（精确匹配）
    pub status: Option<String>,
    /// 文件夹ID（精确匹配）
    pub folder_id: Option<i64>,
    /// 创建人ID（精确匹配）
    pub creator_id: Option<i64>,
}

/// 上传参数（用于 multipart 上传接口的 query string）
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FileUploadQueryDto {
    /// 目标文件夹 ID，不传或为 0 表示根（不归属任何文件夹）
    pub folder_id: Option<i64>,
    /// 是否解析 `filename` 中的相对路径（如 `docs/public/a.pdf`），并自动创建子文件夹
    #[serde(default)]
    pub preserve_path: bool,
}

impl From<FileQueryDto> for Condition {
    fn from(query: FileQueryDto) -> Self {
        let mut cond = Condition::all();
        if let Some(file_no) = query.file_no {
            cond = cond.add(sys_file::Column::FileNo.eq(file_no));
        }
        if let Some(name) = query.original_name {
            cond = cond.add(sys_file::Column::OriginalName.contains(name));
        }
        if let Some(name) = query.display_name {
            cond = cond.add(sys_file::Column::DisplayName.contains(name));
        }
        if let Some(ext) = query.extension {
            let ext = ext.strip_prefix('.').map(String::from).unwrap_or(ext);
            cond = cond.add(sys_file::Column::Extension.eq(ext));
        }
        if let Some(bucket) = query.bucket {
            cond = cond.add(sys_file::Column::Bucket.eq(bucket));
        }
        if let Some(provider) = query.provider {
            cond = cond.add(sys_file::Column::Provider.eq(provider));
        }
        if let Some(kind) = query.kind {
            cond = cond.add(sys_file::Column::Kind.eq(kind));
        }
        if let Some(visibility) = query.visibility {
            cond = cond.add(sys_file::Column::Visibility.eq(visibility));
        }
        if let Some(status) = query.status {
            cond = cond.add(sys_file::Column::Status.eq(status));
        }
        if let Some(folder_id) = query.folder_id {
            cond = cond.add(sys_file::Column::FolderId.eq(folder_id));
        }
        if let Some(creator_id) = query.creator_id {
            cond = cond.add(sys_file::Column::CreatorId.eq(creator_id));
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
    pub object_key: String,
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
    pub object_key: String,
    pub file_size: i64,
}

/// 完成分片上传
#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct MultipartCompleteDto {
    pub upload_id: String,
    pub object_key: String,
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
    pub object_key: String,
}

// ─── 文件中心动作 DTO ─────────────────────────────────────────────────────────

/// 生成公开分享链接
#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct GeneratePublicLinkDto {
    /// 公开链接有效期（秒），不传表示永不过期
    pub expires_in: Option<u64>,
}

/// 更新文件可见性
#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateFileVisibilityDto {
    #[validate(length(min = 1, max = 32, message = "visibility不能为空"))]
    pub visibility: String,
}

/// 更新文件状态
#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateFileStatusDto {
    #[validate(length(min = 1, max = 32, message = "status不能为空"))]
    pub status: String,
}

/// 更新展示名称
#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateFileDisplayNameDto {
    #[validate(length(min = 1, max = 255, message = "displayName不能为空"))]
    pub display_name: String,
}

/// 移动文件到文件夹
#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct MoveFileDto {
    pub folder_id: Option<i64>,
}
