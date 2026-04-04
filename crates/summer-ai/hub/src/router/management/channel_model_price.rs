use summer_auth::AdminUser;
use summer_common::error::ApiResult;
use summer_common::extractor::{Path, Query, ValidatedJson};
use summer_common::response::Json;
use summer_sea_orm::pagination::{Page, Pagination};
use summer_web::extractor::Component;
use summer_web::{delete_api, get_api, post_api, put_api};

use summer_ai_model::dto::channel_model_price::{
    CreateChannelModelPriceDto, QueryChannelModelPriceDto, UpdateChannelModelPriceDto,
};
use summer_ai_model::vo::channel_model_price::{ChannelModelPriceDetailVo, ChannelModelPriceVo};

use crate::service::channel_model_price::ChannelModelPriceService;

#[get_api("/ai/channel-model-price")]
pub async fn list_prices(
    Component(svc): Component<ChannelModelPriceService>,
    Query(query): Query<QueryChannelModelPriceDto>,
    pagination: Pagination,
) -> ApiResult<Json<Page<ChannelModelPriceVo>>> {
    let page = svc.list_prices(query, pagination).await?;
    Ok(Json(page))
}

#[get_api("/ai/channel-model-price/{id}")]
pub async fn get_price(
    Component(svc): Component<ChannelModelPriceService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<ChannelModelPriceDetailVo>> {
    let detail = svc.get_price_detail(id).await?;
    Ok(Json(detail))
}

#[post_api("/ai/channel-model-price")]
pub async fn create_price(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<ChannelModelPriceService>,
    ValidatedJson(dto): ValidatedJson<CreateChannelModelPriceDto>,
) -> ApiResult<Json<ChannelModelPriceVo>> {
    let vo = svc.create_price(dto, &profile.nick_name).await?;
    Ok(Json(vo))
}

#[put_api("/ai/channel-model-price/{id}")]
pub async fn update_price(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<ChannelModelPriceService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdateChannelModelPriceDto>,
) -> ApiResult<Json<ChannelModelPriceVo>> {
    let vo = svc.update_price(id, dto, &profile.nick_name).await?;
    Ok(Json(vo))
}

#[delete_api("/ai/channel-model-price/{id}")]
pub async fn delete_price(
    Component(svc): Component<ChannelModelPriceService>,
    Path(id): Path<i64>,
) -> ApiResult<()> {
    svc.delete_price(id).await?;
    Ok(())
}
