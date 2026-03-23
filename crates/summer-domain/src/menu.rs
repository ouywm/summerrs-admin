use std::{collections::HashSet, future::Future, pin::Pin, sync::Arc};

use anyhow::Context;
use schemars::JsonSchema;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, JoinType,
    Order, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, RelationTrait, Set,
    TransactionTrait,
};
use serde::{Deserialize, Serialize};
use summer_common::error::{ApiErrors, ApiResult};
use summer_system_model::{
    dto::sys_menu::{CreateButtonDto, CreateMenuDto, UpdateButtonDto, UpdateMenuDto},
    entity::{sys_menu, sys_role, sys_role_menu, sys_user_role},
    vo::sys_menu::{AuthItem, MenuMeta, MenuTreeVo, MenuVo},
};

use crate::sync::{SyncAction, SyncChange, SyncPlan};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MenuButtonSpec {
    pub auth_name: String,
    pub auth_mark: String,
    pub sort: Option<i32>,
    pub enabled: Option<bool>,
}

impl MenuButtonSpec {
    fn desired_sort(&self) -> i32 {
        self.sort.unwrap_or(0)
    }

    fn desired_enabled(&self) -> bool {
        self.enabled.unwrap_or(true)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MenuNodeSpec {
    pub name: String,
    pub path: String,
    pub component: Option<String>,
    pub redirect: Option<String>,
    pub icon: Option<String>,
    pub title: String,
    pub link: Option<String>,
    pub is_iframe: Option<bool>,
    pub is_hide: Option<bool>,
    pub is_hide_tab: Option<bool>,
    pub is_full_page: Option<bool>,
    pub is_first_level: Option<bool>,
    pub keep_alive: Option<bool>,
    pub fixed_tab: Option<bool>,
    pub show_badge: Option<bool>,
    pub show_text_badge: Option<String>,
    pub active_path: Option<String>,
    pub sort: Option<i32>,
    pub enabled: Option<bool>,
    #[serde(default)]
    pub buttons: Vec<MenuButtonSpec>,
    #[serde(default)]
    pub children: Vec<MenuNodeSpec>,
}

impl MenuNodeSpec {
    fn desired_component(&self) -> String {
        self.component.clone().unwrap_or_default()
    }

    fn desired_redirect(&self) -> String {
        self.redirect.clone().unwrap_or_default()
    }

    fn desired_icon(&self) -> String {
        self.icon.clone().unwrap_or_default()
    }

    fn desired_link(&self) -> String {
        self.link.clone().unwrap_or_default()
    }

    fn desired_is_iframe(&self) -> bool {
        self.is_iframe.unwrap_or(false)
    }

    fn desired_is_hide(&self) -> bool {
        self.is_hide.unwrap_or(false)
    }

    fn desired_is_hide_tab(&self) -> bool {
        self.is_hide_tab.unwrap_or(false)
    }

    fn desired_is_full_page(&self) -> bool {
        self.is_full_page.unwrap_or(false)
    }

    fn desired_is_first_level(&self) -> bool {
        self.is_first_level.unwrap_or(false)
    }

    fn desired_keep_alive(&self) -> bool {
        self.keep_alive.unwrap_or(false)
    }

    fn desired_fixed_tab(&self) -> bool {
        self.fixed_tab.unwrap_or(false)
    }

    fn desired_show_badge(&self) -> bool {
        self.show_badge.unwrap_or(false)
    }

    fn desired_show_text_badge(&self) -> String {
        self.show_text_badge.clone().unwrap_or_default()
    }

    fn desired_active_path(&self) -> String {
        self.active_path.clone().unwrap_or_default()
    }

    fn desired_sort(&self) -> i32 {
        self.sort.unwrap_or(0)
    }

    fn desired_enabled(&self) -> bool {
        self.enabled.unwrap_or(true)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MenuConfigSpec {
    #[serde(default)]
    pub menus: Vec<MenuNodeSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MenuConfigSyncResult {
    pub plan: SyncPlan,
}

pub trait PermissionMapSink: Send + Sync {
    fn replace_permission_map(&self, mappings: Vec<(String, u32)>);
}

#[derive(Default)]
pub struct NoopPermissionMapSink;

impl PermissionMapSink for NoopPermissionMapSink {
    fn replace_permission_map(&self, _mappings: Vec<(String, u32)>) {}
}

#[derive(Clone)]
pub struct MenuDomainService {
    db: DatabaseConnection,
    permission_map_sink: Arc<dyn PermissionMapSink>,
}

impl MenuDomainService {
    pub fn new(db: DatabaseConnection) -> Self {
        Self::with_permission_map_sink(db, Arc::new(NoopPermissionMapSink))
    }

    pub fn with_permission_map_sink(
        db: DatabaseConnection,
        permission_map_sink: Arc<dyn PermissionMapSink>,
    ) -> Self {
        Self {
            db,
            permission_map_sink,
        }
    }

    pub async fn get_menu_tree_for_user_id(&self, user_id: i64) -> ApiResult<Vec<MenuTreeVo>> {
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

        let menus = sys_menu::Entity::find()
            .join(JoinType::InnerJoin, sys_menu::Relation::SysRoleMenu.def())
            .filter(sys_role_menu::Column::RoleId.is_in(role_ids.clone()))
            .filter(sys_menu::Column::Enabled.eq(true))
            .order_by(sys_menu::Column::Sort, Order::Asc)
            .all(&self.db)
            .await
            .context("查询菜单失败")?;

        let mut seen = HashSet::new();
        let menus: Vec<sys_menu::Model> = menus.into_iter().filter(|m| seen.insert(m.id)).collect();

        let role_codes: Vec<String> = sys_role::Entity::find()
            .filter(sys_role::Column::Id.is_in(role_ids))
            .all(&self.db)
            .await
            .context("查询角色编码失败")?
            .into_iter()
            .map(|role| role.role_code)
            .collect();

        Ok(build_menu_tree(&menus, 0, &role_codes))
    }

    pub async fn list_menus(&self) -> ApiResult<Vec<MenuTreeVo>> {
        let menus = sys_menu::Entity::find()
            .order_by(sys_menu::Column::Sort, Order::Asc)
            .all(&self.db)
            .await
            .context("查询菜单失败")?;
        Ok(build_menu_tree(&menus, 0, &[]))
    }

    pub async fn plan_menu_config(&self, spec: &MenuConfigSpec) -> ApiResult<MenuConfigSyncResult> {
        validate_menu_config_spec(spec)?;

        let menus = sys_menu::Entity::find()
            .order_by(sys_menu::Column::Sort, Order::Asc)
            .all(&self.db)
            .await
            .context("查询菜单失败")?;

        build_menu_config_plan(&menus, spec)
    }

    pub async fn apply_menu_config(&self, spec: MenuConfigSpec) -> ApiResult<MenuConfigSyncResult> {
        validate_menu_config_spec(&spec)?;

        let (result, permission_changed) = self
            .db
            .transaction::<_, (MenuConfigSyncResult, bool), ApiErrors>(|txn| {
                let spec = spec.clone();
                Box::pin(async move {
                    let existing = sys_menu::Entity::find()
                        .order_by(sys_menu::Column::Sort, Order::Asc)
                        .all(txn)
                        .await
                        .context("查询菜单失败")
                        .map_err(ApiErrors::Internal)?;

                    let result = build_menu_config_plan(&existing, &spec)?;
                    let mut next_bit_position = next_button_bit_position(&existing);
                    let permission_changed = apply_menu_nodes(
                        txn,
                        &existing,
                        0,
                        "",
                        &spec.menus,
                        &mut next_bit_position,
                    )
                    .await?;

                    Ok((result, permission_changed))
                })
            })
            .await?;

        if permission_changed {
            self.reload_permission_map().await?;
        }

        Ok(result)
    }

    pub async fn create_menu(&self, dto: CreateMenuDto) -> ApiResult<MenuVo> {
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
        let model = menu.insert(&self.db).await.context("创建菜单失败")?;
        Ok(MenuVo::from(model))
    }

    pub async fn create_button(&self, dto: CreateButtonDto) -> ApiResult<MenuVo> {
        let parent = sys_menu::Entity::find_by_id(dto.parent_id)
            .one(&self.db)
            .await
            .context("查询父菜单失败")?;

        if parent.is_none() {
            return Err(ApiErrors::BadRequest("父菜单不存在".to_string()));
        }

        let max_pos: Option<Option<i32>> = sys_menu::Entity::find()
            .filter(sys_menu::Column::BitPosition.is_not_null())
            .select_only()
            .column_as(sys_menu::Column::BitPosition.max(), "max_pos")
            .into_tuple()
            .one(&self.db)
            .await
            .context("查询最大 bit_position 失败")?;
        let next_pos = max_pos.flatten().map(|p| p + 1).unwrap_or(0);

        let mut button: sys_menu::ActiveModel = dto.into();
        button.bit_position = Set(Some(next_pos));
        let model = button.insert(&self.db).await.context("创建按钮失败")?;

        self.reload_permission_map().await?;

        Ok(MenuVo::from(model))
    }

    pub async fn update_menu(&self, id: i64, dto: UpdateMenuDto) -> ApiResult<MenuVo> {
        let menu = sys_menu::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询菜单失败")?
            .ok_or_else(|| ApiErrors::NotFound("菜单不存在".to_string()))?;

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
        let model = active.update(&self.db).await.context("更新菜单失败")?;
        Ok(MenuVo::from(model))
    }

    pub async fn update_button(&self, id: i64, dto: UpdateButtonDto) -> ApiResult<MenuVo> {
        let button = sys_menu::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询按钮失败")?
            .ok_or_else(|| ApiErrors::NotFound("按钮不存在".to_string()))?;

        let need_reload = dto.auth_mark.is_some() || dto.enabled.is_some();

        let mut active: sys_menu::ActiveModel = button.into();
        dto.apply_to(&mut active);
        let model = active.update(&self.db).await.context("更新按钮失败")?;

        if need_reload {
            self.reload_permission_map().await?;
        }

        Ok(MenuVo::from(model))
    }

    pub async fn delete_menu(&self, id: i64) -> ApiResult<i64> {
        let menu = sys_menu::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询菜单失败")?
            .ok_or_else(|| ApiErrors::NotFound("菜单不存在".to_string()))?;

        let is_button = menu.menu_type == sys_menu::MenuType::Button;

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

        let result = sys_menu::Entity::delete_by_id(id)
            .exec(&self.db)
            .await
            .context("删除菜单失败")?;

        if result.rows_affected == 0 {
            return Err(ApiErrors::NotFound("菜单不存在".to_string()));
        }

        if is_button {
            self.reload_permission_map().await?;
        }

        Ok(id)
    }

    pub async fn reload_permission_map(&self) -> ApiResult<()> {
        let mappings = self.load_permission_mappings().await?;
        self.permission_map_sink.replace_permission_map(mappings);
        Ok(())
    }

    pub async fn load_permission_mappings(&self) -> ApiResult<Vec<(String, u32)>> {
        let menus = sys_menu::Entity::find()
            .filter(sys_menu::Column::MenuType.eq(sys_menu::MenuType::Button))
            .filter(sys_menu::Column::Enabled.eq(true))
            .filter(sys_menu::Column::BitPosition.is_not_null())
            .all(&self.db)
            .await
            .context("加载权限位图映射失败")?;

        Ok(menus
            .into_iter()
            .filter(|menu| !menu.auth_mark.is_empty())
            .filter_map(|menu| menu.bit_position.map(|pos| (menu.auth_mark, pos as u32)))
            .collect())
    }
}

fn validate_menu_config_spec(spec: &MenuConfigSpec) -> ApiResult<()> {
    let mut menu_names = HashSet::new();
    let mut auth_marks = HashSet::new();
    validate_menu_nodes(&spec.menus, "root", &mut menu_names, &mut auth_marks)
}

fn validate_menu_nodes(
    nodes: &[MenuNodeSpec],
    parent_key: &str,
    menu_names: &mut HashSet<String>,
    auth_marks: &mut HashSet<String>,
) -> ApiResult<()> {
    let mut sibling_paths = HashSet::new();

    for node in nodes {
        if node.name.trim().is_empty() {
            return Err(ApiErrors::BadRequest("menu.name 不能为空".to_string()));
        }
        if node.path.trim().is_empty() {
            return Err(ApiErrors::BadRequest("menu.path 不能为空".to_string()));
        }
        if node.title.trim().is_empty() {
            return Err(ApiErrors::BadRequest("menu.title 不能为空".to_string()));
        }
        if !sibling_paths.insert(node.path.clone()) {
            return Err(ApiErrors::BadRequest(format!(
                "同级菜单 path 重复: parent={parent_key}, path={}",
                node.path
            )));
        }
        if !menu_names.insert(node.name.clone()) {
            return Err(ApiErrors::BadRequest(format!(
                "菜单名称重复: {}",
                node.name
            )));
        }

        let node_key = menu_key(parent_key, &node.path, &node.name);
        let mut sibling_button_marks = HashSet::new();
        for button in &node.buttons {
            if button.auth_name.trim().is_empty() {
                return Err(ApiErrors::BadRequest(format!(
                    "button.auth_name 不能为空: menu={node_key}"
                )));
            }
            if button.auth_mark.trim().is_empty() {
                return Err(ApiErrors::BadRequest(format!(
                    "button.auth_mark 不能为空: menu={node_key}"
                )));
            }
            if !sibling_button_marks.insert(button.auth_mark.clone()) {
                return Err(ApiErrors::BadRequest(format!(
                    "同一菜单下 button.auth_mark 重复: menu={node_key}, auth_mark={}",
                    button.auth_mark
                )));
            }
            if !auth_marks.insert(button.auth_mark.clone()) {
                return Err(ApiErrors::BadRequest(format!(
                    "button.auth_mark 重复: {}",
                    button.auth_mark
                )));
            }
        }

        validate_menu_nodes(&node.children, &node_key, menu_names, auth_marks)?;
    }

    Ok(())
}

fn build_menu_config_plan(
    existing: &[sys_menu::Model],
    spec: &MenuConfigSpec,
) -> ApiResult<MenuConfigSyncResult> {
    validate_menu_config_spec(spec)?;

    let mut changes = Vec::new();
    plan_menu_nodes(existing, 0, "", &spec.menus, &mut changes)?;

    Ok(MenuConfigSyncResult {
        plan: SyncPlan::new(changes),
    })
}

fn plan_menu_nodes(
    existing: &[sys_menu::Model],
    parent_id: i64,
    parent_key: &str,
    nodes: &[MenuNodeSpec],
    changes: &mut Vec<SyncChange>,
) -> ApiResult<()> {
    for node in nodes {
        let key = menu_key(parent_key, &node.path, &node.name);
        let existing_menu = find_existing_menu(existing, parent_id, &node.path);
        ensure_menu_name_available(existing, existing_menu.map(|menu| menu.id), &node.name)?;

        changes.push(build_menu_change(existing_menu, &key, node));

        if let Some(existing_menu) = existing_menu {
            plan_button_specs(existing, existing_menu.id, &key, &node.buttons, changes)?;
            plan_menu_nodes(existing, existing_menu.id, &key, &node.children, changes)?;
        } else {
            plan_all_create_for_buttons(&key, &node.buttons, changes)?;
            plan_all_create_for_nodes(&key, &node.children, changes)?;
        }
    }

    Ok(())
}

fn plan_all_create_for_nodes(
    parent_key: &str,
    nodes: &[MenuNodeSpec],
    changes: &mut Vec<SyncChange>,
) -> ApiResult<()> {
    for node in nodes {
        let key = menu_key(parent_key, &node.path, &node.name);
        changes.push(build_menu_change(None, &key, node));
        plan_all_create_for_buttons(&key, &node.buttons, changes)?;
        plan_all_create_for_nodes(&key, &node.children, changes)?;
    }
    Ok(())
}

fn plan_button_specs(
    existing: &[sys_menu::Model],
    parent_id: i64,
    parent_key: &str,
    buttons: &[MenuButtonSpec],
    changes: &mut Vec<SyncChange>,
) -> ApiResult<()> {
    for button in buttons {
        let key = format!("{parent_key}#{}", button.auth_mark);
        let existing_button = find_existing_button(existing, parent_id, &button.auth_mark);
        ensure_button_auth_mark_available(
            existing,
            existing_button.map(|button| button.id),
            &button.auth_mark,
        )?;
        changes.push(build_button_change(existing_button, &key, button));
    }

    Ok(())
}

fn plan_all_create_for_buttons(
    parent_key: &str,
    buttons: &[MenuButtonSpec],
    changes: &mut Vec<SyncChange>,
) -> ApiResult<()> {
    for button in buttons {
        let key = format!("{parent_key}#{}", button.auth_mark);
        changes.push(build_button_change(None, &key, button));
    }
    Ok(())
}

fn build_menu_change(
    existing: Option<&sys_menu::Model>,
    key: &str,
    spec: &MenuNodeSpec,
) -> SyncChange {
    let fields = match existing {
        Some(existing) => menu_changed_fields(existing, spec),
        None => vec![
            "name",
            "title",
            "component",
            "redirect",
            "icon",
            "link",
            "is_iframe",
            "is_hide",
            "is_hide_tab",
            "is_full_page",
            "is_first_level",
            "keep_alive",
            "fixed_tab",
            "show_badge",
            "show_text_badge",
            "active_path",
            "sort",
            "enabled",
        ]
        .into_iter()
        .map(str::to_string)
        .collect(),
    };

    SyncChange {
        target: "menu".to_string(),
        key: key.to_string(),
        action: if existing.is_none() {
            SyncAction::Create
        } else if fields.is_empty() {
            SyncAction::Noop
        } else {
            SyncAction::Update
        },
        fields,
    }
}

fn build_button_change(
    existing: Option<&sys_menu::Model>,
    key: &str,
    spec: &MenuButtonSpec,
) -> SyncChange {
    let fields = match existing {
        Some(existing) => button_changed_fields(existing, spec),
        None => vec!["auth_name", "sort", "enabled"]
            .into_iter()
            .map(str::to_string)
            .collect(),
    };

    SyncChange {
        target: "button".to_string(),
        key: key.to_string(),
        action: if existing.is_none() {
            SyncAction::Create
        } else if fields.is_empty() {
            SyncAction::Noop
        } else {
            SyncAction::Update
        },
        fields,
    }
}

fn menu_changed_fields(existing: &sys_menu::Model, spec: &MenuNodeSpec) -> Vec<String> {
    let mut fields = Vec::new();
    if existing.name != spec.name {
        fields.push("name".to_string());
    }
    if existing.title != spec.title {
        fields.push("title".to_string());
    }
    if existing.component != spec.desired_component() {
        fields.push("component".to_string());
    }
    if existing.redirect != spec.desired_redirect() {
        fields.push("redirect".to_string());
    }
    if existing.icon != spec.desired_icon() {
        fields.push("icon".to_string());
    }
    if existing.link != spec.desired_link() {
        fields.push("link".to_string());
    }
    if existing.is_iframe != spec.desired_is_iframe() {
        fields.push("is_iframe".to_string());
    }
    if existing.is_hide != spec.desired_is_hide() {
        fields.push("is_hide".to_string());
    }
    if existing.is_hide_tab != spec.desired_is_hide_tab() {
        fields.push("is_hide_tab".to_string());
    }
    if existing.is_full_page != spec.desired_is_full_page() {
        fields.push("is_full_page".to_string());
    }
    if existing.is_first_level != spec.desired_is_first_level() {
        fields.push("is_first_level".to_string());
    }
    if existing.keep_alive != spec.desired_keep_alive() {
        fields.push("keep_alive".to_string());
    }
    if existing.fixed_tab != spec.desired_fixed_tab() {
        fields.push("fixed_tab".to_string());
    }
    if existing.show_badge != spec.desired_show_badge() {
        fields.push("show_badge".to_string());
    }
    if existing.show_text_badge != spec.desired_show_text_badge() {
        fields.push("show_text_badge".to_string());
    }
    if existing.active_path != spec.desired_active_path() {
        fields.push("active_path".to_string());
    }
    if existing.sort != spec.desired_sort() {
        fields.push("sort".to_string());
    }
    if existing.enabled != spec.desired_enabled() {
        fields.push("enabled".to_string());
    }
    fields
}

fn button_changed_fields(existing: &sys_menu::Model, spec: &MenuButtonSpec) -> Vec<String> {
    let mut fields = Vec::new();
    if existing.auth_name != spec.auth_name {
        fields.push("auth_name".to_string());
    }
    if existing.sort != spec.desired_sort() {
        fields.push("sort".to_string());
    }
    if existing.enabled != spec.desired_enabled() {
        fields.push("enabled".to_string());
    }
    fields
}

fn find_existing_menu<'a>(
    existing: &'a [sys_menu::Model],
    parent_id: i64,
    path: &str,
) -> Option<&'a sys_menu::Model> {
    existing.iter().find(|menu| {
        menu.parent_id == parent_id
            && menu.menu_type == sys_menu::MenuType::Menu
            && menu.path == path
    })
}

fn find_existing_button<'a>(
    existing: &'a [sys_menu::Model],
    parent_id: i64,
    auth_mark: &str,
) -> Option<&'a sys_menu::Model> {
    existing.iter().find(|menu| {
        menu.parent_id == parent_id
            && menu.menu_type == sys_menu::MenuType::Button
            && menu.auth_mark == auth_mark
    })
}

fn ensure_menu_name_available(
    existing: &[sys_menu::Model],
    current_id: Option<i64>,
    name: &str,
) -> ApiResult<()> {
    if existing.iter().any(|menu| {
        menu.menu_type == sys_menu::MenuType::Menu
            && menu.name == name
            && Some(menu.id) != current_id
    }) {
        return Err(ApiErrors::BadRequest(format!("菜单名称 '{}' 已存在", name)));
    }
    Ok(())
}

fn ensure_button_auth_mark_available(
    existing: &[sys_menu::Model],
    current_id: Option<i64>,
    auth_mark: &str,
) -> ApiResult<()> {
    if existing.iter().any(|menu| {
        menu.menu_type == sys_menu::MenuType::Button
            && menu.auth_mark == auth_mark
            && Some(menu.id) != current_id
    }) {
        return Err(ApiErrors::BadRequest(format!(
            "权限标识 '{}' 已存在",
            auth_mark
        )));
    }
    Ok(())
}

fn menu_key(parent_key: &str, path: &str, name: &str) -> String {
    let segment = if path.trim().is_empty() { name } else { path };
    if parent_key.is_empty() {
        segment.to_string()
    } else {
        format!("{parent_key}/{segment}")
    }
}

fn next_button_bit_position(existing: &[sys_menu::Model]) -> i32 {
    existing
        .iter()
        .filter_map(|menu| menu.bit_position)
        .max()
        .map(|pos| pos + 1)
        .unwrap_or(0)
}

fn apply_menu_nodes<'a, C: ConnectionTrait + Sync + 'a>(
    conn: &'a C,
    existing: &'a [sys_menu::Model],
    parent_id: i64,
    parent_key: &'a str,
    nodes: &'a [MenuNodeSpec],
    next_bit_position: &'a mut i32,
) -> Pin<Box<dyn Future<Output = ApiResult<bool>> + Send + 'a>> {
    Box::pin(async move {
        let mut permission_changed = false;

        for node in nodes {
            let key = menu_key(parent_key, &node.path, &node.name);
            let existing_menu = find_existing_menu(existing, parent_id, &node.path);
            ensure_menu_name_available(existing, existing_menu.map(|menu| menu.id), &node.name)?;

            let menu_id = if let Some(existing_menu) = existing_menu {
                apply_existing_menu(conn, existing_menu, node).await?;
                existing_menu.id
            } else {
                create_menu_from_spec(conn, parent_id, node).await?
            };

            permission_changed |= apply_button_specs(
                conn,
                existing,
                menu_id,
                &key,
                &node.buttons,
                next_bit_position,
            )
            .await?;
            permission_changed |= apply_menu_nodes(
                conn,
                existing,
                menu_id,
                &key,
                &node.children,
                next_bit_position,
            )
            .await?;
        }

        Ok(permission_changed)
    })
}

