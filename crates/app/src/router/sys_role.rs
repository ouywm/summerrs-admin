use common::error::ApiResult;
use common::extractor::{Path, Query, ValidatedJson};
use common::response::Json;
use macros::log;
use model::dto::sys_role::{CreateRoleDto, RolePermissionDto, RoleQueryDto, UpdateRoleDto};
use model::vo::sys_role::{RolePermissionVo, RoleVo};
use summer_web::extractor::Component;
use summer_web::{delete_api, get_api, post_api, put_api};

use crate::service::sys_role_service::SysRoleService;
use summer_sea_orm::pagination::{Page, Pagination};

#[log(module = "角色管理", action = "查询角色列表", biz_type = Query)]
#[get_api("/role/list")]
pub async fn list_roles(
    Component(svc): Component<SysRoleService>,
    Query(query): Query<RoleQueryDto>,
    pagination: Pagination,
) -> ApiResult<Json<Page<RoleVo>>> {
    let vo = svc.list_roles(query, pagination).await?;
    Ok(Json(vo))
}

#[log(module = "角色管理", action = "创建角色", biz_type = Create)]
#[post_api("/role")]
pub async fn create_role(
    Component(svc): Component<SysRoleService>,
    ValidatedJson(dto): ValidatedJson<CreateRoleDto>,
) -> ApiResult<()> {
    svc.create_role(dto).await?;
    Ok(())
}

#[log(module = "角色管理", action = "更新角色", biz_type = Update)]
#[put_api("/role/{role_id}")]
pub async fn update_role(
    Component(svc): Component<SysRoleService>,
    Path(role_id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdateRoleDto>,
) -> ApiResult<()> {
    svc.update_role(role_id, dto).await?;
    Ok(())
}

#[log(module = "角色管理", action = "删除角色", biz_type = Delete)]
#[delete_api("/role/{role_id}")]
pub async fn delete_role(
    Component(svc): Component<SysRoleService>,
    Path(role_id): Path<i64>,
) -> ApiResult<()> {
    svc.delete_role(role_id).await?;
    Ok(())
}

#[log(module = "角色管理", action = "查询角色权限", biz_type = Query)]
#[get_api("/role/{role_id}/permissions")]
pub async fn get_role_permissions(
    Component(svc): Component<SysRoleService>,
    Path(role_id): Path<i64>,
) -> ApiResult<Json<RolePermissionVo>> {
    let vo = svc.get_role_permissions(role_id).await?;
    Ok(Json(vo))
}

#[log(module = "角色管理", action = "保存角色权限", biz_type = Update)]
#[put_api("/role/{role_id}/permissions")]
pub async fn save_role_permissions(
    Component(svc): Component<SysRoleService>,
    Path(role_id): Path<i64>,
    Json(dto): Json<RolePermissionDto>,
) -> ApiResult<()> {
    svc.save_role_permissions(role_id, dto).await?;
    Ok(())
}
