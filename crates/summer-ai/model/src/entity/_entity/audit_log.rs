use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "audit_log")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub organization_id: i64,
    pub project_id: i64,
    pub user_id: i64,
    pub action: String,
    pub resource_type: String,
    pub resource_id: String,
    #[sea_orm(column_type = "JsonBinary")]
    pub change_set: serde_json::Value,
    pub client_ip: String,
    pub user_agent: String,
    #[sea_orm(column_type = "JsonBinary")]
    pub metadata: serde_json::Value,
    pub create_time: DateTimeWithTimeZone,
}
