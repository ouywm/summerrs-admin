use crate::entity::sys_menu::{self, MenuType};
use schemars::JsonSchema;
use sea_orm::{NotSet, Set};
use serde::{Deserialize, Serialize};
use validator::Validate;

/// 创建菜单 DTO（menu_type = 1）
#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateMenuDto {
    pub parent_id: Option<i64>,
    #[validate(length(min = 1, max = 64, message = "路由名称长度必须在1-64之间"))]
    pub name: String,
    #[validate(length(max = 256, message = "路由路径长度不能超过256"))]
    pub path: Option<String>,
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
            id: NotSet,
            parent_id: Set(dto.parent_id.unwrap_or(0)),
            menu_type: Set(MenuType::Menu),
            name: Set(dto.name),
            path: Set(dto.path.unwrap_or_default()),
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
            bit_position: Set(None),
            sort: Set(dto.sort.unwrap_or(0)),
            enabled: Set(dto.enabled.unwrap_or(true)),
            create_time: NotSet,
            update_time: NotSet,
        }
    }
}

/// 创建按钮 DTO（menu_type = 2）
#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
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
            id: NotSet,
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
            bit_position: Set(None),
            sort: Set(dto.sort.unwrap_or(0)),
            enabled: Set(dto.enabled.unwrap_or(true)),
            create_time: NotSet,
            update_time: NotSet,
        }
    }
}

/// 更新菜单 DTO
#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMenuDto {
    pub parent_id: Option<i64>,
    #[validate(length(min = 1, max = 64, message = "路由名称长度必须在1-64之间"))]
    pub name: Option<String>,
    #[validate(length(max = 256, message = "路由路径长度不能超过256"))]
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
#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
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
    pub fn validate_for_existing(&self, existing: &sys_menu::Model) -> Result<(), String> {
        self.validate().map_err(first_validation_message)?;

        let path = self.path.as_deref().unwrap_or(&existing.path);
        let link = self.link.as_deref().unwrap_or(&existing.link);
        let is_iframe = self.is_iframe.unwrap_or(existing.is_iframe);

        validate_menu_route_semantics(path, link, is_iframe)
    }

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

impl CreateMenuDto {
    pub fn validate_for_save(&self) -> Result<(), String> {
        self.validate().map_err(first_validation_message)?;

        validate_menu_route_semantics(
            self.path.as_deref().unwrap_or_default(),
            self.link.as_deref().unwrap_or_default(),
            self.is_iframe.unwrap_or(false),
        )
    }
}

fn validate_menu_route_semantics(path: &str, link: &str, is_iframe: bool) -> Result<(), String> {
    let has_path = !path.trim().is_empty();
    let has_link = !link.trim().is_empty();

    if has_link && !is_iframe {
        return Ok(());
    }

    if !has_path {
        if is_iframe {
            return Err("内嵌菜单必须填写路由路径".to_string());
        }
        return Err("路由路径不能为空".to_string());
    }

    Ok(())
}

