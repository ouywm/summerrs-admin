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
use summer_web::{delete_api, get_api, post_api};

use crate::router::alert_silence::req::{AlertSilenceQuery, CreateAlertSilenceReq};
use crate::router::alert_silence::res::AlertSilenceRes;
use crate::service::alert_silence::AlertSilenceService;

pub fn routes() -> Router {
    Router::new()
        .typed_route(list_alert_silences)
        .typed_route(create_alert_silence)
        .typed_route(delete_alert_silence)
}

#[get_api("/ai/alert-silence/list")]
pub async fn list_alert_silences(
    Component(svc): Component<AlertSilenceService>,
    Query(query): Query<AlertSilenceQuery>,
    pagination: Pagination,
) -> ApiResult<Json<Page<AlertSilenceRes>>> {
    let page = svc.list_silences(query, pagination).await?;
    Ok(Json(page))
}

#[post_api("/ai/alert-silence")]
pub async fn create_alert_silence(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<AlertSilenceService>,
    ValidatedJson(req): ValidatedJson<CreateAlertSilenceReq>,
) -> ApiResult<Json<AlertSilenceRes>> {
    let silence = svc.create_silence(req, &profile.nick_name).await?;
    Ok(Json(silence))
}

#[delete_api("/ai/alert-silence/{id}")]
pub async fn delete_alert_silence(
    Component(svc): Component<AlertSilenceService>,
    Path(id): Path<i64>,
) -> ApiResult<()> {
    svc.delete_silence(id).await?;
    Ok(())
}
