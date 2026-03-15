//! 系统文件管理路由（列表、详情、删除）

use common::error::ApiResult;
use common::extractor::{Path, Query};
use common::response::Json;
use macros::log;
use model::dto::sys_file::FileQueryDto;
use model::vo::sys_file::FileVo;
use summer_web::extractor::Component;
use summer_web::{delete_api, get_api};

use crate::service::sys_file_service::SysFileService;
use summer_sea_orm::pagination::{Page, Pagination};

/// 文件列表（分页）
#[log(module = "文件管理", action = "查询文件列表", biz_type = Query)]
#[get_api("/file/list")]
pub async fn list_files(
    Component(svc): Component<SysFileService>,
    Query(query): Query<FileQueryDto>,
    pagination: Pagination,
) -> ApiResult<Json<Page<FileVo>>> {
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
    Component(svc): Component<SysFileService>,
    Path(id): Path<i64>,
) -> ApiResult<()> {
    svc.delete_file(id).await?;
    Ok(())
}
