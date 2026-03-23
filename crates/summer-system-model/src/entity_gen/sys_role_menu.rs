//! 角色菜单关联实体（sys_role 与 sys_menu 的多对多中间表）

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "sys", table_name = "role_menu")]
pub struct Model {
    /// 主键 ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 角色 ID（联合唯一约束）
    #[sea_orm(unique_key = "uk_sys_role_menu")]
    pub role_id: i64,
    /// 菜单 ID（联合唯一约束）
    #[sea_orm(unique_key = "uk_sys_role_menu")]
    pub menu_id: i64,
    /// 关联角色（多对一）
    #[sea_orm(belongs_to, from = "role_id", to = "id", skip_fk)]
    pub role: Option<super::sys_role::Entity>,
    /// 关联菜单（多对一）
    #[sea_orm(belongs_to, from = "menu_id", to = "id", skip_fk)]
    pub menu: Option<super::sys_menu::Entity>,
}
