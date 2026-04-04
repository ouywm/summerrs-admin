use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "organization")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(unique)]
    pub org_code: String,
    pub org_name: String,
    pub display_name: String,
    pub logo_url: String,
    #[sea_orm(column_type = "Text")]
    pub description: String,
    pub owner_user_id: i64,
    pub status: i16,
    #[sea_orm(column_type = "JsonBinary")]
    pub settings: serde_json::Value,
    #[sea_orm(column_type = "JsonBinary")]
    pub metadata: serde_json::Value,
    pub deleted_at: Option<DateTimeWithTimeZone>,
    pub create_by: String,
    pub create_time: DateTimeWithTimeZone,
    pub update_by: String,
    pub update_time: DateTimeWithTimeZone,
}
