use crate::service::routing_target_service::RoutingTargetService;
use summer_admin_macros::log;
use summer_ai_model::dto::routing_target::{
    CreateRoutingTargetDto, RoutingTargetQueryDto, UpdateRoutingTargetDto,
};
use summer_ai_model::vo::routing_target::RoutingTargetVo;
use summer_auth::LoginUser;
use summer_common::error::ApiResult;
use summer_common::extractor::{Path, Query, ValidatedJson};
use summer_common::response::Json;
use summer_sea_orm::pagination::{Page, Pagination};
use summer_web::extractor::Component;
use summer_web::{delete_api, get_api, post_api, put_api};

#[log(module = "ai/路由目标管理", action = "查询路由目标列表", biz_type = Query)]
#[get_api("/routing-target/list")]
pub async fn list(
    Component(svc): Component<RoutingTargetService>,
    Query(query): Query<RoutingTargetQueryDto>,
    pagination: Pagination,
) -> ApiResult<Json<Page<RoutingTargetVo>>> {
    let page = svc.list(query, pagination).await?;
    Ok(Json(page))
}

#[log(module = "ai/路由目标管理", action = "查询路由目标详情", biz_type = Query)]
#[get_api("/routing-target/{id}")]
pub async fn detail(
    Component(svc): Component<RoutingTargetService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<RoutingTargetVo>> {
    let vo = svc.detail(id).await?;
    Ok(Json(vo))
}

#[log(module = "ai/路由目标管理", action = "创建路由目标", biz_type = Create)]
#[post_api("/routing-target")]
pub async fn create(
    _user: LoginUser,
    Component(svc): Component<RoutingTargetService>,
    ValidatedJson(dto): ValidatedJson<CreateRoutingTargetDto>,
) -> ApiResult<()> {
    svc.create(dto).await?;
    Ok(())
}

#[log(module = "ai/路由目标管理", action = "更新路由目标", biz_type = Update)]
#[put_api("/routing-target/{id}")]
pub async fn update(
    _user: LoginUser,
    Component(svc): Component<RoutingTargetService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdateRoutingTargetDto>,
) -> ApiResult<()> {
    svc.update(id, dto).await?;
    Ok(())
}

#[log(module = "ai/路由目标管理", action = "删除路由目标", biz_type = Delete)]
#[delete_api("/routing-target/{id}")]
pub async fn delete(
    Component(svc): Component<RoutingTargetService>,
    Path(id): Path<i64>,
) -> ApiResult<()> {
    svc.delete(id).await?;
    Ok(())
}
