use common::error::ApiResult;
use common::extractor::{LoginIdExtractor, ValidatedJson};
use common::response::ApiResponse;
use macros::log;
use model::dto::login_log::LoginLogQueryDto;
use model::dto::user_profile::{ChangePasswordDto, UpdateProfileDto};
use model::vo::login_log::LoginLogVo;
use model::vo::user_profile::UserProfileVo;
use spring_web::axum::extract::Query;
use spring_web::extractor::Component;
use spring_web::{get, put};

use crate::plugin::pagination::{Page, Pagination};
use crate::service::login_log_service::LoginLogService;
use crate::service::sys_user_service::SysUserService;

/// 修改个人密码
#[log(module = "个人中心", action = "修改密码", biz_type = Update, save_params = false)]
#[put("/user/profile/password")]
pub async fn change_password(
    LoginIdExtractor(login_id): LoginIdExtractor,
    Component(svc): Component<SysUserService>,
    ValidatedJson(dto): ValidatedJson<ChangePasswordDto>,
) -> ApiResult<ApiResponse<()>> {
    svc.change_password(&login_id, dto).await?;
    Ok(ApiResponse::empty_with_msg("密码修改成功"))
}

/// 更新个人信息
#[log(module = "个人中心", action = "更新个人信息", biz_type = Update)]
#[put("/user/profile")]
pub async fn update_profile(
    LoginIdExtractor(login_id): LoginIdExtractor,
    Component(svc): Component<SysUserService>,
    ValidatedJson(dto): ValidatedJson<UpdateProfileDto>,
) -> ApiResult<ApiResponse<UserProfileVo>> {
    let profile = svc.update_profile(&login_id, dto).await?;
    Ok(ApiResponse::ok_with_msg(profile, "个人信息更新成功"))
}

/// 获取登录日志
#[log(module = "个人中心", action = "查询登录日志", biz_type = Query)]
#[get("/user/profile/login-logs")]
pub async fn get_login_logs(
    LoginIdExtractor(login_id): LoginIdExtractor,
    Component(svc): Component<LoginLogService>,
    Query(query): Query<LoginLogQueryDto>,
    pagination: Pagination,
) -> ApiResult<ApiResponse<Page<LoginLogVo>>> {
    let logs = svc.get_user_login_logs(&login_id, query, pagination).await?;
    Ok(ApiResponse::ok(logs))
}
