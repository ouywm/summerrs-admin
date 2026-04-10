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

use crate::router::request::req::RequestQuery;
use crate::router::request::res::{RequestDetailRes, RequestListRes};
use crate::router::request_execution::req::RequestExecutionQuery;
use crate::router::request_execution::res::RequestExecutionListRes;
use crate::service::request::RequestService;
use crate::service::request_execution::RequestExecutionService;

pub fn routes() -> Router {
    Router::new()
        .typed_route(list_requests)
        .typed_route(get_request)
        .typed_route(list_request_executions)
}

#[get_api("/ai/request/list")]
pub async fn list_requests(
    Component(svc): Component<RequestService>,
    Query(query): Query<RequestQuery>,
    pagination: Pagination,
) -> ApiResult<Json<Page<RequestListRes>>> {
    let page = svc.list_requests(query, pagination).await?;
    Ok(Json(page))
}

#[get_api("/ai/request/{id}")]
pub async fn get_request(
    Component(svc): Component<RequestService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<RequestDetailRes>> {
    let detail = svc.get_request(id).await?;
    Ok(Json(detail))
}

#[get_api("/ai/request/{id}/executions")]
pub async fn list_request_executions(
    Component(svc): Component<RequestExecutionService>,
    Path(id): Path<i64>,
    pagination: Pagination,
) -> ApiResult<Json<Page<RequestExecutionListRes>>> {
    let page = svc
        .list_request_executions(
            RequestExecutionQuery {
                ai_request_id: Some(id),
                ..Default::default()
            },
            pagination,
        )
        .await?;
    Ok(Json(page))
}
