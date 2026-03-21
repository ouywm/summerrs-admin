//! 系统菜单实体（包含菜单和按钮权限）

use schemars::JsonSchema;
use sea_orm::Set;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 菜单类型（1: 菜单, 2: 按钮权限）
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    EnumIter,
    DeriveActiveEnum,
    Serialize_repr,
    Deserialize_repr,
    JsonSchema,
)]
#[sea_orm(rs_type = "i16", db_type = "SmallInteger")]
#[repr(i16)]
pub enum MenuType {
    /// 菜单
    #[sea_orm(num_value = 1)]
    Menu = 1,
    /// 按钮权限
    #[sea_orm(num_value = 2)]
    Button = 2,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "sys", table_name = "menu")]
pub struct Model {
    /// 主键 ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 父级菜单 ID（0 表示顶级）
    pub parent_id: i64,
    /// 菜单类型
    pub menu_type: MenuType,
    /// 路由名称
    pub name: String,
    /// 路由路径
    pub path: String,
    /// 组件路径
    pub component: String,
    /// 重定向地址
    pub redirect: String,
    /// 菜单图标
    pub icon: String,
    /// 菜单标题
    pub title: String,
    /// 外链地址
    pub link: String,
    /// 是否内嵌 iframe
    pub is_iframe: bool,
    /// 是否隐藏菜单
    pub is_hide: bool,
    /// 是否隐藏标签页
    pub is_hide_tab: bool,
    /// 是否全屏显示
    pub is_full_page: bool,
    /// 是否一级路由
    pub is_first_level: bool,
    /// 是否缓存组件
    pub keep_alive: bool,
    /// 是否固定标签页
    pub fixed_tab: bool,
    /// 是否显示徽标
    pub show_badge: bool,
    /// 文字徽标内容
    pub show_text_badge: String,
    /// 高亮的菜单路径
    pub active_path: String,
    /// 权限名称
    pub auth_name: String,
    /// 权限标识
    pub auth_mark: String,
    /// 权限位图位置（按钮权限专用，从 0 开始自增）
    #[sea_orm(column_type = "Integer", nullable)]
    pub bit_position: Option<i32>,
    /// 排序值（越小越靠前）
    pub sort: i32,
    /// 是否启用
    pub enabled: bool,
    /// 创建时间
    pub create_time: DateTime,
    /// 更新时间
    pub update_time: DateTime,
    /// sys_menu → sys_role_menu（一对多）
    #[sea_orm(has_many)]
    pub role_menus: HasMany<super::sys_role_menu::Entity>,
    /// sys_menu → sys_role（多对多，通过 sys_role_menu）
    #[sea_orm(has_many, via = "sys_role_menu")]
    pub roles: HasMany<super::sys_role::Entity>,
}

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    /// 保存前自动设置时间戳
    async fn before_save<C>(mut self, _db: &C, insert: bool) -> Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        let now = chrono::Local::now().naive_local();
        self.update_time = Set(now);
        if insert {
            self.create_time = Set(now);
        }
        Ok(self)
    }
}
