use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "routing_rule")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub rule_code: String,
    pub rule_name: String,
    pub priority: i32,
    pub status: i16,
    #[sea_orm(column_type = "JsonBinary")]
    pub conditions: serde_json::Value,
    #[sea_orm(column_type = "JsonBinary")]
    pub metadata: serde_json::Value,
    pub create_by: String,
    pub create_time: DateTimeWithTimeZone,
    pub update_by: String,
    pub update_time: DateTimeWithTimeZone,
}
