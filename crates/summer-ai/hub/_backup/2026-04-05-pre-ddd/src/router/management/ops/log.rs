use summer_common::error::ApiResult;
use summer_common::extractor::Query;
use summer_common::response::Json;
use summer_sea_orm::pagination::{Page, Pagination};
use summer_web::extractor::Component;
use summer_web::get_api;

use summer_ai_model::dto::log::{LogStatsQueryDto, QueryLogDto};
use summer_ai_model::vo::log::{LogStatsVo, LogVo};

use crate::service::log::LogService;

#[get_api("/ai/log")]
pub async fn list_logs(
    Component(svc): Component<LogService>,
    Query(query): Query<QueryLogDto>,
    pagination: Pagination,
) -> ApiResult<Json<Page<LogVo>>> {
    let page = svc.query_logs(query, pagination).await?;
    Ok(Json(page))
}

#[get_api("/ai/log/stats")]
pub async fn log_stats(
    Component(svc): Component<LogService>,
    Query(query): Query<LogStatsQueryDto>,
) -> ApiResult<Json<Vec<LogStatsVo>>> {
    let stats = svc.stats(query).await?;
    Ok(Json(stats))
}
