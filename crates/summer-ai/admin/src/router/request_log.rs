use crate::service::request_log_service::RequestLogService;
use summer_admin_macros::log;
use summer_ai_model::dto::request_log::RequestLogQueryDto;
use summer_ai_model::vo::request_log::{RequestDetailVo, RequestLogVo};
use summer_common::error::ApiResult;
use summer_common::extractor::{Path, Query};
use summer_common::response::Json;
use summer_sea_orm::pagination::{Page, Pagination};
use summer_web::Router;
use summer_web::extractor::Component;
use summer_web::get_api;
use summer_web::handler::TypeRouter;

#[log(
    module = "ai/请求日志",
    action = "查询请求日志列表",
    biz_type = Query,
    save_response = false
)]
#[get_api("/request-log/list")]
pub async fn list(
    Component(svc): Component<RequestLogService>,
    Query(query): Query<RequestLogQueryDto>,
    pagination: Pagination,
) -> ApiResult<Json<Page<RequestLogVo>>> {
    let page = svc.list(query, pagination).await?;
    Ok(Json(page))
}

#[log(
    module = "ai/请求日志",
    action = "查询请求日志详情",
    biz_type = Query,
    save_response = false
)]
#[get_api("/request-log/{id}")]
pub async fn log_detail(
    Component(svc): Component<RequestLogService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<RequestLogVo>> {
    let vo = svc.log_detail(id).await?;
    Ok(Json(vo))
}

#[log(
    module = "ai/请求日志",
    action = "查询请求详情",
    biz_type = Query,
    save_response = false
)]
#[get_api("/request/{id}")]
pub async fn request_detail(
    Component(svc): Component<RequestLogService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<RequestDetailVo>> {
    let vo = svc.request_detail(id).await?;
    Ok(Json(vo))
}

#[log(
    module = "ai/请求日志",
    action = "按请求号查询请求详情",
    biz_type = Query,
    save_response = false
)]
#[get_api("/request/by-request-id/{request_id}")]
pub async fn request_detail_by_request_id(
    Component(svc): Component<RequestLogService>,
    Path(request_id): Path<String>,
) -> ApiResult<Json<RequestDetailVo>> {
    let vo = svc.request_detail_by_request_id(request_id).await?;
    Ok(Json(vo))
}

pub fn routes(router: Router) -> Router {
    router
        .typed_route(list)
        .typed_route(log_detail)
        .typed_route(request_detail)
        .typed_route(request_detail_by_request_id)
}
