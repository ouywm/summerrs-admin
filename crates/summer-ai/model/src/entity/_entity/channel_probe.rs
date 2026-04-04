use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "channel_probe")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub channel_id: i64,
    pub account_id: i64,
    pub probe_type: String,
    pub status: i16,
    pub response_time_ms: i32,
    pub status_code: i32,
    #[sea_orm(column_type = "Text")]
    pub error_message: String,
    #[sea_orm(column_type = "JsonBinary")]
    pub metadata: serde_json::Value,
    pub create_time: DateTimeWithTimeZone,
}
