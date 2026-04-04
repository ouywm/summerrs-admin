use summer_auth::AdminUser;
use summer_common::error::ApiResult;
use summer_common::extractor::{Path, Query, ValidatedJson};
use summer_common::response::Json;
use summer_sea_orm::pagination::{Page, Pagination};
use summer_web::extractor::Component;
use summer_web::{delete_api, get_api, post_api, put_api};

use crate::service::platform_config::PlatformConfigService;
use summer_ai_model::dto::platform::*;
use summer_ai_model::vo::platform::*;

#[get_api("/ai/config")]
pub async fn list_configs(
    Component(svc): Component<PlatformConfigService>,
    Query(q): Query<QueryConfigEntryDto>,
    p: Pagination,
) -> ApiResult<Json<Page<ConfigEntryVo>>> {
    Ok(Json(svc.list_configs(q, p).await?))
}
#[get_api("/ai/config/{id}")]
pub async fn get_config(
    Component(svc): Component<PlatformConfigService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<ConfigEntryVo>> {
    Ok(Json(svc.get_config(id).await?))
}
#[post_api("/ai/config")]
pub async fn create_config(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<PlatformConfigService>,
    ValidatedJson(dto): ValidatedJson<CreateConfigEntryDto>,
) -> ApiResult<Json<ConfigEntryVo>> {
    Ok(Json(svc.create_config(dto, &profile.nick_name).await?))
}
#[put_api("/ai/config/{id}")]
pub async fn update_config(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<PlatformConfigService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdateConfigEntryDto>,
) -> ApiResult<Json<ConfigEntryVo>> {
    Ok(Json(svc.update_config(id, dto, &profile.nick_name).await?))
}
#[delete_api("/ai/config/{id}")]
pub async fn delete_config(
    Component(svc): Component<PlatformConfigService>,
    Path(id): Path<i64>,
) -> ApiResult<()> {
    svc.delete_config(id).await
}
