use crate::entity::sys_menu::{self, MenuType};
use schemars::JsonSchema;
use sea_orm::Set;
use serde::Deserialize;
use validator::Validate;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MenuQueryDto {
    pub name: Option<String>,
    pub path: Option<String>,
    pub title: Option<String>,
    pub menu_type: Option<MenuType>,
    pub enabled: Option<bool>,
}

/// 创建菜单 DTO（menu_type = 1）
#[derive(Debug, Deserialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateMenuDto {
    pub parent_id: Option<i64>,
    #[validate(length(min = 1, max = 64, message = "路由名称长度必须在1-64之间"))]
    pub name: String,
    #[validate(length(min = 1, max = 256, message = "路由路径长度必须在1-256之间"))]
    pub path: String,
    #[validate(length(max = 256, message = "组件路径长度不能超过256"))]
    pub component: Option<String>,
    #[validate(length(max = 256, message = "重定向路径长度不能超过256"))]
    pub redirect: Option<String>,
    #[validate(length(max = 64, message = "图标长度不能超过64"))]
    pub icon: Option<String>,
    #[validate(length(min = 1, max = 64, message = "菜单标题长度必须在1-64之间"))]
    pub title: String,
    #[validate(length(max = 512, message = "外链地址长度不能超过512"))]
    pub link: Option<String>,
    pub is_iframe: Option<bool>,
    pub is_hide: Option<bool>,
    pub is_hide_tab: Option<bool>,
    pub is_full_page: Option<bool>,
    pub is_first_level: Option<bool>,
    pub keep_alive: Option<bool>,
    pub fixed_tab: Option<bool>,
    pub show_badge: Option<bool>,
    #[validate(length(max = 32, message = "文字徽标长度不能超过32"))]
    pub show_text_badge: Option<String>,
    #[validate(length(max = 256, message = "高亮路径长度不能超过256"))]
    pub active_path: Option<String>,
    pub sort: Option<i32>,
    pub enabled: Option<bool>,
}

impl From<CreateMenuDto> for sys_menu::ActiveModel {
    fn from(dto: CreateMenuDto) -> Self {
        Self {
            parent_id: Set(dto.parent_id.unwrap_or(0)),
            menu_type: Set(MenuType::Menu),
            name: Set(dto.name),
            path: Set(dto.path),
            component: Set(dto.component.unwrap_or_default()),
            redirect: Set(dto.redirect.unwrap_or_default()),
            icon: Set(dto.icon.unwrap_or_default()),
            title: Set(dto.title),
            link: Set(dto.link.unwrap_or_default()),
            is_iframe: Set(dto.is_iframe.unwrap_or(false)),
            is_hide: Set(dto.is_hide.unwrap_or(false)),
            is_hide_tab: Set(dto.is_hide_tab.unwrap_or(false)),
            is_full_page: Set(dto.is_full_page.unwrap_or(false)),
            is_first_level: Set(dto.is_first_level.unwrap_or(false)),
            keep_alive: Set(dto.keep_alive.unwrap_or(false)),
            fixed_tab: Set(dto.fixed_tab.unwrap_or(false)),
            show_badge: Set(dto.show_badge.unwrap_or(false)),
            show_text_badge: Set(dto.show_text_badge.unwrap_or_default()),
            active_path: Set(dto.active_path.unwrap_or_default()),
            auth_name: Set(String::new()),
            auth_mark: Set(String::new()),
            sort: Set(dto.sort.unwrap_or(0)),
            enabled: Set(dto.enabled.unwrap_or(true)),
            ..Default::default()
        }
    }
}

/// 创建按钮 DTO（menu_type = 2）
#[derive(Debug, Deserialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateButtonDto {
    #[validate(range(min = 1, message = "父菜单ID必须大于0"))]
    pub parent_id: i64,
    #[validate(length(min = 1, max = 64, message = "权限名称长度必须在1-64之间"))]
    pub auth_name: String,
    #[validate(length(min = 1, max = 64, message = "权限标识长度必须在1-64之间"))]
    pub auth_mark: String,
    pub sort: Option<i32>,
    pub enabled: Option<bool>,
}

impl From<CreateButtonDto> for sys_menu::ActiveModel {
    fn from(dto: CreateButtonDto) -> Self {
        Self {
            parent_id: Set(dto.parent_id),
            menu_type: Set(MenuType::Button),
            name: Set(String::new()),
            path: Set(String::new()),
            component: Set(String::new()),
            redirect: Set(String::new()),
            icon: Set(String::new()),
            title: Set(dto.auth_name.clone()),
            link: Set(String::new()),
            is_iframe: Set(false),
            is_hide: Set(false),
            is_hide_tab: Set(false),
            is_full_page: Set(false),
            is_first_level: Set(false),
            keep_alive: Set(false),
            fixed_tab: Set(false),
            show_badge: Set(false),
            show_text_badge: Set(String::new()),
            active_path: Set(String::new()),
            auth_name: Set(dto.auth_name),
            auth_mark: Set(dto.auth_mark),
            sort: Set(dto.sort.unwrap_or(0)),
            enabled: Set(dto.enabled.unwrap_or(true)),
            ..Default::default()
        }
    }
}

