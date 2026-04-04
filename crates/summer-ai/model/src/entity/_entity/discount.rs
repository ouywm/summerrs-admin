use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "discount")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub discount_code: String,
    pub discount_type: String,
    pub discount_value: String,
    pub status: i16,
    pub max_uses: i64,
    pub used_count: i64,
    pub start_time: DateTimeWithTimeZone,
    pub end_time: DateTimeWithTimeZone,
    #[sea_orm(column_type = "JsonBinary")]
    pub conditions: serde_json::Value,
    pub create_by: String,
    pub create_time: DateTimeWithTimeZone,
    pub update_by: String,
    pub update_time: DateTimeWithTimeZone,
}
