use crate::service::ability_service::AbilityService;
use summer_admin_macros::log;
use summer_ai_model::dto::ability::{AbilityQueryDto, CreateAbilityDto, UpdateAbilityDto};
use summer_ai_model::vo::ability::AbilityVo;
use summer_auth::LoginUser;
use summer_common::error::ApiResult;
use summer_common::extractor::{Path, Query, ValidatedJson};
use summer_common::response::Json;
use summer_sea_orm::pagination::{Page, Pagination};
use summer_web::extractor::Component;
use summer_web::handler::TypeRouter;
use summer_web::{Router, delete_api, get_api, post_api, put_api};

#[log(module = "ai/能力管理", action = "查询能力列表", biz_type = Query)]
#[get_api("/ability/list")]
pub async fn list(
    Component(svc): Component<AbilityService>,
    Query(query): Query<AbilityQueryDto>,
    pagination: Pagination,
) -> ApiResult<Json<Page<AbilityVo>>> {
    let page = svc.list(query, pagination).await?;
    Ok(Json(page))
}

#[log(module = "ai/能力管理", action = "查询能力详情", biz_type = Query)]
#[get_api("/ability/{id}")]
pub async fn detail(
    Component(svc): Component<AbilityService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<AbilityVo>> {
    let vo = svc.detail(id).await?;
    Ok(Json(vo))
}

#[log(module = "ai/能力管理", action = "创建能力", biz_type = Create)]
#[post_api("/ability")]
pub async fn create(
    _user: LoginUser,
    Component(svc): Component<AbilityService>,
    ValidatedJson(dto): ValidatedJson<CreateAbilityDto>,
) -> ApiResult<()> {
    svc.create(dto).await?;
    Ok(())
}

#[log(module = "ai/能力管理", action = "更新能力", biz_type = Update)]
#[put_api("/ability/{id}")]
pub async fn update(
    _user: LoginUser,
    Component(svc): Component<AbilityService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdateAbilityDto>,
) -> ApiResult<()> {
    svc.update(id, dto).await?;
    Ok(())
}

#[log(module = "ai/能力管理", action = "删除能力", biz_type = Delete)]
#[delete_api("/ability/{id}")]
pub async fn delete(
    Component(svc): Component<AbilityService>,
    Path(id): Path<i64>,
) -> ApiResult<()> {
    svc.delete(id).await?;
    Ok(())
}

pub fn routes(router: Router) -> Router {
    router
        .typed_route(list)
        .typed_route(detail)
        .typed_route(create)
        .typed_route(update)
        .typed_route(delete)
}
