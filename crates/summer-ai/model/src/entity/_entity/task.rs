use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "task")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(unique)]
    pub task_no: String,
    pub task_type: String,
    pub status: i16,
    pub priority: i32,
    #[sea_orm(column_type = "JsonBinary")]
    pub payload: serde_json::Value,
    #[sea_orm(column_type = "JsonBinary")]
    pub result: serde_json::Value,
    #[sea_orm(column_type = "Text")]
    pub error_message: String,
    pub max_retries: i32,
    pub retry_count: i32,
    pub scheduled_at: DateTimeWithTimeZone,
    pub started_at: Option<DateTimeWithTimeZone>,
    pub finished_at: Option<DateTimeWithTimeZone>,
    pub create_time: DateTimeWithTimeZone,
    pub update_time: DateTimeWithTimeZone,
}
