use common::error::ApiResult;
use common::response::ApiResponse;
use macros::log;
use model::dto::login_log::LoginLogQueryDto;
use model::vo::login_log::LoginLogVo;
use spring_web::axum::extract::Query;
use spring_web::extractor::Component;
use spring_web::get;

use crate::plugin::pagination::{Page, Pagination};
use crate::service::login_log_service::LoginLogService;

#[log(module = "登录日志", action = "查询登录日志", biz_type = Query)]
#[get("/login-log/list")]
pub async fn list_login_logs(
    Component(svc): Component<LoginLogService>,
    Query(query): Query<LoginLogQueryDto>,
    pagination: Pagination,
) -> ApiResult<ApiResponse<Page<LoginLogVo>>> {
    let page = svc.get_all_login_logs(query, pagination).await?;
    Ok(ApiResponse::ok(page))
}