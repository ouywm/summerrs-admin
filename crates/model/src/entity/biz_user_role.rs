//! B 端用户角色关联实体

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "biz", table_name = "user_role")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(unique_key = "uk_biz_user_role")]
    pub user_id: i64,
    #[sea_orm(unique_key = "uk_biz_user_role")]
    pub role_id: i64,
    /// 关联 B 端用户（多对一）
    #[sea_orm(belongs_to, from = "user_id", to = "id")]
    pub user: Option<super::biz_user::Entity>,
    /// 关联 B 端角色（多对一）
    #[sea_orm(belongs_to, from = "role_id", to = "id")]
    pub role: Option<super::biz_role::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
