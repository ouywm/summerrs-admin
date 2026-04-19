//! AI 对话历史表（用户与 AI 的聊天记录）
//! 对应 sql/ai/conversation.sql

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 状态：1=正常 2=归档 3=删除
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
    /// 正常
    #[sea_orm(num_value = 1)]
    Normal = 1,
    /// 归档
    #[sea_orm(num_value = 2)]
    Archived = 2,
    /// 删除
    #[sea_orm(num_value = 3)]
    Deleted = 3,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "conversation")]
pub struct Model {
    /// 对话ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 用户ID
    pub user_id: i64,
    /// 项目ID
    pub project_id: i64,
    /// 会话ID
    pub session_id: i64,
    /// 线程ID
    pub thread_id: i64,
    /// 对话标题
    pub title: String,
    /// 使用的模型名称
    pub model_name: String,
    /// 系统提示词
    #[sea_orm(column_type = "Text")]
    pub system_prompt: String,
    /// 消息列表快照缓存（JSON 数组，规范化明细见 ai.message）
    #[sea_orm(column_type = "JsonBinary")]
    pub messages: serde_json::Value,
    /// 消息条数
    pub message_count: i32,
    /// 累计消耗 Token 数
    pub total_tokens: i64,
    /// 是否置顶
    pub pinned: bool,
    /// 置顶排序（越小越靠前）
    pub pin_sort: i32,
    /// 状态：1=正常 2=归档 3=删除
    pub status: ConversationStatus,
    /// 扩展元数据（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub metadata: serde_json::Value,
    /// 最后一条消息时间
    pub last_message_at: Option<DateTimeWithTimeZone>,
    /// 创建时间
    pub create_time: DateTimeWithTimeZone,
    /// 最后更新时间
    pub update_time: DateTimeWithTimeZone,

    /// 关联消息实体列表（一对多）
    #[sea_orm(has_many)]
    /// message entities
    pub message_entities: HasMany<super::message::Entity>,
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
