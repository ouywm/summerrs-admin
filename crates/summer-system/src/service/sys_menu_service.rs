use std::sync::Arc;

use summer::plugin::Service;
use summer_auth::{LoginId, PermissionMap, SessionManager};
use summer_common::error::ApiResult;
use summer_domain::menu::{MenuDomainService, PermissionMapSink};
use summer_system_model::dto::sys_menu::{
    CreateButtonDto, CreateMenuDto, UpdateButtonDto, UpdateMenuDto,
};
use summer_system_model::vo::sys_menu::{MenuTreeVo, MenuVo};

use summer_sea_orm::DbConn;

#[derive(Clone, Service)]
pub struct SysMenuService {
    #[inject(component)]
    db: DbConn,
    #[inject(component)]
    auth: SessionManager,
}

impl SysMenuService {
    pub async fn get_menu_tree(&self, login_id: &LoginId) -> ApiResult<Vec<MenuTreeVo>> {
        self.domain()
            .get_menu_tree_for_user_id(login_id.user_id)
            .await
    }

    pub async fn list_menus(&self) -> ApiResult<Vec<MenuTreeVo>> {
        self.domain().list_menus().await
    }

    pub async fn create_menu(&self, dto: CreateMenuDto) -> ApiResult<MenuVo> {
        self.domain().create_menu(dto).await
    }

    pub async fn create_button(&self, dto: CreateButtonDto) -> ApiResult<MenuVo> {
        self.domain().create_button(dto).await
    }

    pub async fn update_menu(&self, id: i64, dto: UpdateMenuDto) -> ApiResult<MenuVo> {
        self.domain().update_menu(id, dto).await
    }

    pub async fn update_button(&self, id: i64, dto: UpdateButtonDto) -> ApiResult<MenuVo> {
        self.domain().update_button(id, dto).await
    }

    pub async fn delete_menu(&self, id: i64) -> ApiResult<i64> {
        self.domain().delete_menu(id).await
    }

    fn domain(&self) -> MenuDomainService {
        MenuDomainService::with_permission_map_sink(
            self.db.clone(),
            Arc::new(SessionPermissionMapSink {
                auth: self.auth.clone(),
            }),
        )
    }
}

struct SessionPermissionMapSink {
    auth: SessionManager,
}

impl PermissionMapSink for SessionPermissionMapSink {
    fn replace_permission_map(&self, mappings: Vec<(String, u32)>) {
        self.auth.set_permission_map(PermissionMap::new(mappings));
    }
}
