use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "team")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub organization_id: i64,
    pub team_code: String,
    pub team_name: String,
    #[sea_orm(column_type = "Text")]
    pub description: String,
    pub status: i16,
    #[sea_orm(column_type = "JsonBinary")]
    pub metadata: serde_json::Value,
    pub deleted_at: Option<DateTimeWithTimeZone>,
    pub create_by: String,
    pub create_time: DateTimeWithTimeZone,
    pub update_by: String,
    pub update_time: DateTimeWithTimeZone,
}
