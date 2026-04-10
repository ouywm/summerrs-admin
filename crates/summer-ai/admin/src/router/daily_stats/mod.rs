pub mod req;
pub mod res;

use summer_common::error::ApiResult;
use summer_common::extractor::Query;
use summer_common::response::Json;
use summer_sea_orm::pagination::{Page, Pagination};
use summer_web::Router;
use summer_web::extractor::Component;
use summer_web::get_api;
use summer_web::handler::TypeRouter;

use crate::router::daily_stats::req::DailyStatsQuery;
use crate::router::daily_stats::res::DailyStatsRes;
use crate::service::daily_stats::DailyStatsAdminService;

pub fn routes() -> Router {
    Router::new().typed_route(list_daily_stats)
}

#[get_api("/ai/daily-stats/list")]
pub async fn list_daily_stats(
    Component(svc): Component<DailyStatsAdminService>,
    Query(query): Query<DailyStatsQuery>,
    pagination: Pagination,
) -> ApiResult<Json<Page<DailyStatsRes>>> {
    let page = svc.list_daily_stats(query, pagination).await?;
    Ok(Json(page))
}
