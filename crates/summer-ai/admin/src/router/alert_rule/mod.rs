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

use crate::router::alert_rule::req::{AlertRuleQuery, CreateAlertRuleReq, UpdateAlertRuleReq};
use crate::router::alert_rule::res::AlertRuleRes;
use crate::service::alert_rule::AlertRuleService;

pub fn routes() -> Router {
    Router::new()
        .typed_route(list_alert_rules)
        .typed_route(get_alert_rule)
        .typed_route(create_alert_rule)
        .typed_route(update_alert_rule)
        .typed_route(delete_alert_rule)
}

#[get_api("/ai/alert-rule/list")]
pub async fn list_alert_rules(
    Component(svc): Component<AlertRuleService>,
    Query(query): Query<AlertRuleQuery>,
    pagination: Pagination,
) -> ApiResult<Json<Page<AlertRuleRes>>> {
    let page = svc.list_rules(query, pagination).await?;
    Ok(Json(page))
}

#[get_api("/ai/alert-rule/{id}")]
pub async fn get_alert_rule(
    Component(svc): Component<AlertRuleService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<AlertRuleRes>> {
    let detail = svc.get_rule(id).await?;
    Ok(Json(detail))
}

#[post_api("/ai/alert-rule")]
pub async fn create_alert_rule(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<AlertRuleService>,
    ValidatedJson(req): ValidatedJson<CreateAlertRuleReq>,
) -> ApiResult<Json<AlertRuleRes>> {
    let rule = svc.create_rule(req, &profile.nick_name).await?;
    Ok(Json(rule))
}

#[put_api("/ai/alert-rule/{id}")]
pub async fn update_alert_rule(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<AlertRuleService>,
    Path(id): Path<i64>,
    ValidatedJson(req): ValidatedJson<UpdateAlertRuleReq>,
) -> ApiResult<Json<AlertRuleRes>> {
    let rule = svc.update_rule(id, req, &profile.nick_name).await?;
    Ok(Json(rule))
}

#[delete_api("/ai/alert-rule/{id}")]
pub async fn delete_alert_rule(
    Component(svc): Component<AlertRuleService>,
    Path(id): Path<i64>,
) -> ApiResult<()> {
    svc.delete_rule(id).await?;
    Ok(())
}
