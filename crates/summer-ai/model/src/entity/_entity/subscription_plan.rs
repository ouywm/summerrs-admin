use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "subscription_plan")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub plan_code: String,
    pub plan_name: String,
    #[sea_orm(column_type = "Text")]
    pub description: String,
    pub status: i16,
    pub billing_cycle: String,
    pub currency: String,
    #[sea_orm(column_type = "Decimal(Some((20, 4)))")]
    pub price: Decimal,
    pub quota: i64,
    #[sea_orm(column_type = "JsonBinary")]
    pub features: serde_json::Value,
    #[sea_orm(column_type = "JsonBinary")]
    pub limits: serde_json::Value,
    pub plan_sort: i32,
    pub create_by: String,
    pub create_time: DateTimeWithTimeZone,
    pub update_by: String,
    pub update_time: DateTimeWithTimeZone,
}
