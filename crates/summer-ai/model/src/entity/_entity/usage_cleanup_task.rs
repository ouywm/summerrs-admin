use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "usage_cleanup_task")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(unique)]
    pub task_no: String,
    pub status: i16,
    pub target_table: String,
    #[sea_orm(column_type = "JsonBinary")]
    pub filters: serde_json::Value,
    pub deleted_rows: i64,
    #[sea_orm(column_type = "Text")]
    pub error_message: String,
    pub started_by: i64,
    pub canceled_by: i64,
    pub canceled_at: Option<DateTimeWithTimeZone>,
    pub started_at: Option<DateTimeWithTimeZone>,
    pub finished_at: Option<DateTimeWithTimeZone>,
    pub create_time: DateTimeWithTimeZone,
    pub update_time: DateTimeWithTimeZone,
}
