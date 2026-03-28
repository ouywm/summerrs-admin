use summer_auth::AdminUser;
use summer_common::error::ApiResult;
use summer_common::extractor::{Path, Query, ValidatedJson};
use summer_common::response::Json;
use summer_web::extractor::Component;
use summer_web::{delete_api, get_api, post_api, put_api};

use summer_ai_model::dto::channel::{CreateChannelDto, QueryChannelDto, UpdateChannelDto};
use summer_ai_model::vo::channel::{ChannelDetailVo, ChannelTestVo, ChannelVo};

use crate::relay::http_client::UpstreamHttpClient;
use crate::service::channel::ChannelService;
use summer_sea_orm::pagination::{Page, Pagination};

#[get_api("/ai/channel")]
pub async fn list_channels(
    _admin: AdminUser,
    Component(svc): Component<ChannelService>,
    Query(query): Query<QueryChannelDto>,
    pagination: Pagination,
) -> ApiResult<Json<Page<ChannelVo>>> {
    let page = svc.list_channels(query, pagination).await?;
    Ok(Json(page))
}

#[get_api("/ai/channel/{id}")]
pub async fn get_channel(
    _admin: AdminUser,
    Component(svc): Component<ChannelService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<ChannelDetailVo>> {
    let channel = svc.get_channel(id).await?;
    Ok(Json(channel))
}

#[post_api("/ai/channel")]
pub async fn create_channel(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<ChannelService>,
    ValidatedJson(dto): ValidatedJson<CreateChannelDto>,
) -> ApiResult<Json<ChannelDetailVo>> {
    let channel = svc.create_channel(dto, &profile.nick_name).await?;
    Ok(Json(channel))
}

#[put_api("/ai/channel/{id}")]
pub async fn update_channel(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<ChannelService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdateChannelDto>,
) -> ApiResult<Json<ChannelDetailVo>> {
    let channel = svc.update_channel(id, dto, &profile.nick_name).await?;
    Ok(Json(channel))
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

#[post_api("/ai/channel/{id}/test")]
pub async fn test_channel(
    _admin: AdminUser,
    Component(svc): Component<ChannelService>,
    Component(http_client): Component<UpstreamHttpClient>,
    Path(id): Path<i64>,
) -> ApiResult<Json<ChannelTestVo>> {
    let result = svc.test_channel(id, http_client.client()).await?;
    Ok(Json(result))
}
