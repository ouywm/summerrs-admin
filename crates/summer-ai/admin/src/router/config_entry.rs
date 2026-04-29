use crate::service::config_entry_service::ConfigEntryService;
use summer_admin_macros::log;
use summer_ai_model::dto::config_entry::{
    ConfigEntryQueryDto, CreateConfigEntryDto, UpdateConfigEntryDto,
};
use summer_ai_model::vo::config_entry::ConfigEntryVo;
use summer_auth::LoginUser;
use summer_common::error::ApiResult;
use summer_common::extractor::{Path, Query, ValidatedJson};
use summer_common::response::Json;
use summer_sea_orm::pagination::{Page, Pagination};
use summer_web::extractor::Component;
use summer_web::{delete_api, get_api, post_api, put_api};

#[log(module = "ai/配置项管理", action = "查询配置项列表", biz_type = Query)]
#[get_api("/config-entry/list")]
pub async fn list(
    Component(svc): Component<ConfigEntryService>,
    Query(query): Query<ConfigEntryQueryDto>,
    pagination: Pagination,
) -> ApiResult<Json<Page<ConfigEntryVo>>> {
    let page = svc.list(query, pagination).await?;
    Ok(Json(page))
}

#[log(module = "ai/配置项管理", action = "查询配置项详情", biz_type = Query)]
#[get_api("/config-entry/{id}")]
pub async fn detail(
    Component(svc): Component<ConfigEntryService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<ConfigEntryVo>> {
    let vo = svc.detail(id).await?;
    Ok(Json(vo))
}

#[log(module = "ai/配置项管理", action = "创建配置项", biz_type = Create)]
#[post_api("/config-entry")]
pub async fn create(
    LoginUser { profile, .. }: LoginUser,
    Component(svc): Component<ConfigEntryService>,
    ValidatedJson(dto): ValidatedJson<CreateConfigEntryDto>,
) -> ApiResult<()> {
    svc.create(dto, &profile.nick_name).await?;
    Ok(())
}

#[log(module = "ai/配置项管理", action = "更新配置项", biz_type = Update)]
#[put_api("/config-entry/{id}")]
pub async fn update(
    LoginUser { profile, .. }: LoginUser,
    Component(svc): Component<ConfigEntryService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdateConfigEntryDto>,
) -> ApiResult<()> {
    svc.update(id, dto, &profile.nick_name).await?;
    Ok(())
}

#[log(module = "ai/配置项管理", action = "删除配置项", biz_type = Delete)]
#[delete_api("/config-entry/{id}")]
pub async fn delete(
    Component(svc): Component<ConfigEntryService>,
    Path(id): Path<i64>,
) -> ApiResult<()> {
    svc.delete(id).await?;
    Ok(())
}
