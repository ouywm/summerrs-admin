use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "error_passthrough_rule")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub rule_code: String,
    pub rule_name: String,
    pub status_code_pattern: String,
    pub error_code_pattern: String,
    pub channel_type_pattern: String,
    pub action: String,
    pub status: i16,
    pub priority: i32,
    #[sea_orm(column_type = "JsonBinary")]
    pub metadata: serde_json::Value,
    pub create_by: String,
    pub create_time: DateTimeWithTimeZone,
    pub update_by: String,
    pub update_time: DateTimeWithTimeZone,
}
