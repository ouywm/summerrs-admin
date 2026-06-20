//! 文件夹路由（文件中心）

use summer_admin_macros::{has_perm, has_perms, log};
use summer_common::error::ApiResult;
use summer_common::extractor::{Path, ValidatedJson};
use summer_common::response::Json;
use summer_system_model::dto::sys_file_folder::{CreateFileFolderDto, UpdateFileFolderDto};
use summer_system_model::vo::sys_file_folder::{FileFolderTreeVo, FileFolderVo};
use summer_web::extractor::Component;
use summer_web::{delete_api, get_api, post_api, put_api};

use crate::service::sys_file_folder_service::SysFileFolderService;

/// 文件夹树查询
#[log(module = "文件管理", action = "查询文件夹树", biz_type = Query)]
#[has_perms(or(
    "file:manage:list",
    "file:manage:upload",
    "file:manage:update",
    "file:folder:tree",
    "file:folder:create",
    "file:folder:update"
))]
#[get_api("/file/folder/tree")]
pub async fn tree(
    Component(svc): Component<SysFileFolderService>,
) -> ApiResult<Json<Vec<FileFolderTreeVo>>> {
    let vo = svc.tree().await?;
    Ok(Json(vo))
}

/// 文件夹详情
#[log(module = "文件管理", action = "查询文件夹详情", biz_type = Query)]
#[has_perm("file:folder:detail")]
#[get_api("/file/folder/{id}")]
pub async fn get_folder(
    Component(svc): Component<SysFileFolderService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<FileFolderVo>> {
    let vo = svc.get_by_id(id).await?;
    Ok(Json(vo))
}

/// 创建文件夹
#[log(module = "文件管理", action = "创建文件夹", biz_type = Create)]
#[has_perm("file:folder:create")]
#[post_api("/file/folder")]
pub async fn create_folder(
    Component(svc): Component<SysFileFolderService>,
    ValidatedJson(dto): ValidatedJson<CreateFileFolderDto>,
) -> ApiResult<Json<FileFolderVo>> {
    let vo = svc.create(dto).await?;
    Ok(Json(vo))
}

/// 更新文件夹
#[log(module = "文件管理", action = "更新文件夹", biz_type = Update)]
#[has_perm("file:folder:update")]
#[put_api("/file/folder/{id}")]
pub async fn update_folder(
    Component(svc): Component<SysFileFolderService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdateFileFolderDto>,
) -> ApiResult<Json<FileFolderVo>> {
    let vo = svc.update(id, dto).await?;
    Ok(Json(vo))
}

/// 删除文件夹
#[log(module = "文件管理", action = "删除文件夹", biz_type = Delete)]
#[has_perm("file:folder:delete")]
#[delete_api("/file/folder/{id}")]
pub async fn delete_folder(
    Component(svc): Component<SysFileFolderService>,
    Path(id): Path<i64>,
) -> ApiResult<()> {
    svc.delete(id).await?;
    Ok(())
}
