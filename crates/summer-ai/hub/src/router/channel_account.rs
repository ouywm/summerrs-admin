use summer_auth::AdminUser;
use summer_common::error::ApiResult;
use summer_common::extractor::{Path, Query, ValidatedJson};
use summer_common::response::Json;
use summer_sea_orm::pagination::{Page, Pagination};
use summer_web::extractor::Component;
use summer_web::{delete_api, get_api, post_api, put_api};

use summer_ai_model::dto::channel_account::{
    CreateChannelAccountDto, QueryChannelAccountDto, UpdateChannelAccountDto,
};
use summer_ai_model::vo::channel_account::ChannelAccountVo;

use crate::service::channel::ChannelService;

#[get_api("/ai/channel-account")]
pub async fn list_channel_accounts(
    Component(svc): Component<ChannelService>,
    Query(query): Query<QueryChannelAccountDto>,
    pagination: Pagination,
) -> ApiResult<Json<Page<ChannelAccountVo>>> {
    let page = svc.list_accounts(query, pagination).await?;
    Ok(Json(page))
}

#[post_api("/ai/channel-account")]
pub async fn create_channel_account(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<ChannelService>,
    ValidatedJson(dto): ValidatedJson<CreateChannelAccountDto>,
) -> ApiResult<()> {
    svc.create_account(dto, &profile.nick_name).await?;
    Ok(())
}

#[put_api("/ai/channel-account/{id}")]
pub async fn update_channel_account(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<ChannelService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdateChannelAccountDto>,
) -> ApiResult<()> {
    svc.update_account(id, dto, &profile.nick_name).await?;
    Ok(())
}

#[delete_api("/ai/channel-account/{id}")]
pub async fn delete_channel_account(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<ChannelService>,
    Path(id): Path<i64>,
) -> ApiResult<()> {
    svc.delete_account(id, &profile.nick_name).await?;
    Ok(())
}
