//! 调度系统的状态 / 策略枚举类型，统一用 String 后端入库（VARCHAR(16)），
//! 网页传值跟数据库一致，便于运维直观查问题。

use schemars::JsonSchema;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// 调度类型
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    EnumIter,
    DeriveActiveEnum,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
pub enum ScheduleType {
    #[sea_orm(string_value = "CRON")]
    Cron,
    #[sea_orm(string_value = "FIXED_RATE")]
    FixedRate,
    #[sea_orm(string_value = "FIXED_DELAY")]
    FixedDelay,
    #[sea_orm(string_value = "ONESHOT")]
    Oneshot,
}

/// 阻塞策略：上一次还没跑完，新触发到达时怎么办
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    EnumIter,
    DeriveActiveEnum,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
pub enum BlockingStrategy {
    /// 串行：新触发排队等待
    #[sea_orm(string_value = "SERIAL")]
    Serial,
    /// 丢弃新触发
    #[sea_orm(string_value = "DISCARD")]
    Discard,
    /// 取消旧任务，立即跑新触发
    #[sea_orm(string_value = "OVERRIDE")]
    Override,
}

/// Misfire 策略：调度器停机/主切换错过触发点时怎么办
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    EnumIter,
    DeriveActiveEnum,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
pub enum MisfireStrategy {
    /// 立即补跑一次
    #[sea_orm(string_value = "FIRE_NOW")]
    FireNow,
    /// 忽略错过的，等下一次
    #[sea_orm(string_value = "IGNORE")]
    Ignore,
    /// 补跑全部错过的（谨慎使用）
    #[sea_orm(string_value = "RESCHEDULE")]
    Reschedule,
}

/// 重试退避策略
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    EnumIter,
    DeriveActiveEnum,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
pub enum RetryBackoff {
    /// Sidekiq 风格指数退避
    #[sea_orm(string_value = "EXPONENTIAL")]
    Exponential,
    #[sea_orm(string_value = "LINEAR")]
    Linear,
    #[sea_orm(string_value = "FIXED")]
    Fixed,
}

/// 触发来源
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    EnumIter,
    DeriveActiveEnum,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
pub enum TriggerType {
    #[sea_orm(string_value = "CRON")]
    Cron,
    #[sea_orm(string_value = "MANUAL")]
    Manual,
    #[sea_orm(string_value = "RETRY")]
    Retry,
    #[sea_orm(string_value = "WORKFLOW")]
    Workflow,
    #[sea_orm(string_value = "API")]
    Api,
    /// 调度器停机/主切换错过 cron 触发点后的补跑（misfire=FIRE_NOW 时）
    #[sea_orm(string_value = "MISFIRE")]
    Misfire,
}

/// 执行状态机（Hangfire 风格）
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    EnumIter,
    DeriveActiveEnum,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
pub enum RunState {
    /// 已入队待执行
    #[sea_orm(string_value = "ENQUEUED")]
    Enqueued,
    /// 执行中
    #[sea_orm(string_value = "RUNNING")]
    Running,
    /// 成功完成
    #[sea_orm(string_value = "SUCCEEDED")]
    Succeeded,
    /// 失败（已耗尽重试）
    #[sea_orm(string_value = "FAILED")]
    Failed,
    /// 超时
    #[sea_orm(string_value = "TIMEOUT")]
    Timeout,
    /// 被取消
    #[sea_orm(string_value = "CANCELED")]
    Canceled,
    /// 因阻塞策略被丢弃
    #[sea_orm(string_value = "DISCARDED")]
    Discarded,
}

/// 脚本引擎（P3）
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    EnumIter,
    DeriveActiveEnum,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
pub enum ScriptEngine {
    #[sea_orm(string_value = "rhai")]
    Rhai,
    #[sea_orm(string_value = "lua")]
    Lua,
}

/// 依赖触发条件：上游 job 跑成什么状态才触发下游
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    EnumIter,
    DeriveActiveEnum,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
pub enum DependencyOnState {
    /// 仅 upstream 终态 = SUCCEEDED 时触发（默认）
    #[sea_orm(string_value = "SUCCEEDED")]
    Succeeded,
    /// 仅 upstream 终态 = FAILED 时触发（做补救任务）
    #[sea_orm(string_value = "FAILED")]
    Failed,
    /// upstream 任意终态（含 Timeout / Canceled）都触发
    #[sea_orm(string_value = "ALWAYS")]
    Always,
}
