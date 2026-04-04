use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "file")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub owner_type: String,
    pub owner_id: i64,
    pub project_id: i64,
    pub session_id: i64,
    pub trace_id: i64,
    pub request_id: String,
    pub filename: String,
    pub purpose: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub content_hash: String,
    pub storage_backend: String,
    pub storage_path: String,
    pub provider_file_id: String,
    pub status: i16,
    #[sea_orm(column_type = "Text")]
    pub status_detail: String,
    #[sea_orm(column_type = "JsonBinary")]
    pub metadata: serde_json::Value,
    pub expires_at: Option<DateTimeWithTimeZone>,
    pub create_by: String,
    pub create_time: DateTimeWithTimeZone,
    pub update_by: String,
    pub update_time: DateTimeWithTimeZone,
}
