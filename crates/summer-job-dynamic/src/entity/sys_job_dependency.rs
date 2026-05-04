use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

use crate::enums::DependencyOnState;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize, JsonSchema)]
#[sea_orm(schema_name = "sys", table_name = "job_dependency")]
#[serde(rename_all = "camelCase")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub upstream_id: i64,
    pub downstream_id: i64,
    pub on_state: DependencyOnState,
    pub enabled: bool,
    pub create_time: DateTime,
}

#[sea_orm::entity::prelude::async_trait::async_trait]
impl sea_orm::ActiveModelBehavior for self::ActiveModel {
    async fn before_save<C>(mut self, _db: &C, insert: bool) -> Result<Self, sea_orm::DbErr>
    where
        C: sea_orm::ConnectionTrait,
    {
        if insert {
            self.create_time = sea_orm::Set(chrono::Local::now().naive_local());
        }
        Ok(self)
    }
}
