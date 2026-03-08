use common::error::ApiResult;
use common::extractor::{Path, ValidatedJson};
use macros::log;
use model::dto::sys_menu::{CreateButtonDto, CreateMenuDto, UpdateButtonDto, UpdateMenuDto};
use model::vo::sys_menu::MenuTreeVo;
use common::response::Json;
use summer_auth::AdminUser;
use summer_web::extractor::Component;
use summer_web::{delete_api, get_api, post_api, put_api};

use crate::service::sys_menu_service::SysMenuService;

/// 获取当前用户的菜单树（前端路由）
#[log(module = "菜单管理", action = "获取菜单树", biz_type = Query)]
#[get_api("/v3/system/menus")]
pub async fn get_menu_tree(
    AdminUser { login_id, .. }: AdminUser,
    Component(svc): Component<SysMenuService>,
) -> ApiResult<Json<Vec<MenuTreeVo>>> {
    let vo = svc.get_menu_tree(&login_id).await?;
    Ok(Json(vo))
}

/// 获取所有菜单列表（管理用）
#[log(module = "菜单管理", action = "查询菜单列表", biz_type = Query)]
#[get_api("/system/menu/list")]
pub async fn list_menus(
    Component(svc): Component<SysMenuService>,
) -> ApiResult<Json<Vec<MenuTreeVo>>> {
    let vo = svc.list_menus().await?;
    Ok(Json(vo))
}

/// 创建菜单
#[log(module = "菜单管理", action = "创建菜单", biz_type = Create)]
#[post_api("/system/menu")]
pub async fn create_menu(
    Component(svc): Component<SysMenuService>,
    ValidatedJson(dto): ValidatedJson<CreateMenuDto>,
) -> ApiResult<()> {
    svc.create_menu(dto).await?;
    Ok(())
}

/// 创建按钮
#[log(module = "菜单管理", action = "创建按钮", biz_type = Create)]
#[post_api("/system/button")]
pub async fn create_button(
    Component(svc): Component<SysMenuService>,
    ValidatedJson(dto): ValidatedJson<CreateButtonDto>,
) -> ApiResult<()> {
    svc.create_button(dto).await?;
    Ok(())
}

/// 更新菜单
#[log(module = "菜单管理", action = "更新菜单", biz_type = Update)]
#[put_api("/system/menu/{id}")]
pub async fn update_menu(
    Component(svc): Component<SysMenuService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdateMenuDto>,
) -> ApiResult<()> {
    svc.update_menu(id, dto).await?;
    Ok(())
}

/// 更新按钮
#[log(module = "菜单管理", action = "更新按钮", biz_type = Update)]
#[put_api("/system/button/{id}")]
pub async fn update_button(
    Component(svc): Component<SysMenuService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdateButtonDto>,
) -> ApiResult<()> {
    svc.update_button(id, dto).await?;
    Ok(())
}

/// 删除菜单/按钮
#[log(module = "菜单管理", action = "删除菜单", biz_type = Delete)]
#[delete_api("/system/menu/{id}")]
pub async fn delete_menu(
    Component(svc): Component<SysMenuService>,
    Path(id): Path<i64>,
) -> ApiResult<()> {
    svc.delete_menu(id).await?;
    Ok(())
}
