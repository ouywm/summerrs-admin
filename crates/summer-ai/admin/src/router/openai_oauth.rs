use summer_admin_macros::log;
use summer_ai_model::dto::openai_oauth::{
    ExchangeOpenAiOAuthCodeDto, GenerateOpenAiOAuthAuthUrlDto, RefreshOpenAiOAuthTokenDto,
};
use summer_ai_model::vo::openai_oauth::{
    OpenAiOAuthAuthUrlVo, OpenAiOAuthExchangeVo, OpenAiOAuthRefreshVo,
};
use summer_auth::LoginUser;
use summer_common::error::ApiResult;
use summer_common::extractor::ValidatedJson;
use summer_common::response::Json;
use summer_web::Router;
use summer_web::extractor::Component;
use summer_web::handler::TypeRouter;
use summer_web::post_api;

use crate::service::openai_oauth_service::OpenAiOAuthService;

#[log(
    module = "ai/OpenAI OAuth",
    action = "生成授权地址",
    biz_type = Auth,
    save_response = false
)]
#[post_api("/openai-oauth/auth-url")]
pub async fn generate_auth_url(
    Component(svc): Component<OpenAiOAuthService>,
    ValidatedJson(dto): ValidatedJson<GenerateOpenAiOAuthAuthUrlDto>,
) -> ApiResult<Json<OpenAiOAuthAuthUrlVo>> {
    let vo = svc.generate_auth_url(dto).await?;
    Ok(Json(vo))
}

#[log(
    module = "ai/OpenAI OAuth",
    action = "交换授权码",
    biz_type = Auth,
    save_params = false
)]
#[post_api("/openai-oauth/exchange")]
pub async fn exchange_code(
    LoginUser { profile, .. }: LoginUser,
    Component(svc): Component<OpenAiOAuthService>,
    ValidatedJson(dto): ValidatedJson<ExchangeOpenAiOAuthCodeDto>,
) -> ApiResult<Json<OpenAiOAuthExchangeVo>> {
    let vo = svc.exchange_code(dto, &profile.nick_name).await?;
    Ok(Json(vo))
}

#[log(
    module = "ai/OpenAI OAuth",
    action = "刷新OAuth令牌",
    biz_type = Auth,
    save_params = false
)]
#[post_api("/openai-oauth/refresh")]
pub async fn refresh_token(
    LoginUser { profile, .. }: LoginUser,
    Component(svc): Component<OpenAiOAuthService>,
    ValidatedJson(dto): ValidatedJson<RefreshOpenAiOAuthTokenDto>,
) -> ApiResult<Json<OpenAiOAuthRefreshVo>> {
    let vo = svc.refresh_token(dto, &profile.nick_name).await?;
    Ok(Json(vo))
}

pub fn routes(router: Router) -> Router {
    router
        .typed_route(generate_auth_url)
        .typed_route(exchange_code)
        .typed_route(refresh_token)
}
