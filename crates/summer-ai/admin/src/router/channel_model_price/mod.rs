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

use crate::service::channel_model_price::ChannelModelPriceService;

use self::req::{ChannelModelPriceQuery, CreateChannelModelPriceReq, UpdateChannelModelPriceReq};
use self::res::{ChannelModelPriceDetailRes, ChannelModelPriceRes};

pub fn routes() -> Router {
    Router::new()
        .typed_route(list_prices)
        .typed_route(get_price)
        .typed_route(create_price)
        .typed_route(update_price)
        .typed_route(delete_price)
}

#[get_api("/ai/channel-model-price/list")]
pub async fn list_prices(
    Component(svc): Component<ChannelModelPriceService>,
    Query(query): Query<ChannelModelPriceQuery>,
    pagination: Pagination,
) -> ApiResult<Json<Page<ChannelModelPriceRes>>> {
    let page = svc.list_prices(query, pagination).await?;
    Ok(Json(page))
}

#[get_api("/ai/channel-model-price/{id}")]
pub async fn get_price(
    Component(svc): Component<ChannelModelPriceService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<ChannelModelPriceDetailRes>> {
    let detail = svc.get_price_detail(id).await?;
    Ok(Json(detail))
}

#[post_api("/ai/channel-model-price")]
pub async fn create_price(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<ChannelModelPriceService>,
    ValidatedJson(dto): ValidatedJson<CreateChannelModelPriceReq>,
) -> ApiResult<Json<ChannelModelPriceRes>> {
    let price = svc.create_price(dto, &profile.nick_name).await?;
    Ok(Json(price))
}

#[put_api("/ai/channel-model-price/{id}")]
pub async fn update_price(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<ChannelModelPriceService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdateChannelModelPriceReq>,
) -> ApiResult<Json<ChannelModelPriceRes>> {
    let price = svc.update_price(id, dto, &profile.nick_name).await?;
    Ok(Json(price))
}

#[delete_api("/ai/channel-model-price/{id}")]
pub async fn delete_price(
    Component(svc): Component<ChannelModelPriceService>,
    Path(id): Path<i64>,
) -> ApiResult<()> {
    svc.delete_price(id).await?;
    Ok(())
}
