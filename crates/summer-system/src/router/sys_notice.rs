use summer_admin_macros::log;
use summer_auth::AdminUser;
use summer_common::error::ApiResult;
use summer_common::extractor::{Path, Query, ValidatedJson};
use summer_common::response::Json;
use summer_system_model::dto::sys_notice::{CreateNoticeDto, NoticeQueryDto, UpdateNoticeDto};
use summer_system_model::vo::sys_notice::{NoticeDetailVo, NoticeVo};
use summer_web::extractor::Component;
use summer_web::{delete_api, get_api, post_api, put_api};

use crate::service::sys_notice_service::SysNoticeService;
use summer_sea_orm::pagination::{Page, Pagination};

#[log(module = "系统公告", action = "查询列表", biz_type = Query)]
#[get_api("/notice/list")]
pub async fn list(
    Component(svc): Component<SysNoticeService>,
    Query(query): Query<NoticeQueryDto>,
    pagination: Pagination,
) -> ApiResult<Json<Page<NoticeVo>>> {
    let page = svc.list(query, pagination).await?;
    Ok(Json(page))
}

#[log(module = "系统公告", action = "查询详情", biz_type = Query)]
#[get_api("/notice/{id}")]
pub async fn detail(
    Component(svc): Component<SysNoticeService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<NoticeDetailVo>> {
    let item = svc.get_by_id(id).await?;
    Ok(Json(item))
}

#[log(module = "系统公告", action = "创建", biz_type = Create)]
#[post_api("/notice")]
pub async fn create(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<SysNoticeService>,
    ValidatedJson(dto): ValidatedJson<CreateNoticeDto>,
) -> ApiResult<()> {
    svc.create(dto, &profile.nick_name).await?;
    Ok(())
}

#[log(module = "系统公告", action = "更新", biz_type = Update)]
#[put_api("/notice/{id}")]
pub async fn update(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<SysNoticeService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdateNoticeDto>,
) -> ApiResult<()> {
    svc.update(id, dto, &profile.nick_name).await?;
    Ok(())
}

#[log(module = "系统公告", action = "删除", biz_type = Delete)]
#[delete_api("/notice/{id}")]
pub async fn delete(
    Component(svc): Component<SysNoticeService>,
    Path(id): Path<i64>,
) -> ApiResult<()> {
    svc.delete(id).await?;
    Ok(())
}

#[log(module = "系统公告", action = "发布公告", biz_type = Update)]
#[put_api("/notice/{id}/publish")]
pub async fn publish(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<SysNoticeService>,
    Path(id): Path<i64>,
) -> ApiResult<()> {
    svc.publish(id, &profile.nick_name).await?;
    Ok(())
}

#[log(module = "系统公告", action = "撤回公告", biz_type = Update)]
#[put_api("/notice/{id}/revoke")]
pub async fn revoke(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<SysNoticeService>,
    Path(id): Path<i64>,
) -> ApiResult<()> {
    svc.revoke(id, &profile.nick_name).await?;
    Ok(())
}

#[log(module = "系统公告", action = "置顶公告", biz_type = Update)]
#[put_api("/notice/{id}/pin")]
pub async fn pin(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<SysNoticeService>,
    Path(id): Path<i64>,
) -> ApiResult<()> {
    svc.pin(id, &profile.nick_name).await?;
    Ok(())
}

#[log(module = "系统公告", action = "取消置顶", biz_type = Update)]
#[put_api("/notice/{id}/unpin")]
pub async fn unpin(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<SysNoticeService>,
    Path(id): Path<i64>,
) -> ApiResult<()> {
    svc.unpin(id, &profile.nick_name).await?;
    Ok(())
}
