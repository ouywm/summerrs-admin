use anyhow::Context;
use sea_orm::*;
use spring_sea_orm::DbConn;
use spring_web::axum::Json;
use spring_web::extractor::{Component, Path};
use spring_web::{delete, get, post, put};

use common::error::{ApiErrors, ApiResult};
use common::response::ApiResponse;
use crate::service::sys_user::SysUserService;
use model::dto::sys_user::ResetPasswordDto;
use model::entity::sys_user::Entity as SysUser;
use model::vo::sys_user::UserVo;

// ==================== 简单 CRUD（直接操作 ORM） ====================

/// 获取用户列表
#[get("/api/sys-user/list")]
pub async fn list(Component(db): Component<DbConn>) -> ApiResult<ApiResponse<Vec<UserVo>>> {
    let users = SysUser::find()
        .all(&db)
        .await
        .context("查询用户列表失败")?;
    Ok(ApiResponse::ok(users.into_iter().map(UserVo::from).collect()))
}

/// 根据 ID 查询用户
#[get("/api/sys-user/{id}")]
pub async fn get_by_id(
    Component(db): Component<DbConn>,
    Path(id): Path<i64>,
) -> ApiResult<ApiResponse<UserVo>> {
    let user = SysUser::find_by_id(id)
        .one(&db)
        .await
        .context("查询用户失败")?
        .ok_or_else(|| ApiErrors::NotFound("用户不存在".to_string()))?;
    Ok(ApiResponse::ok(user.into()))
}

/// 删除用户
#[delete("/api/sys-user/{id}")]
pub async fn delete(
    Component(db): Component<DbConn>,
    Path(id): Path<i64>,
) -> ApiResult<ApiResponse<()>> {
    SysUser::delete_by_id(id)
        .exec(&db)
        .await
        .context("删除用户失败")?;
    Ok(ApiResponse::ok(()))
}

// ==================== 复杂业务（委托 Service） ====================

/// 创建用户（含用户名/邮箱查重、密码加密等）
#[post("/api/sys-user")]
pub async fn create(
    Component(svc): Component<SysUserService>,
    Json(dto): Json<model::dto::sys_user::CreateUserDto>,
) -> ApiResult<ApiResponse<UserVo>> {
    let user = svc.create_user(dto).await?;
    Ok(ApiResponse::ok(user.into()))
}

/// 重置密码
#[put("/api/sys-user/{id}/reset-password")]
pub async fn reset_password(
    Component(svc): Component<SysUserService>,
    Path(id): Path<i64>,
    Json(dto): Json<ResetPasswordDto>,
) -> ApiResult<ApiResponse<()>> {
    svc.reset_password(id, dto.new_password).await?;
    Ok(ApiResponse::ok(()))
}