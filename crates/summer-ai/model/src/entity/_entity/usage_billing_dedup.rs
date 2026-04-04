use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "usage_billing_dedup")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(unique)]
    pub dedup_key: String,
    pub request_id: String,
    pub billing_type: String,
    pub processed_at: DateTimeWithTimeZone,
    pub create_time: DateTimeWithTimeZone,
}
