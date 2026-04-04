use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "managed_object")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub project_id: i64,
    pub file_id: i64,
    pub vector_store_id: i64,
    pub trace_id: i64,
    pub request_id: String,
    pub object_type: String,
    pub provider_code: String,
    #[sea_orm(unique)]
    pub unified_object_key: String,
    pub provider_object_id: String,
    pub status: i16,
    #[sea_orm(column_type = "JsonBinary")]
    pub payload: serde_json::Value,
    #[sea_orm(column_type = "JsonBinary")]
    pub result: serde_json::Value,
    #[sea_orm(column_type = "JsonBinary")]
    pub metadata: serde_json::Value,
    pub submit_time: DateTimeWithTimeZone,
    pub finish_time: Option<DateTimeWithTimeZone>,
    pub create_by: String,
    pub create_time: DateTimeWithTimeZone,
    pub update_by: String,
    pub update_time: DateTimeWithTimeZone,
}
