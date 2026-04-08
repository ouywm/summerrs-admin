pub mod req;
pub mod res;

use summer_common::error::ApiResult;
use summer_common::extractor::{Path, Query};
use summer_common::response::Json;
use summer_sea_orm::pagination::{Page, Pagination};
use summer_web::Router;
use summer_web::extractor::Component;
use summer_web::get_api;
use summer_web::handler::TypeRouter;

use crate::service::request_execution::RequestExecutionService;

use self::req::RequestExecutionQuery;
use self::res::{RequestExecutionDetailRes, RequestExecutionListRes};

pub fn routes() -> Router {
    Router::new()
        .typed_route(list_request_executions)
        .typed_route(get_request_execution)
}

#[get_api("/ai/request-execution/list")]
pub async fn list_request_executions(
    Component(svc): Component<RequestExecutionService>,
    Query(query): Query<RequestExecutionQuery>,
    pagination: Pagination,
) -> ApiResult<Json<Page<RequestExecutionListRes>>> {
    let page = svc.list_request_executions(query, pagination).await?;
    Ok(Json(page))
}

#[get_api("/ai/request-execution/{id}")]
pub async fn get_request_execution(
    Component(svc): Component<RequestExecutionService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<RequestExecutionDetailRes>> {
    let detail = svc.get_request_execution(id).await?;
    Ok(Json(detail))
}
