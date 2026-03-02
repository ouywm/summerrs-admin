use crate::service::auth_service::AuthService;
use common::error::ApiResult;
use common::extractor::{ClientIp, LoginIdExtractor, ValidatedJson};
use common::response::ApiResponse;
use common::user_agent::UserAgentInfo;
use macros::log;
use model::dto::auth::LoginDto;
use model::vo::auth::LoginVo;
use spring_sa_token::sa_ignore;
use spring_web::axum::http::HeaderMap;
use spring_web::extractor::Component;
use spring_web::{post, post_api};

#[log(module = "认证管理", action = "用户登录", biz_type = Auth, save_params = false)]
#[sa_ignore]
#[post_api("/auth/login")]
pub async fn login(
    Component(svc): Component<AuthService>,
    ClientIp(client_ip): ClientIp,
    headers: HeaderMap,
    ValidatedJson(dto): ValidatedJson<LoginDto>,
) -> ApiResult<ApiResponse<LoginVo>> {
    let ua_info = UserAgentInfo::from_headers(&headers);
    let vo = svc.login(dto, client_ip, ua_info).await?;
    Ok(ApiResponse::ok(vo))
}

#[log(module = "认证管理", action = "退出登录", biz_type = Auth)]
#[post("/auth/logout")]
pub async fn logout(
    LoginIdExtractor(login_id): LoginIdExtractor,
    Component(svc): Component<AuthService>,
) -> ApiResult<ApiResponse<()>> {
    svc.logout(&login_id).await?;
    Ok(ApiResponse::ok(()))
}
