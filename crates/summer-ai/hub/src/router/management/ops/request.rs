use summer_common::error::ApiResult;
use summer_common::extractor::{Path, Query};
use summer_common::response::Json;
use summer_sea_orm::pagination::{Page, Pagination};
use summer_web::extractor::Component;
use summer_web::get_api;

use summer_ai_model::dto::request::QueryRequestDto;
use summer_ai_model::vo::request::{RequestVo, RequestWithExecutionsVo};

use crate::service::request::RequestService;

#[get_api("/ai/request")]
pub async fn list_requests(
    Component(svc): Component<RequestService>,
    Query(query): Query<QueryRequestDto>,
    pagination: Pagination,
) -> ApiResult<Json<Page<RequestVo>>> {
    let page = svc.query_requests(query, pagination).await?;
    Ok(Json(page))
}

#[get_api("/ai/request/{id}")]
pub async fn get_request(
    Component(svc): Component<RequestService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<RequestWithExecutionsVo>> {
    let detail = svc.get_request_detail(id).await?;
    Ok(Json(detail))
}

#[get_api("/ai/request/by-request-id/{request_id}")]
pub async fn get_request_by_request_id(
    Component(svc): Component<RequestService>,
    Path(request_id): Path<String>,
) -> ApiResult<Json<RequestWithExecutionsVo>> {
    let detail = svc.get_by_request_id(&request_id).await?;
    Ok(Json(detail))
}
