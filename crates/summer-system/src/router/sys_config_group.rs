use summer_admin_macros::log;
use summer_auth::LoginUser;
use summer_common::error::ApiResult;
use summer_common::extractor::{Path, Query, ValidatedJson};
use summer_common::response::Json;
use summer_system_model::dto::sys_config_group::{
    ConfigGroupQueryDto, CreateConfigGroupDto, UpdateConfigGroupDto,
};
use summer_system_model::vo::sys_config_group::ConfigGroupVo;
use summer_web::Router;
use summer_web::extractor::Component;
use summer_web::handler::TypeRouter;
use summer_web::{delete_api, get_api, post_api, put_api};

use crate::service::sys_config_group_service::SysConfigGroupService;
use summer_sea_orm::pagination::{Page, Pagination};

#[log(module = "系统参数分组", action = "查询列表", biz_type = Query)]
#[get_api("/config/group/list")]
pub async fn list(
    Component(svc): Component<SysConfigGroupService>,
    Query(query): Query<ConfigGroupQueryDto>,
    pagination: Pagination,
) -> ApiResult<Json<Page<ConfigGroupVo>>> {
    let page = svc.list(query, pagination).await?;
    Ok(Json(page))
}

#[log(module = "系统参数分组", action = "查询详情", biz_type = Query)]
#[get_api("/config/group/{id}")]
pub async fn detail(
    Component(svc): Component<SysConfigGroupService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<ConfigGroupVo>> {
    let item = svc.get_by_id(id).await?;
    Ok(Json(item))
}

#[log(module = "系统参数分组", action = "创建", biz_type = Create)]
#[post_api("/config/group")]
pub async fn create(
    LoginUser { profile, .. }: LoginUser,
    Component(svc): Component<SysConfigGroupService>,
    ValidatedJson(dto): ValidatedJson<CreateConfigGroupDto>,
) -> ApiResult<()> {
    svc.create(dto, &profile.nick_name).await?;
    Ok(())
}

#[log(module = "系统参数分组", action = "更新", biz_type = Update)]
#[put_api("/config/group/{id}")]
pub async fn update(
    LoginUser { profile, .. }: LoginUser,
    Component(svc): Component<SysConfigGroupService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdateConfigGroupDto>,
) -> ApiResult<()> {
    svc.update(id, dto, &profile.nick_name).await?;
    Ok(())
}

#[log(module = "系统参数分组", action = "删除", biz_type = Delete)]
#[delete_api("/config/group/{id}")]
pub async fn delete(
    Component(svc): Component<SysConfigGroupService>,
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