async fn apply_existing_menu<C: ConnectionTrait>(
    conn: &C,
    existing: &sys_menu::Model,
    spec: &MenuNodeSpec,
) -> ApiResult<()> {
    let fields = menu_changed_fields(existing, spec);
    if fields.is_empty() {
        return Ok(());
    }

    let mut active: sys_menu::ActiveModel = existing.clone().into();
    active.name = Set(spec.name.clone());
    active.title = Set(spec.title.clone());
    active.component = Set(spec.desired_component());
    active.redirect = Set(spec.desired_redirect());
    active.icon = Set(spec.desired_icon());
    active.link = Set(spec.desired_link());
    active.is_iframe = Set(spec.desired_is_iframe());
    active.is_hide = Set(spec.desired_is_hide());
    active.is_hide_tab = Set(spec.desired_is_hide_tab());
    active.is_full_page = Set(spec.desired_is_full_page());
    active.is_first_level = Set(spec.desired_is_first_level());
    active.keep_alive = Set(spec.desired_keep_alive());
    active.fixed_tab = Set(spec.desired_fixed_tab());
    active.show_badge = Set(spec.desired_show_badge());
    active.show_text_badge = Set(spec.desired_show_text_badge());
    active.active_path = Set(spec.desired_active_path());
    active.sort = Set(spec.desired_sort());
    active.enabled = Set(spec.desired_enabled());
    active
        .update(conn)
        .await
        .context("更新菜单失败")
        .map_err(ApiErrors::Internal)?;
    Ok(())
}

