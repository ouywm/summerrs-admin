use crate::service::group_ratio_service::GroupRatioService;
use summer_admin_macros::log;
use summer_ai_model::dto::group_ratio::{
    CreateGroupRatioDto, GroupRatioQueryDto, UpdateGroupRatioDto,
};
use summer_ai_model::vo::group_ratio::GroupRatioVo;
use summer_auth::LoginUser;
use summer_common::error::ApiResult;
use summer_common::extractor::{Path, Query, ValidatedJson};
use summer_common::response::Json;
use summer_sea_orm::pagination::{Page, Pagination};
use summer_web::Router;
use summer_web::extractor::Component;
use summer_web::handler::TypeRouter;
use summer_web::{delete_api, get_api, post_api, put_api};

#[log(module = "ai/分组比率管理", action = "查询分组比率列表", biz_type = Query)]
#[get_api("/group-ratio/list")]
pub async fn list(
    Component(svc): Component<GroupRatioService>,
    Query(query): Query<GroupRatioQueryDto>,
    pagination: Pagination,
) -> ApiResult<Json<Page<GroupRatioVo>>> {
    let page = svc.list(query, pagination).await?;
    Ok(Json(page))
}

#[log(module = "ai/分组比率管理", action = "查询分组比率详情", biz_type = Query)]
#[get_api("/group-ratio/{id}")]
pub async fn detail(
    Component(svc): Component<GroupRatioService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<GroupRatioVo>> {
    let vo = svc.detail(id).await?;
    Ok(Json(vo))
}

#[log(module = "ai/分组比率管理", action = "创建分组比率", biz_type = Create)]
#[post_api("/group-ratio")]
pub async fn create(
    LoginUser { profile, .. }: LoginUser,
    Component(svc): Component<GroupRatioService>,
    ValidatedJson(dto): ValidatedJson<CreateGroupRatioDto>,
) -> ApiResult<()> {
    svc.create(dto, &profile.nick_name).await?;
    Ok(())
}

#[log(module = "ai/分组比率管理", action = "更新分组比率", biz_type = Update)]
#[put_api("/group-ratio/{id}")]
pub async fn update(
    LoginUser { profile, .. }: LoginUser,
    Component(svc): Component<GroupRatioService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdateGroupRatioDto>,
) -> ApiResult<()> {
    svc.update(id, dto, &profile.nick_name).await?;
    Ok(())
}

#[log(module = "ai/分组比率管理", action = "删除分组比率", biz_type = Delete)]
#[delete_api("/group-ratio/{id}")]
pub async fn delete(
    Component(svc): Component<GroupRatioService>,
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
