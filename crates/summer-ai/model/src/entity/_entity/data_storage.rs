use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "data_storage")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub project_id: i64,
    pub session_id: i64,
    pub thread_id: i64,
    pub trace_id: i64,
    pub data_key: String,
    pub data_type: String,
    pub storage_backend: String,
    pub storage_path: String,
    #[sea_orm(column_type = "JsonBinary")]
    pub content_json: serde_json::Value,
    #[sea_orm(column_type = "Text")]
    pub content_text: String,
    pub content_hash: String,
    #[sea_orm(column_type = "JsonBinary")]
    pub metadata: serde_json::Value,
    pub status: i16,
    pub expire_time: Option<DateTimeWithTimeZone>,
    pub create_time: DateTimeWithTimeZone,
    pub update_time: DateTimeWithTimeZone,
}
