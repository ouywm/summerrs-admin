use summer_admin_macros::log;
use summer_common::error::ApiResult;
use summer_common::extractor::{Path, Query, ValidatedJson};
use summer_common::response::Json;
use summer_sea_orm::pagination::{Page, Pagination};
use summer_system_model::dto::sys_resource::{
    CreateResourceDto, ResourceQueryDto, SaveActionResourcesDto, UpdateResourceDto,
    UpdateResourceEnabledDto,
};
use summer_system_model::vo::sys_resource::{
    ActionResourceVo, ResourceActionVo, ResourceOptionVo, ResourceVo,
};
use summer_web::extractor::Component;
use summer_web::{delete_api, get_api, post_api, put_api};

use crate::service::sys_resource_service::SysResourceService;

#[log(module = "资源权限管理", action = "查询资源列表", biz_type = Query)]
#[get_api("/system/resource/list")]
pub async fn list(
    Component(svc): Component<SysResourceService>,
    Query(query): Query<ResourceQueryDto>,
    pagination: Pagination,
) -> ApiResult<Json<Page<ResourceVo>>> {
    let page = svc.list(query, pagination).await?;
    Ok(Json(page))
}

#[log(module = "资源权限管理", action = "查询资源选项", biz_type = Query)]
#[get_api("/system/resource/options")]
pub async fn options(
    Component(svc): Component<SysResourceService>,
) -> ApiResult<Json<Vec<ResourceOptionVo>>> {
    let items = svc.options().await?;
    Ok(Json(items))
}

#[log(module = "资源权限管理", action = "查询资源详情", biz_type = Query)]
#[get_api("/system/resource/{id}")]
pub async fn detail(
    Component(svc): Component<SysResourceService>,
    Path(id): Path<i64>,
) -> ApiResult<Json<ResourceVo>> {
    let item = svc.get_by_id(id).await?;
    Ok(Json(item))
}

#[log(module = "资源权限管理", action = "创建资源", biz_type = Create)]
#[post_api("/system/resource")]
pub async fn create(
    Component(svc): Component<SysResourceService>,
    ValidatedJson(dto): ValidatedJson<CreateResourceDto>,
) -> ApiResult<()> {
    svc.create(dto).await?;
    Ok(())
}

#[log(module = "资源权限管理", action = "更新资源", biz_type = Update)]
#[put_api("/system/resource/{id}")]
pub async fn update(
    Component(svc): Component<SysResourceService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdateResourceDto>,
) -> ApiResult<()> {
    svc.update(id, dto).await?;
    Ok(())
}

#[log(module = "资源权限管理", action = "启停资源", biz_type = Update)]
#[put_api("/system/resource/{id}/enabled")]
pub async fn update_enabled(
    Component(svc): Component<SysResourceService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdateResourceEnabledDto>,
) -> ApiResult<()> {
    svc.update_enabled(id, dto).await?;
    Ok(())
}

#[log(module = "资源权限管理", action = "删除资源", biz_type = Delete)]
#[delete_api("/system/resource/{id}")]
pub async fn delete(
    Component(svc): Component<SysResourceService>,
    Path(id): Path<i64>,
) -> ApiResult<()> {
    svc.delete(id).await?;
    Ok(())
}

#[log(module = "资源权限管理", action = "查询按钮资源绑定", biz_type = Query)]
#[get_api("/system/action-resource/action/{action_menu_id}")]
pub async fn get_action_resources(
    Component(svc): Component<SysResourceService>,
    Path(action_menu_id): Path<i64>,
) -> ApiResult<Json<ActionResourceVo>> {
    let item = svc.get_action_resources(action_menu_id).await?;
    Ok(Json(item))
}

#[log(module = "资源权限管理", action = "保存按钮资源绑定", biz_type = Update)]
#[put_api("/system/action-resource/action/{action_menu_id}")]
pub async fn save_action_resources(
    Component(svc): Component<SysResourceService>,
    Path(action_menu_id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<SaveActionResourcesDto>,
) -> ApiResult<()> {
    svc.save_action_resources(action_menu_id, dto).await?;
    Ok(())
}

#[log(module = "资源权限管理", action = "查询资源关联按钮", biz_type = Query)]
#[get_api("/system/action-resource/resource/{resource_id}")]
pub async fn get_resource_actions(
    Component(svc): Component<SysResourceService>,
    Path(resource_id): Path<i64>,
) -> ApiResult<Json<ResourceActionVo>> {
    let item = svc.get_resource_actions(resource_id).await?;
    Ok(Json(item))
}

#[log(module = "资源权限管理", action = "刷新资源权限策略", biz_type = Update)]
#[post_api("/system/resource-permission/reload")]
pub async fn reload_policy(Component(svc): Component<SysResourceService>) -> ApiResult<()> {
    svc.reload_policy().await?;
    Ok(())
}
