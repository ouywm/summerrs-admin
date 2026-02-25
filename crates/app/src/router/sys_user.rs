use anyhow::Context;
use sea_orm::*;
use spring_sea_orm::DbConn;
use spring_web::axum::Json;
use spring_web::error::{KnownWebError, Result};
use spring_web::extractor::{Component, Path};
use spring_web::{get, post, put, delete};

use crate::service::sys_user::SysUserService;
use model::dto::sys_user::ResetPasswordDto;
use model::entity::sys_user::Entity as SysUser;
use model::vo::sys_user::UserVo;

// ==================== 简单 CRUD（直接操作 ORM） ====================

/// 获取用户列表
#[get("/api/sys-user/list")]
pub async fn list(Component(db): Component<DbConn>) -> Result<Json<Vec<UserVo>>> {
    let users = SysUser::find()
        .all(&db)
        .await
        .context("查询用户列表失败")?;
    Ok(Json(users.into_iter().map(UserVo::from).collect()))
}

/// 根据 ID 查询用户
#[get("/api/sys-user/{id}")]
pub async fn get_by_id(
    Component(db): Component<DbConn>,
    Path(id): Path<i64>,
) -> Result<Json<UserVo>> {
    let user = SysUser::find_by_id(id)
        .one(&db)
        .await
        .context("查询用户失败")?
        .ok_or_else(|| KnownWebError::not_found("用户不存在"))?;
    Ok(Json(user.into()))
}

/// 删除用户
#[delete("/api/sys-user/{id}")]
pub async fn delete(
    Component(db): Component<DbConn>,
    Path(id): Path<i64>,
) -> Result<Json<bool>> {
    SysUser::delete_by_id(id)
        .exec(&db)
        .await
        .context("删除用户失败")?;
    Ok(Json(true))
}

// ==================== 复杂业务（委托 Service） ====================

/// 创建用户（含用户名/邮箱查重、密码加密等）
#[post("/api/sys-user")]
pub async fn create(
    Component(svc): Component<SysUserService>,
    Json(dto): Json<model::dto::sys_user::CreateUserDto>,
) -> Result<Json<UserVo>> {
    let user = svc.create_user(dto).await.map_err(|e| {
        let msg = e.to_string();
        if msg.contains("已存在") || msg.contains("已被注册") {
            KnownWebError::bad_request(msg)
        } else {
            KnownWebError::internal_server_error(msg)
        }
    })?;
    Ok(Json(user.into()))
}

/// 重置密码
#[put("/api/sys-user/{id}/reset-password")]
pub async fn reset_password(
    Component(svc): Component<SysUserService>,
    Path(id): Path<i64>,
    Json(dto): Json<ResetPasswordDto>,
) -> Result<Json<bool>> {
    svc.reset_password(id, dto.new_password).await.map_err(|e| {
        let msg = e.to_string();
        if msg.contains("不存在") {
            KnownWebError::not_found(msg)
        } else {
            KnownWebError::internal_server_error(msg)
        }
    })?;
    Ok(Json(true))
}
