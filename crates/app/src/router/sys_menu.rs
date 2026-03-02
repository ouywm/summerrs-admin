use common::error::ApiResult;
use common::extractor::{LoginIdExtractor, ValidatedJson};
use common::response::ApiResponse;
use macros::log;
use model::dto::sys_menu::{CreateButtonDto, CreateMenuDto, UpdateButtonDto, UpdateMenuDto};
use model::vo::sys_menu::MenuTreeVo;
use spring_web::axum::extract::Path;
use spring_web::extractor::Component;
use spring_web::{delete, get, post, put};

use crate::service::sys_menu_service::SysMenuService;

/// 获取当前用户的菜单树（前端路由）
#[log(module = "菜单管理", action = "获取菜单树", biz_type = Query)]
#[get("/v3/system/menus")]
pub async fn get_menu_tree(
    LoginIdExtractor(login_id): LoginIdExtractor,
    Component(svc): Component<SysMenuService>,
) -> ApiResult<ApiResponse<Vec<MenuTreeVo>>> {
    let vo = svc.get_menu_tree(&login_id).await?;
    Ok(ApiResponse::ok(vo))
}

/// 获取所有菜单列表（管理用）
#[log(module = "菜单管理", action = "查询菜单列表", biz_type = Query)]
#[get("/system/menu/list")]
pub async fn list_menus(
    Component(svc): Component<SysMenuService>,
) -> ApiResult<ApiResponse<Vec<MenuTreeVo>>> {
    let vo = svc.list_menus().await?;
    Ok(ApiResponse::ok(vo))
}

/// 创建菜单
#[log(module = "菜单管理", action = "创建菜单", biz_type = Create)]
#[post("/system/menu")]
pub async fn create_menu(
    Component(svc): Component<SysMenuService>,
    ValidatedJson(dto): ValidatedJson<CreateMenuDto>,
) -> ApiResult<ApiResponse<()>> {
    svc.create_menu(dto).await?;
    Ok(ApiResponse::empty_with_msg("创建成功"))
}

/// 创建按钮
#[log(module = "菜单管理", action = "创建按钮", biz_type = Create)]
#[post("/system/button")]
pub async fn create_button(
    Component(svc): Component<SysMenuService>,
    ValidatedJson(dto): ValidatedJson<CreateButtonDto>,
) -> ApiResult<ApiResponse<()>> {
    svc.create_button(dto).await?;
    Ok(ApiResponse::empty_with_msg("创建成功"))
}

/// 更新菜单
#[log(module = "菜单管理", action = "更新菜单", biz_type = Update)]
#[put("/system/menu/{id}")]
pub async fn update_menu(
    Component(svc): Component<SysMenuService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdateMenuDto>,
) -> ApiResult<ApiResponse<()>> {
    svc.update_menu(id, dto).await?;
    Ok(ApiResponse::empty_with_msg("更新成功"))
}

/// 更新按钮
#[log(module = "菜单管理", action = "更新按钮", biz_type = Update)]
#[put("/system/button/{id}")]
pub async fn update_button(
    Component(svc): Component<SysMenuService>,
    Path(id): Path<i64>,
    ValidatedJson(dto): ValidatedJson<UpdateButtonDto>,
) -> ApiResult<ApiResponse<()>> {
    svc.update_button(id, dto).await?;
    Ok(ApiResponse::empty_with_msg("更新成功"))
}

/// 删除菜单/按钮
#[log(module = "菜单管理", action = "删除菜单", biz_type = Delete)]
#[delete("/system/menu/{id}")]
pub async fn delete_menu(
    Component(svc): Component<SysMenuService>,
    Path(id): Path<i64>,
) -> ApiResult<ApiResponse<()>> {
    svc.delete_menu(id).await?;
    Ok(ApiResponse::empty_with_msg("删除成功"))
}
