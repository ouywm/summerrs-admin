use summer_common::error::ApiResult;
use summer_common::extractor::{Path, Query};
use summer_common::response::Json;
use summer_admin_macros::log;
use summer_model::dto::operation_log::OperationLogQueryDto;
use summer_model::vo::operation_log::{OperationLogDetailVo, OperationLogVo};
use summer_web::extractor::Component;
use summer_web::get_api;

use crate::service::operation_log_service::OperationLogService;
use summer_sea_orm::pagination::{Page, Pagination};

#[log(module = "操作日志", action = "查询操作日志", biz_type = Query)]
#[get_api("/operation-log/list")]
pub async fn list_operation_logs(
    Component(svc): Component<OperationLogService>,
    Query(query): Query<OperationLogQueryDto>,
    pagination: Pagination,
) -> ApiResult<Json<Page<OperationLogVo>>> {
    let page = svc.get_operation_logs(query, pagination).await?;
    Ok(Json(page))
}

#[log(module = "操作日志", action = "查询操作日志详情", biz_type = Query)]
#[get_api("/operation-log/{id}")]
pub async fn get_operation_log_detail(
    Component(svc): Component<OperationLogService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<OperationLogDetailVo>> {
    let detail = svc.get_operation_log_detail(id).await?;
    Ok(Json(detail))
}
