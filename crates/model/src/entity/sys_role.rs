//! 系统角色实体

use sea_orm::entity::prelude::*;
use sea_orm::Set;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "sys_role")]
pub struct Model {
    /// 主键 ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 角色名称
    pub role_name: String,
    /// 角色编码（唯一，如 R_SUPER, R_ADMIN, R_USER）
    #[sea_orm(unique)]
    pub role_code: String,
    /// 角色描述
    pub description: String,
    /// 是否启用
    pub enabled: bool,
    /// 创建时间
    pub create_time: DateTime,
    /// 更新时间
    pub update_time: DateTime,
    /// sys_role → sys_user_role（一对多）
    #[sea_orm(has_many)]
    pub user_roles: HasMany<super::sys_user_role::Entity>,
    /// sys_role → sys_user（多对多，通过 sys_user_role）
    #[sea_orm(has_many, via = "sys_user_role")]
    pub users: HasMany<super::sys_user::Entity>,
    /// sys_role → sys_role_menu（一对多）
    #[sea_orm(has_many)]
    pub role_menus: HasMany<super::sys_role_menu::Entity>,
    /// sys_role → sys_menu（多对多，通过 sys_role_menu）
    #[sea_orm(has_many, via = "sys_role_menu")]
    pub menus: HasMany<super::sys_menu::Entity>,
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