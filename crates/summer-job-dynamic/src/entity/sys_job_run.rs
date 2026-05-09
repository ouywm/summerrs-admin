use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

use crate::enums::{RunState, TriggerType};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize, JsonSchema)]
#[sea_orm(schema_name = "sys", table_name = "job_run")]
#[serde(rename_all = "camelCase")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub job_id: i64,
    pub trace_id: String,
    pub trigger_type: TriggerType,
    pub trigger_by: Option<i64>,
    pub state: RunState,
    pub instance: Option<String>,
    pub scheduled_at: DateTime,
    pub started_at: Option<DateTime>,
    pub finished_at: Option<DateTime>,
    pub retry_count: i32,
    #[sea_orm(column_type = "JsonBinary", nullable)]
    pub result_json: Option<Json>,
    pub error_message: Option<String>,
    pub log_excerpt: Option<String>,
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
