use summer_auth::AdminUser;
use summer_common::error::ApiResult;
use summer_common::extractor::{Path, Query, ValidatedJson};
use summer_common::response::Json;
use summer_web::extractor::Component;
use summer_web::{get_api, post_api, put_api};

use summer_ai_model::dto::model_config::{
    CreateModelConfigDto, QueryModelConfigDto, UpdateModelConfigDto,
};
use summer_ai_model::vo::model_config::ModelConfigVo;

use crate::service::model::ModelService;
use summer_sea_orm::pagination::{Page, Pagination};

#[get_api("/ai/model-config")]
pub async fn list_model_configs(
    _admin: AdminUser,
    Component(svc): Component<ModelService>,
    Query(query): Query<QueryModelConfigDto>,
    pagination: Pagination,
) -> ApiResult<Json<Page<ModelConfigVo>>> {
    let page = svc.list_model_configs(query, pagination).await?;
    Ok(Json(page))
}

#[get_api("/ai/model-config/{id}")]
pub async fn get_model_config(
    _admin: AdminUser,
    Component(svc): Component<ModelService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<ModelConfigVo>> {
    let model = svc.get_model_config(id).await?;
    Ok(Json(model))
}

#[post_api("/ai/model-config")]
pub async fn create_model_config(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<ModelService>,
    ValidatedJson(dto): ValidatedJson<CreateModelConfigDto>,
) -> ApiResult<Json<ModelConfigVo>> {
    let model = svc.create_model_config(dto, &profile.nick_name).await?;
    Ok(Json(model))
}

#[put_api("/ai/model-config/{id}")]
pub async fn update_model_config(
    AdminUser { profile, .. }: AdminUser,
    Component(svc): Component<ModelService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdateModelConfigDto>,
) -> ApiResult<Json<ModelConfigVo>> {
    let model = svc.update_model_config(id, dto, &profile.nick_name).await?;
    Ok(Json(model))
}
