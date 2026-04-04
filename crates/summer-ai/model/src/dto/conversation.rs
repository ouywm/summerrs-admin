use schemars::JsonSchema;
use sea_orm::Set;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::entity::conversation::{self, ConversationStatus};
use crate::entity::message::{self, MessageStatus};
use crate::entity::prompt_template::{self, PromptTemplateStatus};

// ─── Conversation ───

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateConversationDto {
    pub user_id: i64,
    #[serde(default)]
    pub project_id: i64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub model_name: String,
    #[serde(default)]
    pub system_prompt: String,
}

impl CreateConversationDto {
    pub fn into_active_model(self) -> conversation::ActiveModel {
        let now = chrono::Utc::now().fixed_offset();
        conversation::ActiveModel {
            user_id: Set(self.user_id),
            project_id: Set(self.project_id),
            session_id: Set(0),
            thread_id: Set(0),
            title: Set(self.title),
            model_name: Set(self.model_name),
            system_prompt: Set(self.system_prompt),
            messages: Set(serde_json::json!([])),
            message_count: Set(0),
            total_tokens: Set(0),
            pinned: Set(false),
            pin_sort: Set(0),
            status: Set(ConversationStatus::Normal),
            metadata: Set(serde_json::json!({})),
            last_message_at: Set(None),
            create_time: Set(now),
            update_time: Set(now),
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateConversationDto {
    pub title: Option<String>,
    pub model_name: Option<String>,
    pub system_prompt: Option<String>,
    pub pinned: Option<bool>,
    pub pin_sort: Option<i32>,
    pub status: Option<ConversationStatus>,
}

impl UpdateConversationDto {
    pub fn apply_to(self, active: &mut conversation::ActiveModel) {
        if let Some(v) = self.title {
            active.title = Set(v);
        }
        if let Some(v) = self.model_name {
            active.model_name = Set(v);
        }
        if let Some(v) = self.system_prompt {
            active.system_prompt = Set(v);
        }
        if let Some(v) = self.pinned {
            active.pinned = Set(v);
        }
        if let Some(v) = self.pin_sort {
            active.pin_sort = Set(v);
        }
        if let Some(v) = self.status {
            active.status = Set(v);
        }
    }
}

#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct QueryConversationDto {
    pub user_id: Option<i64>,
    pub project_id: Option<i64>,
    pub model_name: Option<String>,
    pub status: Option<ConversationStatus>,
    pub pinned: Option<bool>,
}

impl From<QueryConversationDto> for sea_orm::Condition {
    fn from(dto: QueryConversationDto) -> Self {
        use sea_orm::ColumnTrait;
        let mut cond = sea_orm::Condition::all();
        if let Some(v) = dto.user_id {
            cond = cond.add(conversation::Column::UserId.eq(v));
        }
        if let Some(v) = dto.project_id {
            cond = cond.add(conversation::Column::ProjectId.eq(v));
        }
        if let Some(v) = dto.model_name {
            cond = cond.add(conversation::Column::ModelName.contains(&v));
        }
        if let Some(v) = dto.status {
            cond = cond.add(conversation::Column::Status.eq(v));
        }
        if let Some(v) = dto.pinned {
            cond = cond.add(conversation::Column::Pinned.eq(v));
        }
        cond
    }
}

// ─── Message ───

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateMessageDto {
    pub conversation_id: i64,
    #[serde(default)]
    pub user_id: i64,
    pub role: String,
    pub content_text: String,
    #[serde(default)]
    pub model_name: String,
    #[serde(default)]
    pub content_blocks: serde_json::Value,
    #[serde(default)]
    pub tool_calls: serde_json::Value,
}

impl CreateMessageDto {
    pub fn into_active_model(self) -> message::ActiveModel {
        let now = chrono::Utc::now().fixed_offset();
        message::ActiveModel {
            conversation_id: Set(self.conversation_id),
            organization_id: Set(0),
            project_id: Set(0),
            user_id: Set(self.user_id),
            session_id: Set(0),
            thread_id: Set(0),
            trace_id: Set(0),
            request_id: Set(String::new()),
            parent_message_id: Set(0),
            actor_type: Set("user".into()),
            actor_id: Set(self.user_id),
            role: Set(self.role),
            message_type: Set("chat".into()),
            status: Set(MessageStatus::Normal),
            model_name: Set(self.model_name),
            content_text: Set(self.content_text),
            content_blocks: Set(self.content_blocks),
            tool_calls: Set(self.tool_calls),
            tool_results: Set(serde_json::json!([])),
            file_refs: Set(serde_json::json!([])),
            token_usage: Set(serde_json::json!({})),
            finish_reason: Set(String::new()),
            latency_ms: Set(0),
            metadata: Set(serde_json::json!({})),
            create_time: Set(now),
            update_time: Set(now),
            ..Default::default()
        }
    }
}

#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct QueryMessageDto {
    pub conversation_id: Option<i64>,
    pub thread_id: Option<i64>,
    pub role: Option<String>,
    pub status: Option<MessageStatus>,
}

impl From<QueryMessageDto> for sea_orm::Condition {
    fn from(dto: QueryMessageDto) -> Self {
        use sea_orm::ColumnTrait;
        let mut cond = sea_orm::Condition::all();
        if let Some(v) = dto.conversation_id {
            cond = cond.add(message::Column::ConversationId.eq(v));
        }
        if let Some(v) = dto.thread_id {
            cond = cond.add(message::Column::ThreadId.eq(v));
        }
        if let Some(v) = dto.role {
            cond = cond.add(message::Column::Role.eq(v));
        }
        if let Some(v) = dto.status {
            cond = cond.add(message::Column::Status.eq(v));
        }
        cond
    }
}

// ─── PromptTemplate ───

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreatePromptTemplateDto {
    #[serde(default)]
    pub user_id: i64,
    #[validate(length(min = 1, max = 128))]
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub content: String,
    #[serde(default)]
    pub model_name: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub tags: serde_json::Value,
    #[serde(default)]
    pub is_public: bool,
}

impl CreatePromptTemplateDto {
    pub fn into_active_model(self, operator: &str) -> prompt_template::ActiveModel {
        let now = chrono::Utc::now().fixed_offset();
        prompt_template::ActiveModel {
            user_id: Set(self.user_id),
            name: Set(self.name),
            description: Set(self.description),
            content: Set(self.content),
            model_name: Set(self.model_name),
            category: Set(self.category),
            tags: Set(self.tags),
            is_public: Set(self.is_public),
            use_count: Set(0),
            template_sort: Set(0),
            status: Set(PromptTemplateStatus::Enabled),
            remark: Set(String::new()),
            create_by: Set(operator.to_string()),
            update_by: Set(operator.to_string()),
            create_time: Set(now),
            update_time: Set(now),
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePromptTemplateDto {
    pub name: Option<String>,
    pub description: Option<String>,
    pub content: Option<String>,
    pub model_name: Option<String>,
    pub category: Option<String>,
    pub tags: Option<serde_json::Value>,
    pub is_public: Option<bool>,
    pub status: Option<PromptTemplateStatus>,
}

impl UpdatePromptTemplateDto {
    pub fn apply_to(self, active: &mut prompt_template::ActiveModel, operator: &str) {
        if let Some(v) = self.name {
            active.name = Set(v);
        }
        if let Some(v) = self.description {
            active.description = Set(v);
        }
        if let Some(v) = self.content {
            active.content = Set(v);
        }
        if let Some(v) = self.model_name {
            active.model_name = Set(v);
        }
        if let Some(v) = self.category {
            active.category = Set(v);
        }
        if let Some(v) = self.tags {
            active.tags = Set(v);
        }
        if let Some(v) = self.is_public {
            active.is_public = Set(v);
        }
        if let Some(v) = self.status {
            active.status = Set(v);
        }
        active.update_by = Set(operator.to_string());
    }
}

#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct QueryPromptTemplateDto {
    pub user_id: Option<i64>,
    pub category: Option<String>,
    pub is_public: Option<bool>,
    pub status: Option<PromptTemplateStatus>,
    pub name: Option<String>,
}

impl From<QueryPromptTemplateDto> for sea_orm::Condition {
    fn from(dto: QueryPromptTemplateDto) -> Self {
        use sea_orm::ColumnTrait;
        let mut cond = sea_orm::Condition::all();
        if let Some(v) = dto.user_id {
            cond = cond.add(prompt_template::Column::UserId.eq(v));
        }
        if let Some(v) = dto.category {
            cond = cond.add(prompt_template::Column::Category.eq(v));
        }
        if let Some(v) = dto.is_public {
            cond = cond.add(prompt_template::Column::IsPublic.eq(v));
        }
        if let Some(v) = dto.status {
            cond = cond.add(prompt_template::Column::Status.eq(v));
        }
        if let Some(v) = dto.name {
            cond = cond.add(prompt_template::Column::Name.contains(&v));
        }
        cond
    }
}
