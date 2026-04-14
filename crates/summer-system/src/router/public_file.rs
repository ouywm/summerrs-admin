//! 公开文件访问路由（无需登录）

use summer_admin_macros::log;
use summer_common::error::ApiErrors;
use summer_common::extractor::Path;
use summer_web::axum::body::Body;
use summer_web::axum::http::{StatusCode, header};
use summer_web::axum::response::IntoResponse;
use summer_web::extractor::Component;
use summer_web::handler::TypeRouter;
use summer_web::{Router, get_api};

use crate::service::sys_file_upload_service::SysFileUploadService;

/// 公开分享链接下载（无需登录）
#[log(module = "文件管理", action = "公开分享下载", biz_type = Query, save_response = false)]
#[get_api("/public/file/{token}")]
pub async fn download_public_file(
    Component(svc): Component<SysFileUploadService>,
    Path(token): Path<String>,
) -> Result<summer_web::axum::response::Response, ApiErrors> {
    let (file, byte_stream) = svc.download_public_file(&token).await?;

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
            (header::CONTENT_LENGTH, file.size.to_string()),
        ],
        body,
    )
        .into_response())
}

pub fn routes(router: Router) -> Router {
    router.typed_route(download_public_file)
}
