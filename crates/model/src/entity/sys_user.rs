//! 系统用户实体

use schemars::JsonSchema;
use sea_orm::Set;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 性别（0: 未知, 1: 男, 2: 女）
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
pub enum Gender {
    /// 未知
    #[sea_orm(num_value = 0)]
    Unknown = 0,
    /// 男
    #[sea_orm(num_value = 1)]
    Male = 1,
    /// 女
    #[sea_orm(num_value = 2)]
    Female = 2,
}

/// 用户账号状态（1: 启用, 2: 禁用）
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
pub enum UserStatus {
    /// 启用 - 账号正常可用
    #[sea_orm(num_value = 1)]
    Enabled = 1,
    /// 禁用 - 管理员封禁，禁止登录
    #[sea_orm(num_value = 2)]
    Disabled = 2,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "sys_user")]
pub struct Model {
    /// 主键 ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 用户名（唯一）
    #[sea_orm(unique)]
    pub user_name: String,
    /// 密码（Argon2 哈希）
    pub password: String,
    /// 昵称
    pub nick_name: String,
    /// 性别
    pub gender: Gender,
    /// 手机号
    pub phone: String,
    /// 邮箱
    pub email: String,
    /// 头像地址
    pub avatar: String,
    /// 用户状态
    pub status: UserStatus,
    /// 创建人
    pub create_by: String,
    /// 创建时间
    pub create_time: DateTime,
    /// 更新人
    pub update_by: String,
    /// 更新时间
    pub update_time: DateTime,
    /// sys_user → sys_user_role（一对多）
    #[sea_orm(has_many)]
    pub user_roles: HasMany<super::sys_user_role::Entity>,
    /// sys_user → sys_role（多对多，通过 sys_user_role）
    #[sea_orm(has_many, via = "sys_user_role")]
    pub roles: HasMany<super::sys_role::Entity>,
    /// sys_user → sys_notice_user（一对多）
    #[sea_orm(has_many)]
    pub notice_users: HasMany<super::sys_notice_user::Entity>,
    /// sys_user → sys_notice（多对多，通过 sys_notice_user）
    #[sea_orm(has_many, via = "sys_notice_user")]
    pub notices: HasMany<super::sys_notice::Entity>,
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