async fn create_menu_from_spec<C: ConnectionTrait>(
    conn: &C,
    parent_id: i64,
    spec: &MenuNodeSpec,
) -> ApiResult<i64> {
    let model = sys_menu::ActiveModel {
        parent_id: Set(parent_id),
        menu_type: Set(sys_menu::MenuType::Menu),
        name: Set(spec.name.clone()),
        path: Set(spec.path.clone()),
        component: Set(spec.desired_component()),
        redirect: Set(spec.desired_redirect()),
        icon: Set(spec.desired_icon()),
        title: Set(spec.title.clone()),
        link: Set(spec.desired_link()),
        is_iframe: Set(spec.desired_is_iframe()),
        is_hide: Set(spec.desired_is_hide()),
        is_hide_tab: Set(spec.desired_is_hide_tab()),
        is_full_page: Set(spec.desired_is_full_page()),
        is_first_level: Set(spec.desired_is_first_level()),
        keep_alive: Set(spec.desired_keep_alive()),
        fixed_tab: Set(spec.desired_fixed_tab()),
        show_badge: Set(spec.desired_show_badge()),
        show_text_badge: Set(spec.desired_show_text_badge()),
        active_path: Set(spec.desired_active_path()),
        auth_name: Set(String::new()),
        auth_mark: Set(String::new()),
        sort: Set(spec.desired_sort()),
        enabled: Set(spec.desired_enabled()),
        ..Default::default()
    }
    .insert(conn)
    .await
    .context("创建菜单失败")
    .map_err(ApiErrors::Internal)?;

    Ok(model.id)
}

