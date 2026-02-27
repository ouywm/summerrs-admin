use anyhow::Context;
use common::error::{ApiErrors, ApiResult};
use model::dto::sys_menu::{CreateButtonDto, CreateMenuDto, UpdateButtonDto, UpdateMenuDto};
use model::entity::sys_menu;
use model::entity::sys_role_menu;
use model::entity::sys_user_role;
use model::vo::sys_menu::{AuthItem, MenuMeta, MenuTreeVo};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, JoinType, Order, PaginatorTrait, QueryFilter,
    QueryOrder, QuerySelect, RelationTrait,
};
use spring::plugin::Service;

use crate::plugin::sea_orm_plugin::DbConn;

#[derive(Clone, Service)]
pub struct SysMenuService {
    #[inject(component)]
    db: DbConn,
}

impl SysMenuService {
    /// 获取当前用户的菜单树（前端路由）
    pub async fn get_menu_tree(&self, login_id: &str) -> ApiResult<Vec<MenuTreeVo>> {
        let user_id: i64 = login_id
            .parse()
            .map_err(|_| ApiErrors::BadRequest("无效的用户ID".to_string()))?;

        // 查询用户角色
        let role_ids: Vec<i64> = sys_user_role::Entity::find()
            .filter(sys_user_role::Column::UserId.eq(user_id))
            .all(&self.db)
            .await
            .context("查询用户角色失败")?
            .into_iter()
            .map(|ur| ur.role_id)
            .collect();

        if role_ids.is_empty() {
            return Ok(vec![]);
        }

        // 查询角色关联的所有已启用菜单
        let menus = sys_menu::Entity::find()
            .join(JoinType::InnerJoin, sys_menu::Relation::SysRoleMenu.def())
            .filter(sys_role_menu::Column::RoleId.is_in(role_ids.clone()))
            .filter(sys_menu::Column::Enabled.eq(true))
            .order_by(sys_menu::Column::Sort, Order::Asc)
            .all(&self.db)
            .await
            .context("查询菜单失败")?;

        // 去重
        let mut seen = std::collections::HashSet::new();
        let menus: Vec<sys_menu::Model> = menus.into_iter().filter(|m| seen.insert(m.id)).collect();

        // 查询角色编码（用于 meta.roles）
        let role_codes: Vec<String> = model::entity::sys_role::Entity::find()
            .filter(model::entity::sys_role::Column::Id.is_in(role_ids))
            .all(&self.db)
            .await
            .context("查询角色编码失败")?
            .into_iter()
            .map(|r| r.role_code)
            .collect();

        Ok(build_menu_tree(&menus, 0, &role_codes))
    }

    /// 获取所有菜单列表（树形结构，管理用）
    pub async fn list_menus(&self) -> ApiResult<Vec<MenuTreeVo>> {
        let menus = sys_menu::Entity::find()
            .order_by(sys_menu::Column::Sort, Order::Asc)
            .all(&self.db)
            .await
            .context("查询菜单失败")?;

        // 构建树形结构（管理用，不需要角色信息）
        Ok(build_menu_tree(&menus, 0, &[]))
    }

    /// 创建菜单
    pub async fn create_menu(&self, dto: CreateMenuDto) -> ApiResult<()> {
        // 检查菜单名称是否重复
        let existing = sys_menu::Entity::find()
            .filter(sys_menu::Column::Name.eq(&dto.name))
            .filter(sys_menu::Column::MenuType.eq(sys_menu::MenuType::Menu))
            .one(&self.db)
            .await
            .context("查询菜单失败")?;

        if existing.is_some() {
            return Err(ApiErrors::BadRequest(format!(
                "菜单名称 '{}' 已存在",
                dto.name
            )));
        }

        let menu: sys_menu::ActiveModel = dto.into();
        menu.insert(&self.db).await.context("创建菜单失败")?;
        Ok(())
    }

    /// 创建按钮
    pub async fn create_button(&self, dto: CreateButtonDto) -> ApiResult<()> {
        // 检查父菜单是否存在
        let parent = sys_menu::Entity::find_by_id(dto.parent_id)
            .one(&self.db)
            .await
            .context("查询父菜单失败")?;

        if parent.is_none() {
            return Err(ApiErrors::BadRequest("父菜单不存在".to_string()));
        }

        let button: sys_menu::ActiveModel = dto.into();
        button.insert(&self.db).await.context("创建按钮失败")?;
        Ok(())
    }

