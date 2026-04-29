use summer_admin_macros::{log, public};
use summer_ai_model::dto::channel::{ChannelQueryDto, CreateChannelDto, UpdateChannelDto};
use summer_ai_model::vo::channel::{ChannelDetailVo, ChannelStatusCountsVo, ChannelVo};
use summer_auth::LoginUser;
use summer_common::error::ApiResult;
use summer_common::extractor::{Path, Query, ValidatedJson};
use summer_common::response::Json;
use summer_sea_orm::pagination::{Page, Pagination};
use summer_web::extractor::Component;
use summer_web::{delete_api, get_api, post_api, put_api};

use crate::service::channel_service::ChannelService;

/// 分页查询渠道列表
#[public]
#[log(module = "ai/渠道管理", action = "查询渠道列表", biz_type = Query)]
#[get_api("/channel/list")]
pub async fn list(
    Component(svc): Component<ChannelService>,
    Query(query): Query<ChannelQueryDto>,
    pagination: Pagination,
) -> ApiResult<Json<Page<ChannelVo>>> {
    let page = svc.list(query, pagination).await?;
    Ok(Json(page))
}

/// 查询渠道详情
#[log(
    module = "ai/渠道管理",
    action = "查询渠道详情",
    biz_type = Query,
    save_response = false
)]
#[get_api("/channel/{id}")]
pub async fn detail(
    Component(svc): Component<ChannelService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<ChannelDetailVo>> {
    let vo = svc.detail(id).await?;
    Ok(Json(vo))
}

/// 创建渠道
#[log(
    module = "ai/渠道管理",
    action = "创建渠道",
    biz_type = Create,
    save_params = false
)]
#[post_api("/channel")]
pub async fn create(
    LoginUser { profile, .. }: LoginUser,
    Component(svc): Component<ChannelService>,
    ValidatedJson(dto): ValidatedJson<CreateChannelDto>,
) -> ApiResult<()> {
    svc.create(dto, &profile.nick_name).await?;
    Ok(())
}

/// 更新渠道
#[log(
    module = "ai/渠道管理",
    action = "更新渠道",
    biz_type = Update,
    save_params = false
)]
#[put_api("/channel/{id}")]
pub async fn update(
    LoginUser { profile, .. }: LoginUser,
    Component(svc): Component<ChannelService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdateChannelDto>,
) -> ApiResult<()> {
    svc.update(id, dto, &profile.nick_name).await?;
    Ok(())
}

/// 软删除渠道
#[log(module = "ai/渠道管理", action = "删除渠道", biz_type = Delete)]
#[delete_api("/channel/{id}")]
pub async fn delete(
    Component(svc): Component<ChannelService>,
    Path(id): Path<i64>,
) -> ApiResult<()> {
    svc.delete(id).await?;
    Ok(())
}

/// 批量软删除渠道
#[derive(
    Debug, serde::Deserialize, serde::Serialize, schemars::JsonSchema, validator::Validate,
)]
#[serde(rename_all = "camelCase")]
pub struct BatchDeleteDto {
    #[validate(length(min = 1, message = "ids不能为空"))]
    pub ids: Vec<i64>,
}

#[log(module = "ai/渠道管理", action = "批量删除渠道", biz_type = Delete)]
#[post_api("/channel/batch-delete")]
pub async fn batch_delete(
    Component(svc): Component<ChannelService>,
    ValidatedJson(dto): ValidatedJson<BatchDeleteDto>,
) -> ApiResult<Json<serde_json::Value>> {
    let count = svc.batch_delete(dto.ids).await?;
    Ok(Json(serde_json::json!({ "deleted": count })))
}

/// 查询渠道状态统计
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StatusCountsQuery {
    pub channel_type: Option<i16>,
}

#[log(module = "ai/渠道管理", action = "查询渠道状态统计", biz_type = Query)]
#[get_api("/channel/status-counts")]
pub async fn status_counts(
    Component(svc): Component<ChannelService>,
    Query(query): Query<StatusCountsQuery>,
) -> ApiResult<Json<ChannelStatusCountsVo>> {
    let vo = svc.status_counts(query.channel_type).await?;
    Ok(Json(vo))
}
