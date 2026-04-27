use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 状态：1=正常 2=编辑中 3=删除
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
    /// 正常
    #[sea_orm(num_value = 1)]
    Normal = 1,
    /// 编辑中
    #[sea_orm(num_value = 2)]
    Editing = 2,
    /// 删除
    #[sea_orm(num_value = 3)]
    Deleted = 3,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "message")]
pub struct Model {
    /// 消息ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 对话ID
    pub conversation_id: i64,
    /// 组织ID
    pub organization_id: i64,
    /// 项目ID
    pub project_id: i64,
    /// 用户ID
    pub user_id: i64,
    /// 会话ID
    pub session_id: i64,
    /// 线程ID
    pub thread_id: i64,
    /// 追踪ID
    pub trace_id: i64,
    /// 关联请求ID
    pub request_id: String,
    /// 父消息ID
    pub parent_message_id: i64,
    /// 消息生产者类型：user/assistant/tool/system/service_account
    pub actor_type: String,
    /// 消息生产者ID
    pub actor_id: i64,
    /// 消息角色：system/user/assistant/tool
    pub role: String,
    /// 消息类型：chat/tool_call/tool_result/event
    pub message_type: String,
    /// 状态：1=正常 2=编辑中 3=删除
    pub status: MessageStatus,
    /// 生成该消息的模型名
    pub model_name: String,
    /// 纯文本内容
    #[sea_orm(column_type = "Text")]
    pub content_text: String,
    /// 结构化内容块（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub content_blocks: serde_json::Value,
    /// 工具调用列表（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub tool_calls: serde_json::Value,
    /// 工具调用结果（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub tool_results: serde_json::Value,
    /// 关联文件引用（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub file_refs: serde_json::Value,
    /// 消息级 Token 用量（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub token_usage: serde_json::Value,
    /// 结束原因
    pub finish_reason: String,
    /// 消息生成耗时
    pub latency_ms: i32,
    /// 扩展元数据（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub metadata: serde_json::Value,
    /// 创建时间
    pub create_time: DateTimeWithTimeZone,
    /// 更新时间
    pub update_time: DateTimeWithTimeZone,

    /// 关联对话（多对一，逻辑关联 ai.conversation.id，不建立数据库外键）
    #[sea_orm(belongs_to, from = "conversation_id", to = "id", skip_fk)]
    /// conversation
    pub conversation: Option<super::conversation::Entity>,
}

#[sea_orm::entity::prelude::async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C>(mut self, _db: &C, insert: bool) -> Result<Self, sea_orm::DbErr>
    where
        C: sea_orm::ConnectionTrait,
    {
        let now = chrono::Utc::now().fixed_offset();
        self.update_time = sea_orm::Set(now);
        if insert {
            self.create_time = sea_orm::Set(now);
        }
        Ok(self)
    }
}
