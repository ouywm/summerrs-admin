//! AI 对话历史表实体

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
pub enum ConversationStatus {
    #[sea_orm(num_value = 1)]
    Normal = 1,
    #[sea_orm(num_value = 2)]
    Archived = 2,
    #[sea_orm(num_value = 3)]
    Deleted = 3,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "conversation")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub user_id: i64,
    pub project_id: i64,
    pub session_id: i64,
    pub thread_id: i64,
    pub title: String,
    pub model_name: String,
    #[sea_orm(column_type = "Text")]
    pub system_prompt: String,
    #[sea_orm(column_type = "JsonBinary")]
    pub messages: serde_json::Value,
    pub message_count: i32,
    pub total_tokens: i64,
    pub pinned: bool,
    pub pin_sort: i32,
    pub status: ConversationStatus,
    #[sea_orm(column_type = "JsonBinary")]
    pub metadata: serde_json::Value,
    pub last_message_at: Option<DateTimeWithTimeZone>,
    pub create_time: DateTimeWithTimeZone,
    pub update_time: DateTimeWithTimeZone,
}