fn first_validation_message(errors: validator::ValidationErrors) -> String {
    for (_, field_errors) in errors.field_errors() {
        if let Some(first_error) = field_errors.first()
            && let Some(message) = &first_error.message
        {
            return message.to_string();
        }
    }

    "验证失败".to_string()
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

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm::ActiveValue;

    fn base_create_menu() -> CreateMenuDto {
        CreateMenuDto {
            parent_id: Some(0),
            name: "ExternalDocs".to_string(),
            path: Some(String::new()),
            component: None,
            redirect: None,
            icon: None,
            title: "外部文档".to_string(),
            link: Some("https://example.com".to_string()),
            is_iframe: Some(false),
            is_hide: None,
            is_hide_tab: None,
            is_full_page: None,
            is_first_level: None,
            keep_alive: None,
            fixed_tab: None,
            show_badge: None,
            show_text_badge: None,
            active_path: None,
            sort: Some(1),
            enabled: Some(true),
        }
    }

    fn existing_external_link() -> sys_menu::Model {
        let now = chrono::NaiveDate::from_ymd_opt(2026, 6, 6)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        sys_menu::Model {
            id: 900,
            parent_id: 0,
            menu_type: MenuType::Menu,
            name: "ExternalDocs".to_string(),
            path: String::new(),
            component: String::new(),
            redirect: String::new(),
            icon: String::new(),
            title: "外部文档".to_string(),
            link: "https://example.com".to_string(),
            is_iframe: false,
            is_hide: false,
            is_hide_tab: false,
            is_full_page: false,
            is_first_level: false,
            keep_alive: false,
            fixed_tab: false,
            show_badge: false,
            show_text_badge: String::new(),
            active_path: String::new(),
            auth_name: String::new(),
            auth_mark: String::new(),
            bit_position: None,
            sort: 1,
            enabled: true,
            create_time: now,
            update_time: now,
        }
    }

    fn existing_button() -> sys_menu::Model {
        let now = chrono::NaiveDate::from_ymd_opt(2026, 6, 6)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        sys_menu::Model {
            id: 901,
            parent_id: 1,
            menu_type: MenuType::Button,
            name: String::new(),
            path: String::new(),
            component: String::new(),
            redirect: String::new(),
            icon: String::new(),
            title: "新增".to_string(),
            link: String::new(),
            is_iframe: false,
            is_hide: false,
            is_hide_tab: false,
            is_full_page: false,
            is_first_level: false,
            keep_alive: false,
            fixed_tab: false,
            show_badge: false,
            show_text_badge: String::new(),
            active_path: String::new(),
            auth_name: "新增".to_string(),
            auth_mark: "system:user:add".to_string(),
            bit_position: Some(1),
            sort: 1,
            enabled: true,
            create_time: now,
            update_time: now,
        }
    }

    #[test]
    fn external_link_create_allows_empty_route_path() {
        let dto = base_create_menu();
        assert!(dto.validate_for_save().is_ok());
    }

    #[test]
    fn external_link_create_allows_missing_route_path() {
        let mut dto = base_create_menu();
        dto.path = None;
        assert!(dto.validate_for_save().is_ok());
    }

    #[test]
    fn normal_menu_create_requires_route_path() {
        let mut dto = base_create_menu();
        dto.link = None;
        dto.component = Some("/system/user".to_string());
        let err = dto.validate_for_save().unwrap_err();
        assert!(err.contains("路由路径"));
    }

    #[test]
    fn external_link_update_allows_empty_route_path() {
        let dto = UpdateMenuDto {
            parent_id: None,
            name: None,
            path: Some(String::new()),
            component: Some(String::new()),
            redirect: None,
            icon: None,
            title: None,
            link: None,
            is_iframe: None,
            is_hide: None,
            is_hide_tab: None,
            is_full_page: None,
            is_first_level: None,
            keep_alive: None,
            fixed_tab: None,
            show_badge: None,
            show_text_badge: None,
            active_path: None,
            sort: None,
            enabled: None,
        };
        assert!(dto.validate_for_existing(&existing_external_link()).is_ok());
    }

    #[test]
    fn button_create_does_not_validate_route_path() {
        let dto = CreateButtonDto {
            parent_id: 1,
            auth_name: "新增".to_string(),
            auth_mark: "system:user:add".to_string(),
            sort: Some(1),
            enabled: Some(true),
        };

        assert!(dto.validate().is_ok());

        let active: sys_menu::ActiveModel = dto.into();
        assert_eq!(active.path, ActiveValue::Set(String::new()));
        assert_eq!(active.menu_type, Set(MenuType::Button));
    }

    #[test]
    fn button_update_does_not_validate_route_path() {
        let dto = UpdateButtonDto {
            parent_id: None,
            auth_name: Some("编辑".to_string()),
            auth_mark: Some("system:user:edit".to_string()),
            sort: Some(2),
            enabled: Some(true),
        };

        assert!(dto.validate().is_ok());

        let mut active: sys_menu::ActiveModel = existing_button().into();
        dto.apply_to(&mut active);
        assert_eq!(active.path, ActiveValue::Unchanged(String::new()));
        assert_eq!(active.menu_type, ActiveValue::Unchanged(MenuType::Button));
    }
}
