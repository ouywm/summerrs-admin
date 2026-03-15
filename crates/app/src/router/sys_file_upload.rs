//! 文件上传 / 下载路由

use common::error::{ApiErrors, ApiResult};
use common::extractor::{Multipart, Path, Query, ValidatedJson};
use common::file_util::read_multipart_files;
use common::response::Json;
use macros::log;
use model::dto::sys_file::{
    MultipartAbortDto, MultipartCompleteDto, MultipartInitDto, MultipartListPartsDto,
    PresignUploadCallbackDto, PresignUploadDto,
};
use model::vo::sys_file::{
    BatchUploadVo, FileUploadVo, MultipartInitVo, MultipartListPartsVo, PresignedDownloadVo,
    PresignedUploadVo,
};
use summer_auth::AdminUser;
use summer_web::axum::body::Body;
use summer_web::axum::http::{StatusCode, header};
use summer_web::axum::response::IntoResponse;
use summer_web::extractor::Component;
use summer_web::{get_api, post_api};

use crate::service::sys_file_upload_service::SysFileUploadService;

// ─── 服务端代理上传 ─────────────────────────────────────────────────────────

/// 单文件上传（multipart/form-data）
#[log(module = "文件管理", action = "上传文件", biz_type = Create, save_params = false)]
#[post_api("/file/upload")]
pub async fn upload_file(
    AdminUser { login_id, profile }: AdminUser,
    Component(svc): Component<SysFileUploadService>,
    Multipart(mut multipart): Multipart,
) -> ApiResult<Json<FileUploadVo>> {
    let mut files = read_multipart_files(&mut multipart).await?;
    let file = files
        .pop()
        .ok_or_else(|| ApiErrors::BadRequest("未找到上传文件".to_string()))?;

    let vo = svc
        .upload_file(
            &file.file_name,
            file.content_type.as_deref(),
            file.data,
            &login_id,
            &profile.nick_name,
        )
        .await?;

    Ok(Json(vo))
}

/// 批量文件上传（multipart/form-data，多文件）
#[log(module = "文件管理", action = "批量上传", biz_type = Create, save_params = false)]
#[post_api("/file/upload/batch")]
pub async fn batch_upload(
    AdminUser { login_id, profile }: AdminUser,
    Component(svc): Component<SysFileUploadService>,
    Multipart(mut multipart): Multipart,
) -> ApiResult<Json<BatchUploadVo>> {
    let files = read_multipart_files(&mut multipart).await?;
    if files.is_empty() {
        return Err(ApiErrors::BadRequest("未找到上传文件".to_string()));
    }

    let files = files
        .into_iter()
        .map(|f| (f.file_name, f.content_type, f.data))
        .collect();

    let vo = svc
        .batch_upload(files, &login_id, &profile.nick_name)
        .await?;

    Ok(Json(vo))
}

// ─── Presigned URL ──────────────────────────────────────────────────────────

/// 获取上传用 presigned URL（前端直传）
#[log(module = "文件管理", action = "获取上传链接", biz_type = Query)]
#[post_api("/file/presign/upload")]
pub async fn presign_upload(
    AdminUser { login_id, profile }: AdminUser,
    Component(svc): Component<SysFileUploadService>,
    ValidatedJson(dto): ValidatedJson<PresignUploadDto>,
) -> ApiResult<Json<PresignedUploadVo>> {
    let vo = svc
        .generate_presigned_upload(dto, &login_id, &profile.nick_name)
        .await?;
    Ok(Json(vo))
}

/// 前端直传完成回调
#[log(module = "文件管理", action = "确认上传", biz_type = Create)]
#[post_api("/file/presign/upload/callback")]
pub async fn presign_upload_callback(
    AdminUser { login_id, profile }: AdminUser,
    Component(svc): Component<SysFileUploadService>,
    ValidatedJson(dto): ValidatedJson<PresignUploadCallbackDto>,
) -> ApiResult<Json<FileUploadVo>> {
    let vo = svc
        .confirm_presigned_upload(dto, &login_id, &profile.nick_name)
        .await?;
    Ok(Json(vo))
}

