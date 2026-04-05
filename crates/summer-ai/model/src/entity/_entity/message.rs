//! AI 消息表实体

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
pub enum MessageStatus {
    #[sea_orm(num_value = 1)]
    Normal = 1,
    #[sea_orm(num_value = 2)]
    Editing = 2,
    #[sea_orm(num_value = 3)]
    Deleted = 3,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "message")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub conversation_id: i64,
    pub organization_id: i64,
    pub project_id: i64,
    pub user_id: i64,
    pub session_id: i64,
    pub thread_id: i64,
    pub trace_id: i64,
    pub request_id: String,
    pub parent_message_id: i64,
    pub actor_type: String,
    pub actor_id: i64,
    pub role: String,
    pub message_type: String,
    pub status: MessageStatus,
    pub model_name: String,
    #[sea_orm(column_type = "Text")]
    pub content_text: String,
    #[sea_orm(column_type = "JsonBinary")]
    pub content_blocks: serde_json::Value,
    #[sea_orm(column_type = "JsonBinary")]
    pub tool_calls: serde_json::Value,
    #[sea_orm(column_type = "JsonBinary")]
    pub tool_results: serde_json::Value,
    #[sea_orm(column_type = "JsonBinary")]
    pub file_refs: serde_json::Value,
    #[sea_orm(column_type = "JsonBinary")]
    pub token_usage: serde_json::Value,
    pub finish_reason: String,
    pub latency_ms: i32,
    #[sea_orm(column_type = "JsonBinary")]
    pub metadata: serde_json::Value,
    pub create_time: DateTimeWithTimeZone,
    pub update_time: DateTimeWithTimeZone,
    /// 关联对话（多对一，逻辑关联 ai.conversation.id，不建立数据库外键）
    #[sea_orm(belongs_to, from = "conversation_id", to = "id", skip_fk)]
    pub conversation: Option<super::conversation::Entity>,
}