async fn apply_button_specs<C: ConnectionTrait>(
    conn: &C,
    existing: &[sys_menu::Model],
    parent_id: i64,
    parent_key: &str,
    buttons: &[MenuButtonSpec],
    next_bit_position: &mut i32,
) -> ApiResult<bool> {
    let mut permission_changed = false;

    for button in buttons {
        let existing_button = find_existing_button(existing, parent_id, &button.auth_mark);
        ensure_button_auth_mark_available(
            existing,
            existing_button.map(|button| button.id),
            &button.auth_mark,
        )?;

        if let Some(existing_button) = existing_button {
            let changed = apply_existing_button(conn, existing_button, button).await?;
            permission_changed |= changed;
        } else {
            create_button_from_spec(conn, parent_id, button, next_bit_position).await?;
            permission_changed = true;
        }

        let _ = parent_key;
    }

    Ok(permission_changed)
}

async fn apply_existing_button<C: ConnectionTrait>(
    conn: &C,
    existing: &sys_menu::Model,
    spec: &MenuButtonSpec,
) -> ApiResult<bool> {
    let fields = button_changed_fields(existing, spec);
    if fields.is_empty() {
        return Ok(false);
    }

    let permission_changed = existing.enabled != spec.desired_enabled();
    let mut active: sys_menu::ActiveModel = existing.clone().into();
    active.auth_name = Set(spec.auth_name.clone());
    active.title = Set(spec.auth_name.clone());
    active.sort = Set(spec.desired_sort());
    active.enabled = Set(spec.desired_enabled());
    active
        .update(conn)
        .await
        .context("更新按钮失败")
        .map_err(ApiErrors::Internal)?;

    Ok(permission_changed)
}