/// 获取下载用 presigned URL
#[log(module = "文件管理", action = "获取下载链接", biz_type = Query)]
#[get_api("/file/{id}/presign/download")]
pub async fn presign_download(
    Component(svc): Component<SysFileUploadService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<PresignedDownloadVo>> {
    let vo = svc.generate_presigned_download(id).await?;
    Ok(Json(vo))
}

/// 服务端代理下载（返回二进制流）
#[log(module = "文件管理", action = "服务端代理下载", biz_type = Query, save_response = false)]
#[get_api("/file/{id}/download")]
pub async fn download_file(
    Component(svc): Component<SysFileUploadService>,
    Path(id): Path<i64>,
) -> Result<summer_web::axum::response::Response, ApiErrors> {
    let (file, byte_stream) = svc.download_file(id).await?;

    let content_type = if file.mime_type.is_empty() {
        mime::APPLICATION_OCTET_STREAM.to_string()
    } else {
        file.mime_type
    };

    // RFC 6266 + RFC 5987: Content-Disposition（兼容中文文件名）
    let ascii_name = file.original_name.replace('"', "\\\"");
    let encoded_name =
        url::form_urlencoded::byte_serialize(file.original_name.as_bytes()).collect::<String>();
    let disposition = format!(
        "attachment; filename=\"{}\"; filename*=UTF-8''{}",
        ascii_name, encoded_name
    );

    let body = Body::new(byte_stream.into_inner());

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, content_type),
            (header::CONTENT_DISPOSITION, disposition),
            (header::CONTENT_LENGTH, file.file_size.to_string()),
        ],
        body,
    )
        .into_response())
}

// ─── 前端驱动分片上传（含秒传 + 断点续传） ─────────────────────────────────────

/// 初始化分片上传（含秒传检查）
#[log(module = "文件管理", action = "初始化分片上传", biz_type = Create)]
#[post_api("/file/multipart/init")]
pub async fn multipart_init(
    AdminUser { login_id, profile }: AdminUser,
    Component(svc): Component<SysFileUploadService>,
    ValidatedJson(dto): ValidatedJson<MultipartInitDto>,
) -> ApiResult<Json<MultipartInitVo>> {
    let vo = svc
        .init_multipart_upload(dto, &login_id, &profile.nick_name)
        .await?;
    Ok(Json(vo))
}

/// 查询已上传分片（断点续传）
#[log(module = "文件管理", action = "查询已上传分片", biz_type = Query)]
#[get_api("/file/multipart/parts")]
pub async fn multipart_list_parts(
    Component(svc): Component<SysFileUploadService>,
    Query(dto): Query<MultipartListPartsDto>,
) -> ApiResult<Json<MultipartListPartsVo>> {
    let vo = svc.list_uploaded_parts(dto).await?;
    Ok(Json(vo))
}

/// 完成分片上传
#[log(module = "文件管理", action = "完成分片上传", biz_type = Create)]
#[post_api("/file/multipart/complete")]
pub async fn multipart_complete(
    AdminUser { login_id, profile }: AdminUser,
    Component(svc): Component<SysFileUploadService>,
    ValidatedJson(dto): ValidatedJson<MultipartCompleteDto>,
) -> ApiResult<Json<FileUploadVo>> {
    let vo = svc
        .complete_multipart_upload(dto, &login_id, &profile.nick_name)
        .await?;
    Ok(Json(vo))
}

/// 取消分片上传
#[log(module = "文件管理", action = "取消分片上传", biz_type = Delete)]
#[post_api("/file/multipart/abort")]
pub async fn multipart_abort(
    AdminUser { .. }: AdminUser,
    Component(svc): Component<SysFileUploadService>,
    ValidatedJson(dto): ValidatedJson<MultipartAbortDto>,
) -> ApiResult<()> {
    svc.abort_multipart_upload(dto).await?;
    Ok(())
}
