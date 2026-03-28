use summer_auth::AdminUser;
use summer_common::error::ApiResult;
use summer_common::extractor::{Path, Query, ValidatedJson};
use summer_common::response::Json;
use summer_web::extractor::Component;
use summer_web::{delete_api, get_api, post_api, put_api};

use summer_ai_model::dto::token::{
    CreateTokenDto, QueryTokenDto, RechargeTokenDto, UpdateTokenDto,
};
use summer_ai_model::vo::token::{TokenCreatedVo, TokenVo};

use crate::service::token::TokenService;
use summer_sea_orm::pagination::{Page, Pagination};

#[get_api("/ai/token")]
pub async fn list_tokens(
    _admin: AdminUser,
    Component(svc): Component<TokenService>,
    Query(query): Query<QueryTokenDto>,
    pagination: Pagination,
) -> ApiResult<Json<Page<TokenVo>>> {
    let page = svc.list_tokens(query, pagination).await?;
    Ok(Json(page))
}

#[get_api("/ai/token/{id}")]
pub async fn get_token(
    _admin: AdminUser,
    Component(svc): Component<TokenService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<TokenVo>> {
    let token = svc.get_token(id).await?;
    Ok(Json(token))
}

#[post_api("/ai/token")]
pub async fn create_token(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<TokenService>,
    ValidatedJson(dto): ValidatedJson<CreateTokenDto>,
) -> ApiResult<Json<TokenCreatedVo>> {
    let token = svc.create_token(dto, &profile.nick_name).await?;
    Ok(Json(token))
}

#[put_api("/ai/token/{id}")]
pub async fn update_token(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<TokenService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdateTokenDto>,
) -> ApiResult<()> {
    svc.update_token(id, dto, &profile.nick_name).await?;
    Ok(())
}

#[delete_api("/ai/token/{id}")]
pub async fn delete_token(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<TokenService>,
    Path(id): Path<i64>,
) -> ApiResult<()> {
    svc.disable_token(id, &profile.nick_name).await?;
    Ok(())
}

#[post_api("/ai/token/{id}/recharge")]
pub async fn recharge_token(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<TokenService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<RechargeTokenDto>,
) -> ApiResult<()> {
    svc.recharge_token(id, dto, &profile.nick_name).await?;
    Ok(())
}
