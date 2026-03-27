use summer_common::error::ApiResult;
use summer_common::response::Json;
use summer_web::extractor::Component;
use summer_web::get_api;

use summer_ai_model::vo::dashboard::DashboardOverviewVo;

use crate::service::log::LogService;

#[get_api("/ai/dashboard/overview")]
pub async fn overview(
    Component(svc): Component<LogService>,
) -> ApiResult<Json<DashboardOverviewVo>> {
    let vo = svc.dashboard_overview().await?;
    Ok(Json(vo))
}
