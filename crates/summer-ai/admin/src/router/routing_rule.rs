use crate::service::routing_rule_service::RoutingRuleService;
use summer_admin_macros::log;
use summer_ai_model::dto::routing_rule::{
    CreateRoutingRuleDto, RoutingRuleQueryDto, UpdateRoutingRuleDto,
};
use summer_ai_model::vo::routing_rule::RoutingRuleVo;
use summer_auth::LoginUser;
use summer_common::error::ApiResult;
use summer_common::extractor::{Path, Query, ValidatedJson};
use summer_common::response::Json;
use summer_sea_orm::pagination::{Page, Pagination};
use summer_web::Router;
use summer_web::extractor::Component;
use summer_web::handler::TypeRouter;
use summer_web::{delete_api, get_api, post_api, put_api};

#[log(module = "ai/路由规则管理", action = "查询路由规则列表", biz_type = Query)]
#[get_api("/routing-rule/list")]
pub async fn list(
    Component(svc): Component<RoutingRuleService>,
    Query(query): Query<RoutingRuleQueryDto>,
    pagination: Pagination,
) -> ApiResult<Json<Page<RoutingRuleVo>>> {
    let page = svc.list(query, pagination).await?;
    Ok(Json(page))
}

#[log(module = "ai/路由规则管理", action = "查询路由规则详情", biz_type = Query)]
#[get_api("/routing-rule/{id}")]
pub async fn detail(
    Component(svc): Component<RoutingRuleService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<RoutingRuleVo>> {
    let vo = svc.detail(id).await?;
    Ok(Json(vo))
}

#[log(module = "ai/路由规则管理", action = "创建路由规则", biz_type = Create)]
#[post_api("/routing-rule")]
pub async fn create(
    LoginUser { profile, .. }: LoginUser,
    Component(svc): Component<RoutingRuleService>,
    ValidatedJson(dto): ValidatedJson<CreateRoutingRuleDto>,
) -> ApiResult<()> {
    svc.create(dto, &profile.nick_name).await?;
    Ok(())
}

#[log(module = "ai/路由规则管理", action = "更新路由规则", biz_type = Update)]
#[put_api("/routing-rule/{id}")]
pub async fn update(
    LoginUser { profile, .. }: LoginUser,
    Component(svc): Component<RoutingRuleService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdateRoutingRuleDto>,
) -> ApiResult<()> {
    svc.update(id, dto, &profile.nick_name).await?;
    Ok(())
}

#[log(module = "ai/路由规则管理", action = "删除路由规则", biz_type = Delete)]
#[delete_api("/routing-rule/{id}")]
pub async fn delete(
    Component(svc): Component<RoutingRuleService>,
    Path(id): Path<i64>,
) -> ApiResult<()> {
    svc.delete(id).await?;
    Ok(())
}

pub fn routes(router: Router) -> Router {
    router
        .typed_route(list)
        .typed_route(detail)
        .typed_route(create)
        .typed_route(update)
        .typed_route(delete)
}
