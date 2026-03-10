use crate::service::auth_service::AuthService;
use common::error::ApiResult;
use common::extractor::{ClientIp, ValidatedJson};
use common::response::Json;
use common::user_agent::UserAgentInfo;
use macros::log;
use model::dto::auth::{BizLoginDto, CustomerLoginDto, LoginDto, RefreshTokenDto};
use model::vo::auth::{DeviceSessionVo, LoginVo};
use summer_auth::{DeviceType, LoginUser};
use summer_web::axum::extract::Path;
use summer_web::axum::http::HeaderMap;
use summer_web::extractor::Component;
use summer_web::{delete_api, get_api, post_api};

#[log(module = "认证管理", action = "管理员登录", biz_type = Auth, save_params = false)]
#[post_api("/auth/login")]
pub async fn login(
    Component(svc): Component<AuthService>,
    ClientIp(client_ip): ClientIp,
    headers: HeaderMap,
    ValidatedJson(dto): ValidatedJson<LoginDto>,
) -> ApiResult<Json<LoginVo>> {
    let ua_info = UserAgentInfo::from_headers(&headers);
    let vo = svc.admin_login(dto, client_ip, ua_info).await?;
    Ok(Json(vo))
}

#[log(module = "认证管理", action = "B端登录", biz_type = Auth, save_params = false)]
#[post_api("/auth/biz/login")]
pub async fn biz_login(
    Component(svc): Component<AuthService>,
    ClientIp(client_ip): ClientIp,
    headers: HeaderMap,
    ValidatedJson(dto): ValidatedJson<BizLoginDto>,
) -> ApiResult<Json<LoginVo>> {
    let ua_info = UserAgentInfo::from_headers(&headers);
    let vo = svc.biz_login(dto, client_ip, ua_info).await?;
    Ok(Json(vo))
}

#[log(module = "认证管理", action = "C端登录", biz_type = Auth, save_params = false)]
#[post_api("/auth/customer/login")]
pub async fn customer_login(
    Component(svc): Component<AuthService>,
    ClientIp(client_ip): ClientIp,
    headers: HeaderMap,
    ValidatedJson(dto): ValidatedJson<CustomerLoginDto>,
) -> ApiResult<Json<LoginVo>> {
    let ua_info = UserAgentInfo::from_headers(&headers);
    let vo = svc.customer_login(dto, client_ip, ua_info).await?;
    Ok(Json(vo))
}

#[log(module = "认证管理", action = "退出登录", biz_type = Auth)]
#[post_api("/auth/logout")]
pub async fn logout(
    LoginUser { session }: LoginUser,
    Component(svc): Component<AuthService>,
) -> ApiResult<()> {
    svc.logout(&session.login_id, &session.device).await?;
    Ok(())
}

/// 刷新 Token
#[log(module = "认证管理", action = "刷新Token", biz_type = Auth)]
#[post_api("/auth/refresh")]
pub async fn refresh_token(
    Component(svc): Component<AuthService>,
    ValidatedJson(dto): ValidatedJson<RefreshTokenDto>,
) -> ApiResult<Json<LoginVo>> {
    let vo = svc.refresh_token(&dto.refresh_token).await?;
    Ok(Json(vo))
}

/// 登出所有设备
#[log(module = "认证管理", action = "登出所有设备", biz_type = Auth)]
#[post_api("/auth/logout/all")]
pub async fn logout_all(
    LoginUser { session }: LoginUser,
    Component(svc): Component<AuthService>,
) -> ApiResult<()> {
    svc.logout_all(&session.login_id).await?;
    Ok(())
}

/// 查看在线设备
#[log(module = "认证管理", action = "查看在线设备", biz_type = Query)]
#[get_api("/auth/sessions")]
pub async fn list_sessions(
    LoginUser { session }: LoginUser,
    Component(svc): Component<AuthService>,
) -> ApiResult<Json<Vec<DeviceSessionVo>>> {
    let sessions = svc.get_sessions(&session.login_id).await?;
    Ok(Json(sessions))
}

/// 踢下指定设备
#[log(module = "认证管理", action = "踢下设备", biz_type = Delete)]
#[delete_api("/auth/sessions/{device}")]
pub async fn kick_session(
    LoginUser { session }: LoginUser,
    Component(svc): Component<AuthService>,
    Path(device): Path<String>,
) -> ApiResult<()> {
    let device_type = DeviceType::from(device.as_str());
    svc.kick_device(&session.login_id, device_type).await?;
    Ok(())
}
