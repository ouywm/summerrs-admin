use common::error::ApiResult;
use common::extractor::ValidatedJson;
use common::response::{ApiResponse, PageResponse};
use model::dto::sys_role::{CreateRoleDto, RolePermissionDto, RoleQueryDto, UpdateRoleDto};
use model::vo::sys_role::{RolePermissionVo, RoleVo};
use spring_web::axum::extract::Path;
use spring_web::axum::Json;
use spring_web::extractor::Component;
use spring_web::{delete, get, post, put};

use crate::service::sys_role_service::SysRoleService;

#[get("/role/list")]
pub async fn list_roles(
    Component(svc): Component<SysRoleService>,
    spring_web::axum::extract::Query(query): spring_web::axum::extract::Query<RoleQueryDto>,
) -> ApiResult<ApiResponse<PageResponse<RoleVo>>> {
    let vo = svc.list_roles(query).await?;
    Ok(ApiResponse::ok(vo))
}

#[post("/role")]
pub async fn create_role(
    Component(svc): Component<SysRoleService>,
    ValidatedJson(dto): ValidatedJson<CreateRoleDto>,
) -> ApiResult<ApiResponse<()>> {
    svc.create_role(dto).await?;
    Ok(ApiResponse::empty_with_msg("创建成功"))
}

#[put("/role/{role_id}")]
pub async fn update_role(
    Component(svc): Component<SysRoleService>,
    Path(role_id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdateRoleDto>,
) -> ApiResult<ApiResponse<()>> {
    svc.update_role(role_id, dto).await?;
    Ok(ApiResponse::empty_with_msg("更新成功"))
}

#[delete("/role/{role_id}")]
pub async fn delete_role(
    Component(svc): Component<SysRoleService>,
    Path(role_id): Path<i64>,
) -> ApiResult<ApiResponse<()>> {
    svc.delete_role(role_id).await?;
    Ok(ApiResponse::empty_with_msg("删除成功"))
}

#[get("/role/{role_id}/permissions")]
pub async fn get_role_permissions(
    Component(svc): Component<SysRoleService>,
    Path(role_id): Path<i64>,
) -> ApiResult<ApiResponse<RolePermissionVo>> {
    let vo = svc.get_role_permissions(role_id).await?;
    Ok(ApiResponse::ok(vo))
}

#[put("/role/{role_id}/permissions")]
pub async fn save_role_permissions(
    Component(svc): Component<SysRoleService>,
    Path(role_id): Path<i64>,
    Json(dto): Json<RolePermissionDto>,
) -> ApiResult<ApiResponse<()>> {
    svc.save_role_permissions(role_id, dto).await?;
    Ok(ApiResponse::empty_with_msg("保存成功"))
}
