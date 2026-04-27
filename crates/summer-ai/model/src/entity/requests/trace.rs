use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 状态：1=运行中 2=成功 3=失败 4=取消
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
pub enum TraceStatus {
    /// 运行中
    #[sea_orm(num_value = 1)]
    Running = 1,
    /// 成功
    #[sea_orm(num_value = 2)]
    Succeeded = 2,
    /// 失败
    #[sea_orm(num_value = 3)]
    Failed = 3,
    /// 取消
    #[sea_orm(num_value = 4)]
    Cancelled = 4,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "trace")]
pub struct Model {
    /// 追踪ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 项目ID
    pub project_id: i64,
    /// 会话ID
    pub session_id: i64,
    /// 线程ID
    pub thread_id: i64,
    /// 用户ID
    pub user_id: i64,
    /// 追踪键
    pub trace_key: String,
    /// 根请求ID
    pub root_request_id: String,
    /// 来源类型：request/task/workflow/agent
    pub source_type: String,
    /// 状态：1=运行中 2=成功 3=失败 4=取消
    pub status: TraceStatus,
    /// 扩展元数据（JSON）
    #[sea_orm(column_type = "JsonBinary")]
    pub metadata: serde_json::Value,
    /// 开始时间
    pub started_at: DateTimeWithTimeZone,
    /// 结束时间
    pub finished_at: Option<DateTimeWithTimeZone>,
    /// 创建时间
    pub create_time: DateTimeWithTimeZone,
    /// 更新时间
    pub update_time: DateTimeWithTimeZone,

    /// 关联 Span 列表（一对多）
    #[sea_orm(has_many)]
    /// spans
    pub spans: HasMany<super::trace_span::Entity>,
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
