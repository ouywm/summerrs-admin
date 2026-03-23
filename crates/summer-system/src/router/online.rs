use summer_admin_macros::log;
use summer_common::error::ApiResult;
use summer_common::response::Json;
use summer_system_model::vo::online::OnlineUserVo;
use summer_web::axum::extract::Path;
use summer_web::extractor::Component;
use summer_web::{delete_api, get_api};

use crate::service::online_service::OnlineUserService;
use summer_sea_orm::pagination::{Page, Pagination};

#[log(module = "在线用户", action = "查询在线用户列表", biz_type = Query)]
#[get_api("/online/list")]
pub async fn list_online_users(
    Component(svc): Component<OnlineUserService>,
    pagination: Pagination,
) -> ApiResult<Json<Page<OnlineUserVo>>> {
    let vo = svc.list_online_users(pagination).await?;
    Ok(Json(vo))
}

#[log(module = "在线用户", action = "强制下线", biz_type = Delete)]
#[delete_api("/online/{login_id}")]
pub async fn kick_online_user(
    Component(svc): Component<OnlineUserService>,
    Path(login_id): Path<String>,
) -> ApiResult<()> {
    svc.kick_out(&login_id).await?;
    Ok(())
}

#[log(module = "在线用户", action = "踢下指定设备", biz_type = Delete)]
#[delete_api("/online/{login_id}/{device}")]
pub async fn kick_online_device(
    Component(svc): Component<OnlineUserService>,
    Path((login_id, device)): Path<(String, String)>,
) -> ApiResult<()> {
    svc.kick_out_device(&login_id, &device).await?;
    Ok(())
}
