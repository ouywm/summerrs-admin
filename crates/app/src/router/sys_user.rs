use common::error::ApiResult;
use common::extractor::{LoginIdExtractor, Path, Query, ValidatedJson};
use common::response::ApiResponse;
use macros::log;
use model::dto::sys_user::{CreateUserDto, ResetPasswordDto, UpdateUserDto, UserQueryDto};
use model::vo::sys_user::{UserDetailVo, UserInfoVo, UserVo};
use summer_web::extractor::Component;
use summer_web::{delete_api, get_api, post_api, put_api};

use crate::plugin::sea_orm::pagination::{Page, Pagination};
use crate::service::sys_user_service::SysUserService;

#[log(module = "用户管理", action = "获取用户信息", biz_type = Query)]
#[get_api("/user/info")]
pub async fn get_user_info(
    LoginIdExtractor(login_id): LoginIdExtractor,
    Component(svc): Component<SysUserService>,
) -> ApiResult<ApiResponse<UserInfoVo>> {
    let vo = svc.get_user_info(&login_id).await?;
    Ok(ApiResponse::ok(vo))
}

#[log(module = "用户管理", action = "获取用户详情", biz_type = Query)]
#[get_api("/user/{id}")]
pub async fn get_user_detail(
    Component(svc): Component<SysUserService>,
    Path(id): Path<i64>,
) -> ApiResult<ApiResponse<UserDetailVo>> {
    let vo = svc.get_user_detail(id).await?;
    Ok(ApiResponse::ok(vo))
}

#[log(module = "用户管理", action = "查询用户列表", biz_type = Query)]
#[get_api("/user/list")]
pub async fn list_users(
    Component(svc): Component<SysUserService>,
    Query(query): Query<UserQueryDto>,
    pagination: Pagination,
) -> ApiResult<ApiResponse<Page<UserVo>>> {
    let vo = svc.list_users(query, pagination).await?;
    Ok(ApiResponse::ok(vo))
}

#[log(module = "用户管理", action = "创建用户", biz_type = Create)]
#[post_api("/user")]
pub async fn create_user(
    LoginIdExtractor(login_id): LoginIdExtractor,
    Component(svc): Component<SysUserService>,
    ValidatedJson(dto): ValidatedJson<CreateUserDto>,
) -> ApiResult<ApiResponse<()>> {
    svc.create_user(dto, &login_id).await?;
    Ok(ApiResponse::empty_with_msg("创建成功"))
}

#[log(module = "用户管理", action = "更新用户", biz_type = Update)]
#[put_api("/user/{id}")]
pub async fn update_user(
    LoginIdExtractor(login_id): LoginIdExtractor,
    Component(svc): Component<SysUserService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdateUserDto>,
) -> ApiResult<ApiResponse<()>> {
    svc.update_user(id, dto, &login_id).await?;
    Ok(ApiResponse::empty_with_msg("更新成功"))
}

#[log(module = "用户管理", action = "删除用户", biz_type = Delete)]
#[delete_api("/user/{id}")]
pub async fn delete_user(
    Component(svc): Component<SysUserService>,
    Path(id): Path<i64>,
) -> ApiResult<ApiResponse<()>> {
    svc.delete_user(id).await?;
    Ok(ApiResponse::empty_with_msg("删除成功"))
}

#[log(module = "用户管理", action = "重置用户密码", biz_type = Update, save_params = false)]
#[put_api("/user/{id}/reset-password")]
pub async fn reset_user_password(
    Component(svc): Component<SysUserService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<ResetPasswordDto>,
) -> ApiResult<ApiResponse<()>> {
    svc.reset_password(id, dto).await?;
    Ok(ApiResponse::empty_with_msg("密码重置成功"))
}
