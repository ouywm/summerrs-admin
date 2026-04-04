use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "vector_store_file")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub vector_store_id: i64,
    pub file_id: i64,
    pub status: i16,
    pub usage_bytes: i64,
    #[sea_orm(column_type = "JsonBinary")]
    pub last_error: serde_json::Value,
    #[sea_orm(column_type = "JsonBinary")]
    pub chunking_strategy: serde_json::Value,
    #[sea_orm(column_type = "JsonBinary")]
    pub attributes: serde_json::Value,
    pub deleted_at: Option<DateTimeWithTimeZone>,
    pub create_time: DateTimeWithTimeZone,
    pub update_time: DateTimeWithTimeZone,
}
