use chrono::{DateTime, FixedOffset};
use schemars::JsonSchema;
use serde::Serialize;

use crate::entity::conversation::{self, ConversationStatus};
use crate::entity::message::{self, MessageStatus};
use crate::entity::prompt_template::{self, PromptTemplateStatus};

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConversationVo {
    pub id: i64,
    pub user_id: i64,
    pub project_id: i64,
    pub title: String,
    pub model_name: String,
    pub message_count: i32,
    pub total_tokens: i64,
    pub pinned: bool,
    pub status: ConversationStatus,
    pub last_message_at: Option<DateTime<FixedOffset>>,
    pub create_time: DateTime<FixedOffset>,
    pub update_time: DateTime<FixedOffset>,
}

impl ConversationVo {
    pub fn from_model(m: conversation::Model) -> Self {
        Self {
            id: m.id,
            user_id: m.user_id,
            project_id: m.project_id,
            title: m.title,
            model_name: m.model_name,
            message_count: m.message_count,
            total_tokens: m.total_tokens,
            pinned: m.pinned,
            status: m.status,
            last_message_at: m.last_message_at,
            create_time: m.create_time,
            update_time: m.update_time,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConversationDetailVo {
    #[serde(flatten)]
    pub base: ConversationVo,
    pub system_prompt: String,
    pub metadata: serde_json::Value,
}

impl ConversationDetailVo {
    pub fn from_model(m: conversation::Model) -> Self {
        let system_prompt = m.system_prompt.clone();
        let metadata = m.metadata.clone();
        Self {
            base: ConversationVo::from_model(m),
            system_prompt,
            metadata,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MessageVo {
    pub id: i64,
    pub conversation_id: i64,
    pub user_id: i64,
    pub role: String,
    pub message_type: String,
    pub status: MessageStatus,
    pub model_name: String,
    pub content_text: String,
    pub content_blocks: serde_json::Value,
    pub tool_calls: serde_json::Value,
    pub tool_results: serde_json::Value,
    pub token_usage: serde_json::Value,
    pub finish_reason: String,
    pub latency_ms: i32,
    pub create_time: DateTime<FixedOffset>,
}

impl MessageVo {
    pub fn from_model(m: message::Model) -> Self {
        Self {
            id: m.id,
            conversation_id: m.conversation_id,
            user_id: m.user_id,
            role: m.role,
            message_type: m.message_type,
            status: m.status,
            model_name: m.model_name,
            content_text: m.content_text,
            content_blocks: m.content_blocks,
            tool_calls: m.tool_calls,
            tool_results: m.tool_results,
            token_usage: m.token_usage,
            finish_reason: m.finish_reason,
            latency_ms: m.latency_ms,
            create_time: m.create_time,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PromptTemplateVo {
    pub id: i64,
    pub user_id: i64,
    pub name: String,
    pub description: String,
    pub content: String,
    pub model_name: String,
    pub category: String,
    pub tags: serde_json::Value,
    pub is_public: bool,
    pub use_count: i64,
    pub status: PromptTemplateStatus,
    pub create_time: DateTime<FixedOffset>,
    pub update_time: DateTime<FixedOffset>,
}

impl PromptTemplateVo {
    pub fn from_model(m: prompt_template::Model) -> Self {
        Self {
            id: m.id,
            user_id: m.user_id,
            name: m.name,
            description: m.description,
            content: m.content,
            model_name: m.model_name,
            category: m.category,
            tags: m.tags,
            is_public: m.is_public,
            use_count: m.use_count,
            status: m.status,
            create_time: m.create_time,
            update_time: m.update_time,
        }
    }
}
