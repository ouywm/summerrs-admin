use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "transaction")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub user_id: i64,
    pub order_id: i64,
    #[sea_orm(unique)]
    pub transaction_no: String,
    pub transaction_type: String,
    pub status: i16,
    pub currency: String,
    #[sea_orm(column_type = "Decimal(Some((20, 4)))")]
    pub amount: Decimal,
    pub payment_method: String,
    pub provider_transaction_id: String,
    #[sea_orm(column_type = "JsonBinary")]
    pub metadata: serde_json::Value,
    pub create_time: DateTimeWithTimeZone,
    pub update_time: DateTimeWithTimeZone,
}
