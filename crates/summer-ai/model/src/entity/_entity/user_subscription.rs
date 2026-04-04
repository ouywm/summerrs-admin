use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "user_subscription")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub user_id: i64,
    pub plan_id: i64,
    pub status: i16,
    pub current_period_start: DateTimeWithTimeZone,
    pub current_period_end: DateTimeWithTimeZone,
    pub cancel_at: Option<DateTimeWithTimeZone>,
    pub canceled_at: Option<DateTimeWithTimeZone>,
    #[sea_orm(column_type = "JsonBinary")]
    pub metadata: serde_json::Value,
    pub create_time: DateTimeWithTimeZone,
    pub update_time: DateTimeWithTimeZone,
}
