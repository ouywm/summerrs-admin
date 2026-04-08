//! AI Guardrail 日统计表
//! 对应 sql/ai/guardrail_metric_daily.sql

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "guardrail_metric_daily")]
pub struct Model {
    /// 统计ID
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 统计日期
    pub stats_date: Date,
    /// 组织ID
    pub organization_id: i64,
    /// 项目ID
    pub project_id: i64,
    /// 规则ID
    pub rule_id: i64,
    /// 规则编码
    pub rule_code: String,
    /// 评估请求数
    pub requests_evaluated: i64,
    /// 通过次数
    pub passed_count: i64,
    /// 拦截次数
    pub blocked_count: i64,
    /// 脱敏次数
    pub redacted_count: i64,
    /// 警告次数
    pub warned_count: i64,
    /// 标记次数
    pub flagged_count: i64,
    /// 平均执行耗时
    pub avg_latency_ms: i32,
    /// 记录时间
    pub create_time: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
