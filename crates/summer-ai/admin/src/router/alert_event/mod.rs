pub mod req;
pub mod res;

use summer_auth::AdminUser;
use summer_common::error::ApiResult;
use summer_common::extractor::{Path, Query};
use summer_common::response::Json;
use summer_sea_orm::pagination::{Page, Pagination};
use summer_web::Router;
use summer_web::extractor::Component;
use summer_web::handler::TypeRouter;
use summer_web::{get_api, put_api};

use crate::router::alert_event::req::AlertEventQuery;
use crate::router::alert_event::res::AlertEventRes;
use crate::service::alert_event::AlertEventService;

pub fn routes() -> Router {
    Router::new()
        .typed_route(list_alert_events)
        .typed_route(get_alert_event)
        .typed_route(ack_alert_event)
        .typed_route(resolve_alert_event)
        .typed_route(ignore_alert_event)
}

#[get_api("/ai/alert-event/list")]
pub async fn list_alert_events(
    Component(svc): Component<AlertEventService>,
    Query(query): Query<AlertEventQuery>,
    pagination: Pagination,
) -> ApiResult<Json<Page<AlertEventRes>>> {
    let page = svc.list_events(query, pagination).await?;
    Ok(Json(page))
}

#[get_api("/ai/alert-event/{id}")]
pub async fn get_alert_event(
    Component(svc): Component<AlertEventService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<AlertEventRes>> {
    let detail = svc.get_event(id).await?;
    Ok(Json(detail))
}

#[put_api("/ai/alert-event/{id}/ack")]
pub async fn ack_alert_event(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<AlertEventService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<AlertEventRes>> {
    let event = svc.ack_event(id, &profile.nick_name).await?;
    Ok(Json(event))
}

#[put_api("/ai/alert-event/{id}/resolve")]
pub async fn resolve_alert_event(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<AlertEventService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<AlertEventRes>> {
    let event = svc.resolve_event(id, &profile.nick_name).await?;
    Ok(Json(event))
}

#[put_api("/ai/alert-event/{id}/ignore")]
pub async fn ignore_alert_event(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<AlertEventService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<AlertEventRes>> {
    let event = svc.ignore_event(id, &profile.nick_name).await?;
    Ok(Json(event))
}
