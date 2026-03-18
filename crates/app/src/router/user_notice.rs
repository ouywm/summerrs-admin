use common::error::ApiResult;
use common::extractor::{Path, Query};
use common::response::Json;
use macros::log;
use model::dto::sys_notice::{UserNoticeLatestQueryDto, UserNoticeQueryDto};
use model::vo::sys_notice::{NoticeUnreadCountVo, UserNoticeDetailVo, UserNoticeVo};
use summer_auth::AdminUser;
use summer_web::extractor::Component;
use summer_web::{get_api, put_api};

use crate::service::user_notice_service::UserNoticeService;
use summer_sea_orm::pagination::{Page, Pagination};

#[log(module = "公告中心", action = "查询公告列表", biz_type = Query)]
#[get_api("/user/notice/list")]
pub async fn list(
    AdminUser { login_id, .. }: AdminUser,
    Component(svc): Component<UserNoticeService>,
    Query(query): Query<UserNoticeQueryDto>,
    pagination: Pagination,
) -> ApiResult<Json<Page<UserNoticeVo>>> {
    let page = svc.list(&login_id, query, pagination).await?;
    Ok(Json(page))
}

#[log(module = "公告中心", action = "查询最新公告", biz_type = Query)]
#[get_api("/user/notice/latest")]
pub async fn latest(
    AdminUser { login_id, .. }: AdminUser,
    Component(svc): Component<UserNoticeService>,
    Query(query): Query<UserNoticeLatestQueryDto>,
) -> ApiResult<Json<Vec<UserNoticeVo>>> {
    let items = svc.latest(&login_id, query).await?;
    Ok(Json(items))
}

#[log(module = "公告中心", action = "查询未读数量", biz_type = Query)]
#[get_api("/user/notice/unread-count")]
pub async fn unread_count(
    AdminUser { login_id, .. }: AdminUser,
    Component(svc): Component<UserNoticeService>,
) -> ApiResult<Json<NoticeUnreadCountVo>> {
    let count = svc.unread_count(&login_id).await?;
    Ok(Json(count))
}

#[log(module = "公告中心", action = "查询公告详情", biz_type = Query)]
#[get_api("/user/notice/{id}")]
pub async fn detail(
    AdminUser { login_id, .. }: AdminUser,
    Component(svc): Component<UserNoticeService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<UserNoticeDetailVo>> {
    let item = svc.detail(&login_id, id).await?;
    Ok(Json(item))
}

#[log(module = "公告中心", action = "标记已读", biz_type = Update)]
#[put_api("/user/notice/{id}/read")]
pub async fn read(
    AdminUser { login_id, .. }: AdminUser,
    Component(svc): Component<UserNoticeService>,
    Path(id): Path<i64>,
) -> ApiResult<()> {
    svc.read(&login_id, id).await?;
    Ok(())
}

#[log(module = "公告中心", action = "全部已读", biz_type = Update)]
#[put_api("/user/notice/read-all")]
pub async fn read_all(
    AdminUser { login_id, .. }: AdminUser,
    Component(svc): Component<UserNoticeService>,
) -> ApiResult<()> {
    svc.read_all(&login_id).await?;
    Ok(())
}
