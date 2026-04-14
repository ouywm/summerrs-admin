//! 系统文件管理路由（列表、详情、删除）

use summer_admin_macros::log;
use summer_auth::LoginUser;
use summer_common::error::ApiResult;
use summer_common::extractor::{Path, Query, ValidatedJson};
use summer_common::response::Json;
use summer_system_model::dto::sys_file::{
    FileQueryDto, GeneratePublicLinkDto, MoveFileDto, UpdateFileDisplayNameDto,
    UpdateFileStatusDto, UpdateFileVisibilityDto,
};
use summer_system_model::vo::sys_file::{FilePageVo, FilePublicLinkVo, FileVo};
use summer_web::Router;
use summer_web::extractor::Component;
use summer_web::handler::TypeRouter;
use summer_web::{delete_api, get_api, post_api, put_api};

use crate::service::sys_file_service::SysFileService;
use summer_sea_orm::pagination::Pagination;

/// 文件列表（分页）
#[log(module = "文件管理", action = "查询文件列表", biz_type = Query)]
#[get_api("/file/list")]
pub async fn list_files(
    Component(svc): Component<SysFileService>,
    Query(query): Query<FileQueryDto>,
    pagination: Pagination,
) -> ApiResult<Json<FilePageVo>> {
    let page = svc.list_files(query, pagination).await?;
    Ok(Json(page))
}

/// 文件详情
#[log(module = "文件管理", action = "查询文件详情", biz_type = Query)]
#[get_api("/file/{id}")]
pub async fn get_file(
    Component(svc): Component<SysFileService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<FileVo>> {
    let vo = svc.get_file(id).await?;
    Ok(Json(vo))
}

/// 删除文件
#[log(module = "文件管理", action = "删除文件", biz_type = Delete)]
#[delete_api("/file/{id}")]
pub async fn delete_file(
    LoginUser { login_id, .. }: LoginUser,
    Component(svc): Component<SysFileService>,
    Path(id): Path<i64>,
) -> ApiResult<()> {
    svc.delete_file(id, Some(login_id.user_id)).await?;
    Ok(())
}

/// 生成公开分享链接
#[log(module = "文件管理", action = "生成公开分享链接", biz_type = Create)]
#[post_api("/file/{id}/public-link")]
pub async fn generate_public_link(
    Component(svc): Component<SysFileService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<GeneratePublicLinkDto>,
) -> ApiResult<Json<FilePublicLinkVo>> {
    let vo = svc.generate_public_link(id, dto).await?;
    Ok(Json(vo))
}

/// 撤销公开分享链接
#[log(module = "文件管理", action = "撤销公开分享链接", biz_type = Delete)]
#[delete_api("/file/{id}/public-link")]
pub async fn revoke_public_link(
    Component(svc): Component<SysFileService>,
    Path(id): Path<i64>,
) -> ApiResult<()> {
    svc.revoke_public_link(id).await?;
    Ok(())
}

/// 更新可见性
#[log(module = "文件管理", action = "更新文件可见性", biz_type = Update)]
#[put_api("/file/{id}/visibility")]
pub async fn update_visibility(
    Component(svc): Component<SysFileService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdateFileVisibilityDto>,
) -> ApiResult<()> {
    svc.update_visibility(id, dto).await?;
    Ok(())
}

/// 更新状态
#[log(module = "文件管理", action = "更新文件状态", biz_type = Update)]
#[put_api("/file/{id}/status")]
pub async fn update_status(
    Component(svc): Component<SysFileService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdateFileStatusDto>,
) -> ApiResult<()> {
    svc.update_status(id, dto).await?;
    Ok(())
}

/// 更新展示名称
#[log(module = "文件管理", action = "更新展示名称", biz_type = Update)]
#[put_api("/file/{id}/display-name")]
pub async fn update_display_name(
    Component(svc): Component<SysFileService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdateFileDisplayNameDto>,
) -> ApiResult<()> {
    svc.update_display_name(id, dto).await?;
    Ok(())
}

/// 移动文件
#[log(module = "文件管理", action = "移动文件", biz_type = Update)]
#[put_api("/file/{id}/move")]
pub async fn move_file(
    Component(svc): Component<SysFileService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<MoveFileDto>,
) -> ApiResult<()> {
    svc.move_file(id, dto).await?;
    Ok(())
}

pub fn routes(router: Router) -> Router {
    router
        .typed_route(list_files)
        .typed_route(get_file)
        .typed_route(delete_file)
        .typed_route(generate_public_link)
        .typed_route(revoke_public_link)
        .typed_route(update_visibility)
        .typed_route(update_status)
        .typed_route(update_display_name)
        .typed_route(move_file)
}
