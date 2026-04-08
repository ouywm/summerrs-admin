//! AI 提示词模板表（可复用的系统提示词/角色预设）
//! 对应 sql/ai/prompt_template.sql

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 状态：1=启用 2=禁用
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
pub enum PromptTemplateStatus {
    /// 启用
    #[sea_orm(num_value = 1)]
    Enabled = 1,
    /// 禁用
    #[sea_orm(num_value = 2)]
    Disabled = 2,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "prompt_template")]
pub struct Model {
    /// 模板ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 创建者用户ID（0=系统模板）
    pub user_id: i64,
    /// 模板名称
    pub name: String,
    /// 模板简介
    pub description: String,
    /// 提示词内容
    #[sea_orm(column_type = "Text")]
    pub content: String,
    /// 推荐模型
    pub model_name: String,
    /// 分类标签
    pub category: String,
    /// 标签数组（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub tags: serde_json::Value,
    /// 是否公开
    pub is_public: bool,
    /// 使用次数
    pub use_count: i64,
    /// 排序
    pub template_sort: i32,
    /// 状态：1=启用 2=禁用
    pub status: PromptTemplateStatus,
    /// 备注
    pub remark: String,
    /// 创建人
    pub create_by: String,
    /// 创建时间
    pub create_time: DateTimeWithTimeZone,
    /// 更新人
    pub update_by: String,
    /// 更新时间
    pub update_time: DateTimeWithTimeZone,
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
