use common::error::ApiResult;
use common::response::ApiResponse;
use macros::log;
use model::dto::operation_log::OperationLogQueryDto;
use model::vo::operation_log::{OperationLogDetailVo, OperationLogVo};
use spring_web::axum::extract::{Path, Query};
use spring_web::extractor::Component;
use spring_web::get;

use crate::plugin::pagination::{Page, Pagination};
use crate::service::operation_log_service::OperationLogService;

#[log(module = "操作日志", action = "查询操作日志", biz_type = Query)]
#[get("/operation-log/list")]
pub async fn list_operation_logs(
    Component(svc): Component<OperationLogService>,
    Query(query): Query<OperationLogQueryDto>,
    pagination: Pagination,
) -> ApiResult<ApiResponse<Page<OperationLogVo>>> {
    let page = svc.get_operation_logs(query, pagination).await?;
    Ok(ApiResponse::ok(page))
}

#[log(module = "操作日志", action = "查询操作日志详情", biz_type = Query)]
#[get("/operation-log/{id}")]
pub async fn get_operation_log_detail(
    Component(svc): Component<OperationLogService>,
    Path(id): Path<i64>,
) -> ApiResult<ApiResponse<OperationLogDetailVo>> {
    let detail = svc.get_operation_log_detail(id).await?;
    Ok(ApiResponse::ok(detail))
}