/// 更新菜单 DTO
#[derive(Debug, Deserialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMenuDto {
    pub parent_id: Option<i64>,
    #[validate(length(min = 1, max = 64, message = "路由名称长度必须在1-64之间"))]
    pub name: Option<String>,
    #[validate(length(min = 1, max = 256, message = "路由路径长度必须在1-256之间"))]
    pub path: Option<String>,
    #[validate(length(max = 256, message = "组件路径长度不能超过256"))]
    pub component: Option<String>,
    #[validate(length(max = 256, message = "重定向路径长度不能超过256"))]
    pub redirect: Option<String>,
    #[validate(length(max = 64, message = "图标长度不能超过64"))]
    pub icon: Option<String>,
    #[validate(length(min = 1, max = 64, message = "菜单标题长度必须在1-64之间"))]
    pub title: Option<String>,
    #[validate(length(max = 512, message = "外链地址长度不能超过512"))]
    pub link: Option<String>,
    pub is_iframe: Option<bool>,
    pub is_hide: Option<bool>,
    pub is_hide_tab: Option<bool>,
    pub is_full_page: Option<bool>,
    pub is_first_level: Option<bool>,
    pub keep_alive: Option<bool>,
    pub fixed_tab: Option<bool>,
    pub show_badge: Option<bool>,
    #[validate(length(max = 32, message = "文字徽标长度不能超过32"))]
    pub show_text_badge: Option<String>,
    #[validate(length(max = 256, message = "高亮路径长度不能超过256"))]
    pub active_path: Option<String>,
    pub sort: Option<i32>,
    pub enabled: Option<bool>,
}

/// 更新按钮 DTO
#[derive(Debug, Deserialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateButtonDto {
    pub parent_id: Option<i64>,
    #[validate(length(min = 1, max = 64, message = "权限名称长度必须在1-64之间"))]
    pub auth_name: Option<String>,
    #[validate(length(min = 1, max = 64, message = "权限标识长度必须在1-64之间"))]
    pub auth_mark: Option<String>,
    pub sort: Option<i32>,
    pub enabled: Option<bool>,
}

impl UpdateMenuDto {
    /// 将 DTO 中的非空字段应用到 ActiveModel
    pub fn apply_to(self, active: &mut sys_menu::ActiveModel) {
        if let Some(parent_id) = self.parent_id {
            active.parent_id = Set(parent_id);
        }
        if let Some(name) = self.name {
            active.name = Set(name);
        }
        if let Some(path) = self.path {
            active.path = Set(path);
        }
        if let Some(component) = self.component {
            active.component = Set(component);
        }
        if let Some(redirect) = self.redirect {
            active.redirect = Set(redirect);
        }
        if let Some(icon) = self.icon {
            active.icon = Set(icon);
        }
        if let Some(title) = self.title {
            active.title = Set(title);
        }
        if let Some(link) = self.link {
            active.link = Set(link);
        }
        if let Some(is_iframe) = self.is_iframe {
            active.is_iframe = Set(is_iframe);
        }
        if let Some(is_hide) = self.is_hide {
            active.is_hide = Set(is_hide);
        }
        if let Some(is_hide_tab) = self.is_hide_tab {
            active.is_hide_tab = Set(is_hide_tab);
        }
        if let Some(is_full_page) = self.is_full_page {
            active.is_full_page = Set(is_full_page);
        }
        if let Some(is_first_level) = self.is_first_level {
            active.is_first_level = Set(is_first_level);
        }
        if let Some(keep_alive) = self.keep_alive {
            active.keep_alive = Set(keep_alive);
        }
        if let Some(fixed_tab) = self.fixed_tab {
            active.fixed_tab = Set(fixed_tab);
        }
        if let Some(show_badge) = self.show_badge {
            active.show_badge = Set(show_badge);
        }
        if let Some(show_text_badge) = self.show_text_badge {
            active.show_text_badge = Set(show_text_badge);
        }
        if let Some(active_path) = self.active_path {
            active.active_path = Set(active_path);
        }
        if let Some(sort) = self.sort {
            active.sort = Set(sort);
        }
        if let Some(enabled) = self.enabled {
            active.enabled = Set(enabled);
        }
    }
}

impl UpdateButtonDto {
    /// 将 DTO 中的非空字段应用到 ActiveModel
    pub fn apply_to(self, active: &mut sys_menu::ActiveModel) {
        if let Some(parent_id) = self.parent_id {
            active.parent_id = Set(parent_id);
        }
        if let Some(auth_name) = self.auth_name {
            active.auth_name = Set(auth_name.clone());
            active.title = Set(auth_name);
        }
        if let Some(auth_mark) = self.auth_mark {
            active.auth_mark = Set(auth_mark);
        }
        if let Some(sort) = self.sort {
            active.sort = Set(sort);
        }
        if let Some(enabled) = self.enabled {
            active.enabled = Set(enabled);
        }
    }
}
