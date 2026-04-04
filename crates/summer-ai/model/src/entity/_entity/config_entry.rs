use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "config_entry")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub config_key: String,
    pub config_value: String,
    pub value_type: String,
    pub category: String,
    #[sea_orm(column_type = "Text")]
    pub description: String,
    pub status: i16,
    pub create_by: String,
    pub create_time: DateTimeWithTimeZone,
    pub update_by: String,
    pub update_time: DateTimeWithTimeZone,
}
