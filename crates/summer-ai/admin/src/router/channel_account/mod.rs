pub mod req;
pub mod res;

use summer_auth::AdminUser;
use summer_common::error::ApiResult;
use summer_common::extractor::{Path, Query, ValidatedJson};
use summer_common::response::Json;
use summer_sea_orm::pagination::{Page, Pagination};
use summer_web::Router;
use summer_web::extractor::Component;
use summer_web::handler::TypeRouter;
use summer_web::{delete_api, get_api, post_api, put_api};

use crate::service::channel_account::ChannelAccountService;

use self::req::{ChannelAccountQuery, CreateChannelAccountReq, UpdateChannelAccountReq};
use self::res::ChannelAccountRes;

pub fn routes() -> Router {
    Router::new()
        .typed_route(list_channel_accounts)
        .typed_route(create_channel_account)
        .typed_route(update_channel_account)
        .typed_route(delete_channel_account)
}

#[get_api("/ai/channel-account/list")]
pub async fn list_channel_accounts(
    Component(svc): Component<ChannelAccountService>,
    Query(query): Query<ChannelAccountQuery>,
    pagination: Pagination,
) -> ApiResult<Json<Page<ChannelAccountRes>>> {
    let page = svc.list_accounts(query, pagination).await?;
    Ok(Json(page))
}

#[post_api("/ai/channel-account")]
pub async fn create_channel_account(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<ChannelAccountService>,
    ValidatedJson(dto): ValidatedJson<CreateChannelAccountReq>,
) -> ApiResult<Json<ChannelAccountRes>> {
    let account = svc.create_account(dto, &profile.nick_name).await?;
    Ok(Json(account))
}

#[put_api("/ai/channel-account/{id}")]
pub async fn update_channel_account(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<ChannelAccountService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdateChannelAccountReq>,
) -> ApiResult<Json<ChannelAccountRes>> {
    let account = svc.update_account(id, dto, &profile.nick_name).await?;
    Ok(Json(account))
}

#[delete_api("/ai/channel-account/{id}")]
pub async fn delete_channel_account(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<ChannelAccountService>,
    Path(id): Path<i64>,
) -> ApiResult<()> {
    svc.delete_account(id, &profile.nick_name).await?;
    Ok(())
}
