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

use crate::service::channel::ChannelService;

use self::req::{ChannelQuery, CreateChannelReq, UpdateChannelReq};
use self::res::{ChannelDetailRes, ChannelListRes};

pub fn routes() -> Router {
    Router::new()
        .typed_route(list_channels)
        .typed_route(get_channel)
        .typed_route(create_channel)
        .typed_route(update_channel)
        .typed_route(delete_channel)
}

#[get_api("/ai/channel/list")]
pub async fn list_channels(
    Component(svc): Component<ChannelService>,
    Query(query): Query<ChannelQuery>,
    pagination: Pagination,
) -> ApiResult<Json<Page<ChannelListRes>>> {
    let page = svc.list_channels(query, pagination).await?;
    Ok(Json(page))
}

#[get_api("/ai/channel/{id}")]
pub async fn get_channel(
    Component(svc): Component<ChannelService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<ChannelDetailRes>> {
    let channel = svc.get_channel(id).await?;
    Ok(Json(channel))
}

#[post_api("/ai/channel")]
pub async fn create_channel(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<ChannelService>,
    ValidatedJson(dto): ValidatedJson<CreateChannelReq>,
) -> ApiResult<()> {
    svc.create_channel(dto, &profile.nick_name).await?;
    Ok(())
}

#[put_api("/ai/channel/{id}")]
pub async fn update_channel(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<ChannelService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdateChannelReq>,
) -> ApiResult<()> {
    svc.update_channel(id, dto, &profile.nick_name).await?;
    Ok(())
}

#[delete_api("/ai/channel/{id}")]
pub async fn delete_channel(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<ChannelService>,
    Path(id): Path<i64>,
) -> ApiResult<()> {
    svc.delete_channel(id, &profile.nick_name).await?;
    Ok(())
}
