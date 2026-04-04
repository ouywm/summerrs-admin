//! AI 会话表实体

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    EnumIter,
    DeriveActiveEnum,
    Serialize_repr,
    Deserialize_repr,
    JsonSchema,
)]
#[sea_orm(rs_type = "i16", db_type = "SmallInteger")]
#[repr(i16)]
pub enum SessionStatus {
    #[sea_orm(num_value = 1)]
    Active = 1,
    #[sea_orm(num_value = 2)]
    Expired = 2,
    #[sea_orm(num_value = 3)]
    Closed = 3,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "session")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(unique)]
    pub session_key: String,
    pub organization_id: i64,
    pub project_id: i64,
    pub user_id: i64,
    pub token_id: i64,
    pub service_account_id: i64,
    pub client_type: String,
    pub client_ip: String,
    pub user_agent: String,
    pub status: SessionStatus,
    #[sea_orm(column_type = "JsonBinary")]
    pub metadata: serde_json::Value,
    pub last_active_at: Option<DateTimeWithTimeZone>,
    pub expire_time: Option<DateTimeWithTimeZone>,
    pub create_time: DateTimeWithTimeZone,
    pub update_time: DateTimeWithTimeZone,
}
