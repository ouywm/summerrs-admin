use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "idempotency_record")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(unique)]
    pub idempotency_key: String,
    pub request_hash: String,
    pub status: i16,
    #[sea_orm(column_type = "JsonBinary")]
    pub response_body: serde_json::Value,
    pub response_status_code: i32,
    pub expires_at: DateTimeWithTimeZone,
    pub create_time: DateTimeWithTimeZone,
}
