use chrono::NaiveDateTime;
use common::serde_utils::datetime_format;
use schemars::JsonSchema;

use serde::Serialize;

use crate::entity::sys_menu::{self, MenuType};

/// 菜单元数据（前端路由 meta）
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MenuMeta {
    pub title: String,
    pub icon: String,
    pub is_hide: bool,
    pub is_hide_tab: bool,
    pub link: String,
    pub is_iframe: bool,
    pub keep_alive: bool,
    pub roles: Vec<String>,
    pub is_first_level: bool,
    pub fixed_tab: bool,
    pub active_path: String,
    pub is_full_page: bool,
    pub show_badge: bool,
    pub show_text_badge: String,
    pub sort: i32,
    pub enabled: bool,
    pub auth_list: Vec<AuthItem>,
}

/// 按钮权限项
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AuthItem {
    pub id: i64,
    pub parent_id: i64,
    pub title: String,
    pub auth_name: String,
    pub auth_mark: String,
    pub sort: i32,
    pub enabled: bool,
    #[serde(serialize_with = "datetime_format::serialize")]
    pub create_time: NaiveDateTime,
    #[serde(serialize_with = "datetime_format::serialize")]
    pub update_time: NaiveDateTime,
}

/// 菜单树（前端路由结构）
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MenuTreeVo {
    pub id: i64,
    pub parent_id: i64,
    pub menu_type: MenuType,
    pub path: String,
    pub name: String,
    pub component: String,
    pub redirect: String,
    pub meta: MenuMeta,
    #[serde(serialize_with = "datetime_format::serialize")]
    pub create_time: NaiveDateTime,
    #[serde(serialize_with = "datetime_format::serialize")]
    pub update_time: NaiveDateTime,
    #[schemars(skip)]
    pub children: Vec<MenuTreeVo>,
}

/// 菜单管理 CRUD 扁平结构
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MenuVo {
    pub id: i64,
    pub parent_id: i64,
    pub menu_type: MenuType,
    pub name: String,
    pub path: String,
    pub component: String,
    pub redirect: String,
    pub icon: String,
    pub title: String,
    pub link: String,
    pub is_iframe: bool,
    pub is_hide: bool,
    pub is_hide_tab: bool,
    pub is_full_page: bool,
    pub is_first_level: bool,
    pub keep_alive: bool,
    pub fixed_tab: bool,
    pub show_badge: bool,
    pub show_text_badge: String,
    pub active_path: String,
    pub auth_name: String,
    pub auth_mark: String,
    pub sort: i32,
    pub enabled: bool,
    #[serde(serialize_with = "datetime_format::serialize")]
    pub create_time: NaiveDateTime,
    #[serde(serialize_with = "datetime_format::serialize")]
    pub update_time: NaiveDateTime,
}

impl From<sys_menu::Model> for MenuVo {
    fn from(m: sys_menu::Model) -> Self {
        Self {
            id: m.id,
            parent_id: m.parent_id,
            menu_type: m.menu_type,
            name: m.name,
            path: m.path,
            component: m.component,
            redirect: m.redirect,
            icon: m.icon,
            title: m.title,
            link: m.link,
            is_iframe: m.is_iframe,
            is_hide: m.is_hide,
            is_hide_tab: m.is_hide_tab,
            is_full_page: m.is_full_page,
            is_first_level: m.is_first_level,
            keep_alive: m.keep_alive,
            fixed_tab: m.fixed_tab,
            show_badge: m.show_badge,
            show_text_badge: m.show_text_badge,
            active_path: m.active_path,
            auth_name: m.auth_name,
            auth_mark: m.auth_mark,
            sort: m.sort,
            enabled: m.enabled,
            create_time: m.create_time,
            update_time: m.update_time,
        }
    }
}
