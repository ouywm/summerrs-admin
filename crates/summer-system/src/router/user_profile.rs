use summer_admin_macros::log;
use summer_auth::AdminUser;
use summer_common::error::ApiResult;
use summer_common::extractor::{Query, ValidatedJson};
use summer_common::response::Json;
use summer_system_model::dto::login_log::LoginLogQueryDto;
use summer_system_model::dto::user_profile::{ChangePasswordDto, UpdateProfileDto};
use summer_system_model::vo::login_log::LoginLogVo;
use summer_system_model::vo::user_profile::UserProfileVo;
use summer_web::extractor::Component;
use summer_web::{get_api, put_api};

use crate::service::login_log_service::LoginLogService;
use crate::service::sys_user_service::SysUserService;
use summer_sea_orm::pagination::{Page, Pagination};

/// 修改个人密码
#[log(module = "个人中心", action = "修改密码", biz_type = Update, save_params = false)]
#[put_api("/user/profile/password")]
pub async fn change_password(
    AdminUser { login_id, .. }: AdminUser,
    Component(svc): Component<SysUserService>,
    ValidatedJson(dto): ValidatedJson<ChangePasswordDto>,
) -> ApiResult<()> {
    svc.change_password(&login_id, dto).await?;
    Ok(())
}

/// 更新个人信息
#[log(module = "个人中心", action = "更新个人信息", biz_type = Update)]
#[put_api("/user/profile")]
pub async fn update_profile(
    AdminUser { login_id, .. }: AdminUser,
    Component(svc): Component<SysUserService>,
    ValidatedJson(dto): ValidatedJson<UpdateProfileDto>,
) -> ApiResult<Json<UserProfileVo>> {
    let profile = svc.update_profile(&login_id, dto).await?;
    Ok(Json(profile))
}

/// 获取登录日志
#[log(module = "个人中心", action = "查询登录日志", biz_type = Query)]
#[get_api("/user/profile/login-logs")]
pub async fn get_login_logs(
    AdminUser { login_id, .. }: AdminUser,
    Component(svc): Component<LoginLogService>,
    Query(query): Query<LoginLogQueryDto>,
    pagination: Pagination,
) -> ApiResult<Json<Page<LoginLogVo>>> {
    let logs = svc
        .get_user_login_logs(&login_id, query, pagination)
        .await?;
    Ok(Json(logs))
}