async fn create_button_from_spec<C: ConnectionTrait>(
    conn: &C,
    parent_id: i64,
    spec: &MenuButtonSpec,
    next_bit_position: &mut i32,
) -> ApiResult<()> {
    sys_menu::ActiveModel {
        parent_id: Set(parent_id),
        menu_type: Set(sys_menu::MenuType::Button),
        name: Set(String::new()),
        path: Set(String::new()),
        component: Set(String::new()),
        redirect: Set(String::new()),
        icon: Set(String::new()),
        title: Set(spec.auth_name.clone()),
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
        auth_name: Set(spec.auth_name.clone()),
        auth_mark: Set(spec.auth_mark.clone()),
        bit_position: Set(Some(*next_bit_position)),
        sort: Set(spec.desired_sort()),
        enabled: Set(spec.desired_enabled()),
        ..Default::default()
    }
    .insert(conn)
    .await
    .context("创建按钮失败")
    .map_err(ApiErrors::Internal)?;

    *next_bit_position += 1;
    Ok(())
}

fn build_menu_tree(
    menus: &[sys_menu::Model],
    parent_id: i64,
    role_codes: &[String],
) -> Vec<MenuTreeVo> {
    menus
        .iter()
        .filter(|menu| menu.parent_id == parent_id && menu.menu_type == sys_menu::MenuType::Menu)
        .map(|menu| {
            let auth_list = menus
                .iter()
                .filter(|item| {
                    item.parent_id == menu.id && item.menu_type == sys_menu::MenuType::Button
                })
                .map(|item| AuthItem {
                    id: item.id,
                    parent_id: item.parent_id,
                    title: item.title.clone(),
                    auth_name: item.auth_name.clone(),
                    auth_mark: item.auth_mark.clone(),
                    sort: item.sort,
                    enabled: item.enabled,
                    create_time: item.create_time,
                    update_time: item.update_time,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_menu_config_spec_rejects_duplicate_auth_marks() {
        let spec = MenuConfigSpec {
            menus: vec![MenuNodeSpec {
                name: "system".to_string(),
                path: "system".to_string(),
                component: Some("/system/index".to_string()),
                redirect: None,
                icon: None,
                title: "系统管理".to_string(),
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
                buttons: vec![
                    MenuButtonSpec {
                        auth_name: "查看".to_string(),
                        auth_mark: "system:user:list".to_string(),
                        sort: None,
                        enabled: None,
                    },
                    MenuButtonSpec {
                        auth_name: "新增".to_string(),
                        auth_mark: "system:user:list".to_string(),
                        sort: None,
                        enabled: None,
                    },
                ],
                children: vec![],
            }],
        };

        let error = validate_menu_config_spec(&spec).expect_err("should reject duplicates");
        assert!(matches!(error, ApiErrors::BadRequest(_)));
    }

    #[test]
    fn menu_key_builds_nested_path() {
        assert_eq!(menu_key("", "system", "system"), "system");
        assert_eq!(menu_key("system", "user", "user"), "system/user");
    }
}
