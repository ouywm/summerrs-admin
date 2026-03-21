//! C 端用户实体

use sea_orm::Set;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// C 端用户状态（1: 启用, 2: 禁用, 3: 注销）
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize_repr, Deserialize_repr,
)]
#[sea_orm(rs_type = "i16", db_type = "SmallInteger")]
#[repr(i16)]
pub enum CustomerStatus {
    #[sea_orm(num_value = 1)]
    Enabled = 1,
    #[sea_orm(num_value = 2)]
    Disabled = 2,
    #[sea_orm(num_value = 3)]
    Cancelled = 3,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "biz", table_name = "customer")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(unique)]
    pub phone: String,
    pub password: String,
    pub nick_name: String,
    pub avatar: String,
    pub status: CustomerStatus,
    pub create_time: DateTime,
    pub update_time: DateTime,
}

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
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
