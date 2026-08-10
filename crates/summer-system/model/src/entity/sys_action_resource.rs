//! 操作与后端 API 资源关联实体

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "sys", table_name = "action_resource")]
pub struct Model {
    /// 主键 ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 操作 ID：当前复用 sys.menu.id，且应为 menu_type = Button
    #[sea_orm(unique_key = "uk_sys_action_resource")]
    pub action_menu_id: i64,
    /// 后端 API 资源 ID
    #[sea_orm(unique_key = "uk_sys_action_resource")]
    pub resource_id: i64,
    /// 关联操作（多对一）
    #[sea_orm(belongs_to, from = "action_menu_id", to = "id", skip_fk)]
    pub action_menu: BelongsTo<super::sys_menu::Entity>,
    /// 关联资源（多对一）
    #[sea_orm(belongs_to, from = "resource_id", to = "id", skip_fk)]
    pub resource: BelongsTo<super::sys_resource::Entity>,
}

impl sea_orm::ActiveModelBehavior for self::ActiveModel {}
