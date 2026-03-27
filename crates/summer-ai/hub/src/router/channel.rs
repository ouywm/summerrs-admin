use summer_auth::AdminUser;
use summer_common::error::ApiResult;
use summer_common::extractor::{Path, Query, ValidatedJson};
use summer_common::response::Json;
use summer_sea_orm::pagination::{Page, Pagination};
use summer_web::extractor::Component;
use summer_web::{delete_api, get_api, post_api, put_api};

use summer_ai_model::dto::channel::{
    CreateChannelDto, QueryChannelDto, TestChannelDto, UpdateChannelDto,
};
use summer_ai_model::vo::channel::{ChannelDetailVo, ChannelListVo, ChannelTestVo};

use crate::service::channel::ChannelService;

#[get_api("/ai/channel")]
pub async fn list_channels(
    Component(svc): Component<ChannelService>,
    Query(query): Query<QueryChannelDto>,
    pagination: Pagination,
) -> ApiResult<Json<Page<ChannelListVo>>> {
    let page = svc.list_channels(query, pagination).await?;
    Ok(Json(page))
}

#[get_api("/ai/channel/{id}")]
pub async fn get_channel(
    Component(svc): Component<ChannelService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<ChannelDetailVo>> {
    let vo = svc.get_channel(id).await?;
    Ok(Json(vo))
}

#[post_api("/ai/channel")]
pub async fn create_channel(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<ChannelService>,
    ValidatedJson(dto): ValidatedJson<CreateChannelDto>,
) -> ApiResult<()> {
    svc.create_channel(dto, &profile.nick_name).await?;
    Ok(())
}

#[put_api("/ai/channel/{id}")]
pub async fn update_channel(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<ChannelService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdateChannelDto>,
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

#[post_api("/ai/channel/{id}/test")]
pub async fn test_channel(
    Component(svc): Component<ChannelService>,
    Path(id): Path<i64>,
    Query(query): Query<TestChannelDto>,
) -> ApiResult<Json<ChannelTestVo>> {
    let vo = svc.test_channel(id, query.endpoint_scope).await?;
    Ok(Json(vo))
}
