//! AI 异步任务表（图像/音频/视频/批处理等长任务）
//! 对应 sql/ai/task.sql

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 任务类型：1=图像生成 2=图像编辑 3=批量推理 4=音频 5=视频
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
pub enum TaskType {
    /// 图像生成
    #[sea_orm(num_value = 1)]
    ImageGeneration = 1,
    /// 图像编辑
    #[sea_orm(num_value = 2)]
    ImageEditing = 2,
    /// 批量推理
    #[sea_orm(num_value = 3)]
    BatchInference = 3,
    /// 音频
    #[sea_orm(num_value = 4)]
    Audio = 4,
    /// 视频
    #[sea_orm(num_value = 5)]
    Video = 5,
}

/// 状态：1=排队中 2=处理中 3=已完成 4=失败 5=已取消
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
pub enum TaskStatus {
    /// 排队中
    #[sea_orm(num_value = 1)]
    Queued = 1,
    /// 处理中
    #[sea_orm(num_value = 2)]
    Processing = 2,
    /// 已完成
    #[sea_orm(num_value = 3)]
    Completed = 3,
    /// 失败
    #[sea_orm(num_value = 4)]
    Failed = 4,
    /// 已取消
    #[sea_orm(num_value = 5)]
    Cancelled = 5,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "task")]
pub struct Model {
    /// 任务ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 用户ID
    pub user_id: i64,
    /// 令牌ID
    pub token_id: i64,
    /// 所属项目ID（0 表示个人任务）
    pub project_id: i64,
    /// 所属追踪ID
    pub trace_id: i64,
    /// 渠道ID
    pub channel_id: i64,
    /// 账号ID
    pub account_id: i64,
    /// 关联订阅ID
    pub subscription_id: i64,
    /// 来源请求ID
    pub request_id: String,
    /// 任务类型：1=图像生成 2=图像编辑 3=批量推理 4=音频 5=视频
    pub task_type: TaskType,
    /// 平台标识（midjourney/dall-e/suno/sora 等）
    pub platform: String,
    /// 操作类型
    pub action: String,
    /// 使用的模型名
    pub model_name: String,
    /// 请求参数
    #[sea_orm(column_type = "JsonBinary", nullable)]
    pub request_body: Option<serde_json::Value>,
    /// 任务结果/轮询结果
    #[sea_orm(column_type = "JsonBinary", nullable)]
    pub response_body: Option<serde_json::Value>,
    /// 上游任务ID
    pub upstream_task_id: String,
    /// 任务进度（0-100）
    pub progress: i16,
    /// 状态：1=排队中 2=处理中 3=已完成 4=失败 5=已取消
    pub status: TaskStatus,
    /// 失败原因
    #[sea_orm(column_type = "Text")]
    pub fail_reason: String,
    /// 消耗额度
    pub quota: i64,
    /// 计费来源：wallet/subscription/free/admin
    pub billing_source: String,
    /// 提交时间
    pub submit_time: DateTimeWithTimeZone,
    /// 开始时间
    pub start_time: Option<DateTimeWithTimeZone>,
    /// 完成时间
    pub finish_time: Option<DateTimeWithTimeZone>,
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
