use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "vector_store")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub owner_type: String,
    pub owner_id: i64,
    pub project_id: i64,
    pub name: String,
    #[sea_orm(column_type = "Text")]
    pub description: String,
    pub embedding_model: String,
    pub embedding_dimensions: i32,
    pub storage_backend: String,
    pub provider_vector_store_id: String,
    pub status: i16,
    pub usage_bytes: i64,
    #[sea_orm(column_type = "JsonBinary")]
    pub file_counts: serde_json::Value,
    #[sea_orm(column_type = "JsonBinary")]
    pub metadata: serde_json::Value,
    #[sea_orm(column_type = "JsonBinary")]
    pub expires_after: serde_json::Value,
    pub expires_at: Option<DateTimeWithTimeZone>,
    pub last_active_at: Option<DateTimeWithTimeZone>,
    pub deleted_at: Option<DateTimeWithTimeZone>,
    pub create_by: String,
    pub create_time: DateTimeWithTimeZone,
    pub update_by: String,
    pub update_time: DateTimeWithTimeZone,
}
