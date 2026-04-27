use crate::service::channel_model_price_service::ChannelModelPriceService;
use summer_admin_macros::log;
use summer_ai_model::dto::channel_model_price::{
    ChannelModelPriceQueryDto, CreateChannelModelPriceDto, UpdateChannelModelPriceDto,
};
use summer_ai_model::vo::channel_model_price::{
    ChannelModelPriceDetailVo, ChannelModelPriceVersionVo, ChannelModelPriceVo,
};
use summer_auth::LoginUser;
use summer_common::error::ApiResult;
use summer_common::extractor::{Path, Query, ValidatedJson};
use summer_common::response::Json;
use summer_sea_orm::pagination::{Page, Pagination};
use summer_web::Router;
use summer_web::extractor::Component;
use summer_web::handler::TypeRouter;
use summer_web::{delete_api, get_api, post_api, put_api};

#[log(module = "ai/渠道模型价格管理", action = "查询价格列表", biz_type = Query)]
#[get_api("/channel-model-price/list")]
pub async fn list(
    Component(svc): Component<ChannelModelPriceService>,
    Query(query): Query<ChannelModelPriceQueryDto>,
    pagination: Pagination,
) -> ApiResult<Json<Page<ChannelModelPriceVo>>> {
    let page = svc.list(query, pagination).await?;
    Ok(Json(page))
}

#[log(module = "ai/渠道模型价格管理", action = "查询价格详情", biz_type = Query)]
#[get_api("/channel-model-price/{id}")]
pub async fn detail(
    Component(svc): Component<ChannelModelPriceService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<ChannelModelPriceDetailVo>> {
    let item = svc.detail(id).await?;
    Ok(Json(item))
}

#[log(module = "ai/渠道模型价格管理", action = "查询价格版本列表", biz_type = Query)]
#[get_api("/channel-model-price/{id}/versions")]
pub async fn versions(
    Component(svc): Component<ChannelModelPriceService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<Vec<ChannelModelPriceVersionVo>>> {
    let items = svc.list_versions(id).await?;
    Ok(Json(items))
}

#[log(module = "ai/渠道模型价格管理", action = "创建价格配置", biz_type = Create)]
#[post_api("/channel-model-price")]
pub async fn create(
    LoginUser { profile, .. }: LoginUser,
    Component(svc): Component<ChannelModelPriceService>,
    ValidatedJson(dto): ValidatedJson<CreateChannelModelPriceDto>,
) -> ApiResult<()> {
    svc.create(dto, &profile.nick_name).await?;
    Ok(())
}

#[log(module = "ai/渠道模型价格管理", action = "更新价格配置", biz_type = Update)]
#[put_api("/channel-model-price/{id}")]
pub async fn update(
    LoginUser { profile, .. }: LoginUser,
    Component(svc): Component<ChannelModelPriceService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdateChannelModelPriceDto>,
) -> ApiResult<()> {
    svc.update(id, dto, &profile.nick_name).await?;
    Ok(())
}

#[log(module = "ai/渠道模型价格管理", action = "删除价格配置", biz_type = Delete)]
#[delete_api("/channel-model-price/{id}")]
pub async fn delete(
    Component(svc): Component<ChannelModelPriceService>,
    Path(id): Path<i64>,
) -> ApiResult<()> {
    svc.delete(id).await?;
    Ok(())
}

pub fn routes(router: Router) -> Router {
    router
        .typed_route(list)
        .typed_route(detail)
        .typed_route(versions)
        .typed_route(create)
        .typed_route(update)
        .typed_route(delete)
}
