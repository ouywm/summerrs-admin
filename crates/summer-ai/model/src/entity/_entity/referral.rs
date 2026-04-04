use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "referral")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub referrer_id: i64,
    pub referee_id: i64,
    pub status: i16,
    pub reward_quota: i64,
    pub reward_settled: bool,
    pub settled_at: Option<DateTimeWithTimeZone>,
    pub create_time: DateTimeWithTimeZone,
    pub update_time: DateTimeWithTimeZone,
}
