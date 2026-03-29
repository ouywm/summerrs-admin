use summer_common::error::ApiResult;
use summer_common::response::Json;
use summer_web::extractor::Component;
use summer_web::get_api;

use summer_ai_model::vo::runtime::{AiRuntimeChannelHealthVo, AiRuntimeRouteVo};

use crate::service::runtime::RuntimeService;

#[get_api("/ai/runtime/health")]
pub async fn runtime_health(
    Component(svc): Component<RuntimeService>,
) -> ApiResult<Json<Vec<AiRuntimeChannelHealthVo>>> {
    Ok(Json(svc.health().await?))
}

#[get_api("/ai/runtime/routes")]
pub async fn runtime_routes(
    Component(svc): Component<RuntimeService>,
) -> ApiResult<Json<Vec<AiRuntimeRouteVo>>> {
    Ok(Json(svc.routes().await?))
}
