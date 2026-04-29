use summer_admin_macros::log;
use summer_ai_model::dto::channel_account::{
    ChannelAccountQueryDto, CreateChannelAccountDto, UpdateChannelAccountDto,
};
use summer_ai_model::vo::channel_account::{ChannelAccountDetailVo, ChannelAccountVo};
use summer_auth::LoginUser;
use summer_common::error::ApiResult;
use summer_common::extractor::{Path, Query, ValidatedJson};
use summer_common::response::Json;
use summer_sea_orm::pagination::{Page, Pagination};
use summer_web::extractor::Component;
use summer_web::{delete_api, get_api, post_api, put_api};

use crate::service::channel_account_service::ChannelAccountService;

/// 分页查询渠道账号列表
#[log(module = "ai/渠道账号管理", action = "查询渠道账号列表", biz_type = Query)]
#[get_api("/channel-account/list")]
pub async fn list(
    Component(svc): Component<ChannelAccountService>,
    Query(query): Query<ChannelAccountQueryDto>,
    pagination: Pagination,
) -> ApiResult<Json<Page<ChannelAccountVo>>> {
    let page = svc.list(query, pagination).await?;
    Ok(Json(page))
}

/// 查询渠道账号详情
#[log(
    module = "ai/渠道账号管理",
    action = "查询渠道账号详情",
    biz_type = Query,
    save_response = false
)]
#[get_api("/channel-account/{id}")]
pub async fn detail(
    Component(svc): Component<ChannelAccountService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<ChannelAccountDetailVo>> {
    let vo = svc.detail(id).await?;
    Ok(Json(vo))
}

/// 创建渠道账号
#[log(
    module = "ai/渠道账号管理",
    action = "创建渠道账号",
    biz_type = Create,
    save_params = false
)]
#[post_api("/channel-account")]
pub async fn create(
    LoginUser { profile, .. }: LoginUser,
    Component(svc): Component<ChannelAccountService>,
    ValidatedJson(dto): ValidatedJson<CreateChannelAccountDto>,
) -> ApiResult<()> {
    svc.create(dto, &profile.nick_name).await?;
    Ok(())
}

/// 更新渠道账号
#[log(
    module = "ai/渠道账号管理",
    action = "更新渠道账号",
    biz_type = Update,
    save_params = false
)]
#[put_api("/channel-account/{id}")]
pub async fn update(
    LoginUser { profile, .. }: LoginUser,
    Component(svc): Component<ChannelAccountService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdateChannelAccountDto>,
) -> ApiResult<()> {
    svc.update(id, dto, &profile.nick_name).await?;
    Ok(())
}

/// 软删除渠道账号
#[log(module = "ai/渠道账号管理", action = "删除渠道账号", biz_type = Delete)]
#[delete_api("/channel-account/{id}")]
pub async fn delete(
    Component(svc): Component<ChannelAccountService>,
    Path(id): Path<i64>,
) -> ApiResult<()> {
    svc.delete(id).await?;
    Ok(())
}
