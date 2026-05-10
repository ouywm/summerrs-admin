use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

use crate::enums::{RunState, TriggerType};

/// 任务执行记录。对应 `sys.job_run` 表，每次触发一条。
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize, JsonSchema)]
#[sea_orm(schema_name = "sys", table_name = "job_run")]
#[serde(rename_all = "camelCase")]
pub struct Model {
    /// 执行记录ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 所属任务ID（外键指向 sys.job.id）
    pub job_id: i64,
    /// 链路追踪ID（每次触发生成 UUID，出问题时可按它关联日志）
    pub trace_id: String,
    /// 触发来源：CRON / MANUAL / RETRY
    pub trigger_type: TriggerType,
    /// 手动触发的用户ID（其他触发类型为 NULL）
    pub trigger_by: Option<i64>,
    /// 状态：RUNNING / SUCCEEDED / FAILED / TIMEOUT / DISCARDED
    pub state: RunState,
    /// 计划触发时间（cron tick 或手动触发那一刻）
    pub scheduled_at: DateTime,
    /// 实际开始执行时间（抢到执行权后写入）
    pub started_at: Option<DateTime>,
    /// 执行结束时间（终态写入）
    pub finished_at: Option<DateTime>,
    /// 当前重试次数（0 = 首次触发，1 = 第一次重试，以此类推）
    pub retry_count: i32,
    /// 返回值（handler 成功返回的 JSON，失败为 NULL）
    #[sea_orm(column_type = "JsonBinary", nullable)]
    pub result_json: Option<Json>,
    /// 错误信息（失败 / 超时 / panic 的可读摘要）
    pub error_message: Option<String>,
    /// 记录创建时间
    pub create_time: DateTime,
}

#[sea_orm::entity::prelude::async_trait::async_trait]
impl sea_orm::ActiveModelBehavior for self::ActiveModel {
    async fn before_save<C>(mut self, _db: &C, insert: bool) -> Result<Self, sea_orm::DbErr>
    where
        C: sea_orm::ConnectionTrait,
    {
        if insert {
            self.create_time = sea_orm::Set(chrono::Local::now().naive_local());
        }
        Ok(self)
    }
}
