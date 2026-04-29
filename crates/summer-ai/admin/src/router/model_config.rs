use crate::service::model_config_service::ModelConfigService;
use summer_admin_macros::log;
use summer_ai_model::dto::model_config::{
    CreateModelConfigDto, ModelConfigQueryDto, UpdateModelConfigDto,
};
use summer_ai_model::vo::model_config::ModelConfigVo;
use summer_auth::LoginUser;
use summer_common::error::ApiResult;
use summer_common::extractor::{Path, Query, ValidatedJson};
use summer_common::response::Json;
use summer_sea_orm::pagination::{Page, Pagination};
use summer_web::extractor::Component;
use summer_web::{delete_api, get_api, post_api, put_api};

#[log(module = "ai/模型配置管理", action = "查询模型配置列表", biz_type = Query)]
#[get_api("/model-config/list")]
pub async fn list(
    Component(svc): Component<ModelConfigService>,
    Query(query): Query<ModelConfigQueryDto>,
    pagination: Pagination,
) -> ApiResult<Json<Page<ModelConfigVo>>> {
    let page = svc.list(query, pagination).await?;
    Ok(Json(page))
}

#[log(module = "ai/模型配置管理", action = "查询模型配置详情", biz_type = Query)]
#[get_api("/model-config/{id}")]
pub async fn detail(
    Component(svc): Component<ModelConfigService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<ModelConfigVo>> {
    let vo = svc.detail(id).await?;
    Ok(Json(vo))
}

#[log(module = "ai/模型配置管理", action = "创建模型配置", biz_type = Create)]
#[post_api("/model-config")]
pub async fn create(
    LoginUser { profile, .. }: LoginUser,
    Component(svc): Component<ModelConfigService>,
    ValidatedJson(dto): ValidatedJson<CreateModelConfigDto>,
) -> ApiResult<()> {
    svc.create(dto, &profile.nick_name).await?;
    Ok(())
}

#[log(module = "ai/模型配置管理", action = "更新模型配置", biz_type = Update)]
#[put_api("/model-config/{id}")]
pub async fn update(
    LoginUser { profile, .. }: LoginUser,
    Component(svc): Component<ModelConfigService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdateModelConfigDto>,
) -> ApiResult<()> {
    svc.update(id, dto, &profile.nick_name).await?;
    Ok(())
}

#[log(module = "ai/模型配置管理", action = "删除模型配置", biz_type = Delete)]
#[delete_api("/model-config/{id}")]
pub async fn delete(
    Component(svc): Component<ModelConfigService>,
    Path(id): Path<i64>,
) -> ApiResult<()> {
    svc.delete(id).await?;
    Ok(())
}
