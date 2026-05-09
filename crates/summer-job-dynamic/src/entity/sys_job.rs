use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

use crate::enums::ScheduleType;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize, JsonSchema)]
#[sea_orm(schema_name = "sys", table_name = "job")]
#[serde(rename_all = "camelCase")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub tenant_id: Option<i64>,
    pub name: String,
    pub group_name: String,
    pub description: String,
    pub handler: String,
    pub schedule_type: ScheduleType,
    pub cron_expr: Option<String>,
    pub interval_ms: Option<i64>,
    pub fire_time: Option<DateTime>,
    #[sea_orm(column_type = "JsonBinary")]
    pub params_json: Json,
    pub enabled: bool,
    pub timeout_ms: i64,
    pub retry_max: i32,
    pub version: i64,
    pub created_by: Option<i64>,
    pub create_time: DateTime,
    pub update_time: DateTime,
}

#[sea_orm::entity::prelude::async_trait::async_trait]
impl sea_orm::ActiveModelBehavior for self::ActiveModel {
    async fn before_save<C>(mut self, _db: &C, insert: bool) -> Result<Self, sea_orm::DbErr>
    where
        C: sea_orm::ConnectionTrait,
    {
        let now = chrono::Local::now().naive_local();
        self.update_time = sea_orm::Set(now);
        if insert {
            self.create_time = sea_orm::Set(now);
        }
        Ok(self)
    }
}