    /// 更新菜单
    pub async fn update_menu(&self, id: i64, dto: UpdateMenuDto) -> ApiResult<()> {
        let menu = sys_menu::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询菜单失败")?
            .ok_or_else(|| ApiErrors::NotFound("菜单不存在".to_string()))?;

        // 如果修改了菜单名称，检查是否重复
        if let Some(ref new_name) = dto.name {
            let existing = sys_menu::Entity::find()
                .filter(sys_menu::Column::Name.eq(new_name))
                .filter(sys_menu::Column::MenuType.eq(sys_menu::MenuType::Menu))
                .filter(sys_menu::Column::Id.ne(id))
                .one(&self.db)
                .await
                .context("查询菜单失败")?;

            if existing.is_some() {
                return Err(ApiErrors::BadRequest(format!(
                    "菜单名称 '{}' 已存在",
                    new_name
                )));
            }
        }

        let mut active: sys_menu::ActiveModel = menu.into();
        dto.apply_to(&mut active);
        active.update(&self.db).await.context("更新菜单失败")?;
        Ok(())
    }

    /// 更新按钮
    pub async fn update_button(&self, id: i64, dto: UpdateButtonDto) -> ApiResult<()> {
        let button = sys_menu::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询按钮失败")?
            .ok_or_else(|| ApiErrors::NotFound("按钮不存在".to_string()))?;

        let mut active: sys_menu::ActiveModel = button.into();
        dto.apply_to(&mut active);
        active.update(&self.db).await.context("更新按钮失败")?;
        Ok(())
    }

    /// 删除菜单
    pub async fn delete_menu(&self, id: i64) -> ApiResult<()> {
        // 检查是否有子菜单
        let children = sys_menu::Entity::find()
            .filter(sys_menu::Column::ParentId.eq(id))
            .count(&self.db)
            .await
            .context("查询子菜单失败")?;

        if children > 0 {
            return Err(ApiErrors::BadRequest(
                "存在子菜单，请先删除子菜单".to_string(),
            ));
        }

        // 检查是否有角色绑定
        let role_bindings = sys_role_menu::Entity::find()
            .filter(sys_role_menu::Column::MenuId.eq(id))
            .count(&self.db)
            .await
            .context("查询角色菜单关联失败")?;

        if role_bindings > 0 {
            return Err(ApiErrors::BadRequest(
                "该菜单已被角色使用，请先解除角色绑定".to_string(),
            ));
        }

        // 删除菜单
        let result = sys_menu::Entity::delete_by_id(id)
            .exec(&self.db)
            .await
            .context("删除菜单失败")?;

        if result.rows_affected == 0 {
            return Err(ApiErrors::NotFound("菜单不存在".to_string()));
        }

        Ok(())
    }
}

/// 递归构建菜单树
fn build_menu_tree(
    menus: &[sys_menu::Model],
    parent_id: i64,
    role_codes: &[String],
) -> Vec<MenuTreeVo> {
    menus
        .iter()
        .filter(|m| m.parent_id == parent_id && m.menu_type == sys_menu::MenuType::Menu)
        .map(|menu| {
            let auth_list: Vec<AuthItem> = menus
                .iter()
                .filter(|m| m.parent_id == menu.id && m.menu_type == sys_menu::MenuType::Button)
                .map(|m| AuthItem {
                    id: m.id,
                    parent_id: m.parent_id,
                    title: m.title.clone(),
                    auth_name: m.auth_name.clone(),
                    auth_mark: m.auth_mark.clone(),
                    sort: m.sort,
                    enabled: m.enabled,
                    create_time: m.create_time,
                    update_time: m.update_time,
                })
                .collect();

            let children = build_menu_tree(menus, menu.id, role_codes);

            MenuTreeVo {
                id: menu.id,
                parent_id: menu.parent_id,
                menu_type: menu.menu_type,
                path: menu.path.clone(),
                name: menu.name.clone(),
                component: menu.component.clone(),
                redirect: menu.redirect.clone(),
                meta: MenuMeta {
                    title: menu.title.clone(),
                    icon: menu.icon.clone(),
                    is_hide: menu.is_hide,
                    is_hide_tab: menu.is_hide_tab,
                    link: menu.link.clone(),
                    is_iframe: menu.is_iframe,
                    keep_alive: menu.keep_alive,
                    roles: role_codes.to_vec(),
                    is_first_level: menu.is_first_level,
                    fixed_tab: menu.fixed_tab,
                    active_path: menu.active_path.clone(),
                    is_full_page: menu.is_full_page,
                    show_badge: menu.show_badge,
                    show_text_badge: menu.show_text_badge.clone(),
                    sort: menu.sort,
                    enabled: menu.enabled,
                    auth_list,
                },
                create_time: menu.create_time,
                update_time: menu.update_time,
                children,
            }
        })
        .collect()
}
