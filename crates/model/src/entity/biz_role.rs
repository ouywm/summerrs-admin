//! B 端角色实体

use sea_orm::Set;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "biz_role")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub role_name: String,
    #[sea_orm(unique)]
    pub role_code: String,
    pub status: i16,
    pub remark: String,
    pub create_by: String,
    pub create_time: DateTime,
    pub update_by: String,
    pub update_time: DateTime,
    /// biz_role → biz_user_role（一对多）
    #[sea_orm(has_many)]
    pub user_roles: HasMany<super::biz_user_role::Entity>,
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
