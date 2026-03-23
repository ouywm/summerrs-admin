use summer_admin_macros::log;
use summer_common::error::ApiResult;
use summer_common::extractor::Query;
use summer_common::response::Json;
use summer_system_model::dto::login_log::LoginLogQueryDto;
use summer_system_model::vo::login_log::LoginLogVo;
use summer_web::extractor::Component;
use summer_web::get_api;

use crate::service::login_log_service::LoginLogService;
use summer_sea_orm::pagination::{Page, Pagination};

#[log(module = "登录日志", action = "查询登录日志", biz_type = Query)]
#[get_api("/login-log/list")]
pub async fn list_login_logs(
    Component(svc): Component<LoginLogService>,
    Query(query): Query<LoginLogQueryDto>,
    pagination: Pagination,
) -> ApiResult<Json<Page<LoginLogVo>>> {
    let page = svc.get_all_login_logs(query, pagination).await?;
    Ok(Json(page))
}
