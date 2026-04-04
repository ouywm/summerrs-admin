use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "rbac_policy_version")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub rbac_policy_id: i64,
    pub version_no: i32,
    #[sea_orm(column_type = "JsonBinary")]
    pub rules: serde_json::Value,
    pub status: i16,
    pub create_time: DateTimeWithTimeZone,
}
