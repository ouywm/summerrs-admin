//! 用户角色关联实体（sys_user 与 sys_role 的多对多中间表）

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "sys_user_role")]
pub struct Model {
    /// 主键 ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 用户 ID（联合唯一约束）
    #[sea_orm(unique_key = "uk_sys_user_role")]
    pub user_id: i64,
    /// 角色 ID（联合唯一约束）
    #[sea_orm(unique_key = "uk_sys_user_role")]
    pub role_id: i64,
    /// 关联用户（多对一）
    #[sea_orm(belongs_to, from = "user_id", to = "id")]
    pub user: Option<super::sys_user::Entity>,
    /// 关联角色（多对一）
    #[sea_orm(belongs_to, from = "role_id", to = "id")]
    pub role: Option<super::sys_role::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
