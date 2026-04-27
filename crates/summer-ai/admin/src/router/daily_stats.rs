use crate::service::daily_stats_service::DailyStatsService;
use summer_admin_macros::log;
use summer_ai_model::dto::daily_stats::{DailyStatsQueryDto, DashboardQueryDto};
use summer_ai_model::vo::daily_stats::{DailyStatsSummaryVo, DailyStatsVo, DashboardOverviewVo};
use summer_common::error::ApiResult;
use summer_common::extractor::Query;
use summer_common::response::Json;
use summer_sea_orm::pagination::{Page, Pagination};
use summer_web::Router;
use summer_web::extractor::Component;
use summer_web::get_api;
use summer_web::handler::TypeRouter;

#[log(module = "ai/每日统计", action = "查询每日统计列表", biz_type = Query)]
#[get_api("/daily-stats/list")]
pub async fn list(
    Component(svc): Component<DailyStatsService>,
    Query(query): Query<DailyStatsQueryDto>,
    pagination: Pagination,
) -> ApiResult<Json<Page<DailyStatsVo>>> {
    let page = svc.list(query, pagination).await?;
    Ok(Json(page))
}

#[log(module = "ai/每日统计", action = "查询每日统计汇总", biz_type = Query)]
#[get_api("/daily-stats/summary")]
pub async fn summary(
    Component(svc): Component<DailyStatsService>,
    Query(query): Query<DailyStatsQueryDto>,
) -> ApiResult<Json<DailyStatsSummaryVo>> {
    let vo = svc.summary(query).await?;
    Ok(Json(vo))
}

#[log(module = "ai/每日统计", action = "查询统计看板", biz_type = Query)]
#[get_api("/daily-stats/dashboard")]
pub async fn dashboard(
    Component(svc): Component<DailyStatsService>,
    Query(query): Query<DashboardQueryDto>,
) -> ApiResult<Json<DashboardOverviewVo>> {
    let vo = svc.dashboard(query).await?;
    Ok(Json(vo))
}

pub fn routes(router: Router) -> Router {
    router
        .typed_route(list)
        .typed_route(summary)
        .typed_route(dashboard)
}
