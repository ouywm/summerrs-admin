use common::error::ApiResult;
use common::response::ApiResponse;
use model::dto::auth::LoginDto;
use model::vo::auth::LoginVo;
use spring_sa_token::sa_ignore;
use spring_sa_token::LoginIdExtractor;
use spring_web::axum::Json;
use spring_web::extractor::Component;
use spring_web::post;

use crate::service::auth_service::AuthService;

#[sa_ignore]
#[post("/auth/login")]
pub async fn login(
    Component(svc): Component<AuthService>,
    Json(dto): Json<LoginDto>,
) -> ApiResult<ApiResponse<LoginVo>> {
    let vo = svc.login(dto).await?;
    Ok(ApiResponse::ok(vo))
}

#[post("/auth/logout")]
pub async fn logout(
    LoginIdExtractor(login_id): LoginIdExtractor,
    Component(svc): Component<AuthService>,
) -> ApiResult<ApiResponse<()>> {
    svc.logout(&login_id).await?;
    Ok(ApiResponse::ok(()))
}
