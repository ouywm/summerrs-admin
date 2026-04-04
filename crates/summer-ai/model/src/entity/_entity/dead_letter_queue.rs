use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "dead_letter_queue")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub source_type: String,
    pub source_id: String,
    pub event_type: String,
    #[sea_orm(column_type = "JsonBinary")]
    pub payload: serde_json::Value,
    #[sea_orm(column_type = "Text")]
    pub error_message: String,
    pub retry_count: i32,
    #[sea_orm(column_type = "JsonBinary")]
    pub metadata: serde_json::Value,
    pub create_time: DateTimeWithTimeZone,
}
