use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "routing_target")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub routing_rule_id: i64,
    pub target_type: String,
    pub target_id: i64,
    pub weight: i32,
    pub priority: i32,
    pub status: i16,
    #[sea_orm(column_type = "JsonBinary")]
    pub config: serde_json::Value,
    pub create_time: DateTimeWithTimeZone,
    pub update_time: DateTimeWithTimeZone,
}
