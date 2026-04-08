//! AI 项目成员表
//! 对应 sql/ai/project_membership.sql

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 状态：1=正常 2=禁用 3=移除
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
pub enum ProjectMembershipStatus {
    /// 正常
    #[sea_orm(num_value = 1)]
    Normal = 1,
    /// 禁用
    #[sea_orm(num_value = 2)]
    Disabled = 2,
    /// 移除
    #[sea_orm(num_value = 3)]
    Removed = 3,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "project_membership")]
pub struct Model {
    /// 成员关系ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 项目ID
    pub project_id: i64,
    /// 用户ID
    pub user_id: i64,
    /// 项目角色
    pub role_code: String,
    /// 状态：1=正常 2=禁用 3=移除
    pub status: ProjectMembershipStatus,
    /// 来源：manual/sso/scim/invite
    pub source: String,
    /// 创建人
    pub create_by: String,
    /// 创建时间
    pub create_time: DateTimeWithTimeZone,
    /// 更新人
    pub update_by: String,
    /// 更新时间
    pub update_time: DateTimeWithTimeZone,
}

#[sea_orm::entity::prelude::async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C>(mut self, _db: &C, insert: bool) -> Result<Self, sea_orm::DbErr>
    where
        C: sea_orm::ConnectionTrait,
    {
        let now = chrono::Utc::now().fixed_offset();
        self.update_time = sea_orm::Set(now);
        if insert {
            self.create_time = sea_orm::Set(now);
        }
        Ok(self)
    }
}
