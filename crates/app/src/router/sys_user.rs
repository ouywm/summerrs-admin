use common::error::ApiResult;
use common::extractor::ValidatedJson;
use common::response::{ApiResponse, PageResponse};
use model::dto::sys_user::{CreateUserDto, UpdateUserDto, UserQueryDto};
use model::vo::sys_user::{UserDetailVo, UserInfoVo, UserVo};
use spring_sa_token::LoginIdExtractor;
use spring_web::axum::extract::Path;
use spring_web::extractor::Component;
use spring_web::{delete, get, post, put};

use crate::service::sys_user_service::SysUserService;

#[get("/user/info")]
pub async fn get_user_info(
    LoginIdExtractor(login_id): LoginIdExtractor,
    Component(svc): Component<SysUserService>,
) -> ApiResult<ApiResponse<UserInfoVo>> {
    let vo = svc.get_user_info(&login_id).await?;
    Ok(ApiResponse::ok(vo))
}

#[get("/user/{id}")]
pub async fn get_user_detail(
    Component(svc): Component<SysUserService>,
    Path(id): Path<i64>,
) -> ApiResult<ApiResponse<UserDetailVo>> {
    let vo = svc.get_user_detail(id).await?;
    Ok(ApiResponse::ok(vo))
}

#[get("/user/list")]
pub async fn list_users(
    Component(svc): Component<SysUserService>,
    spring_web::axum::extract::Query(query): spring_web::axum::extract::Query<UserQueryDto>,
) -> ApiResult<ApiResponse<PageResponse<UserVo>>> {
    let vo = svc.list_users(query).await?;
    Ok(ApiResponse::ok(vo))
}

#[post("/user")]
pub async fn create_user(
    LoginIdExtractor(login_id): LoginIdExtractor,
    Component(svc): Component<SysUserService>,
    ValidatedJson(dto): ValidatedJson<CreateUserDto>,
) -> ApiResult<ApiResponse<()>> {
    svc.create_user(dto, &login_id).await?;
    Ok(ApiResponse::empty_with_msg("创建成功"))
}

#[put("/user/{id}")]
pub async fn update_user(
    LoginIdExtractor(login_id): LoginIdExtractor,
    Component(svc): Component<SysUserService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdateUserDto>,
) -> ApiResult<ApiResponse<()>> {
    svc.update_user(id, dto, &login_id).await?;
    Ok(ApiResponse::empty_with_msg("更新成功"))
}

#[delete("/user/{id}")]
pub async fn delete_user(
    Component(svc): Component<SysUserService>,
    Path(id): Path<i64>,
) -> ApiResult<ApiResponse<()>> {
    svc.delete_user(id).await?;
    Ok(ApiResponse::empty_with_msg("删除成功"))
}
