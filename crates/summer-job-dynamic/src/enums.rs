//! 调度系统的状态 / 策略枚举。
//!
//! 统一用 String 后端入库（VARCHAR(16)），网页传值跟数据库一致，便于运维直观查问题。

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
    #[sea_orm(string_value = "ONESHOT")]
    Oneshot,
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
    #[sea_orm(string_value = "RUNNING")]
    Running,
    #[sea_orm(string_value = "SUCCEEDED")]
    Succeeded,
    #[sea_orm(string_value = "FAILED")]
    Failed,
    #[sea_orm(string_value = "TIMEOUT")]
    Timeout,
    /// 上一次执行尚未结束，本次触发被跳过
    #[sea_orm(string_value = "DISCARDED")]
    Discarded,
}
