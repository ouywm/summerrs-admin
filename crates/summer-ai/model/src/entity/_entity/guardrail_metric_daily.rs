//! AI Guardrail 日统计表实体

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "guardrail_metric_daily")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub stats_date: Date,
    pub organization_id: i64,
    pub project_id: i64,
    pub rule_id: i64,
    pub rule_code: String,
    pub requests_evaluated: i64,
    pub passed_count: i64,
    pub blocked_count: i64,
    pub redacted_count: i64,
    pub warned_count: i64,
    pub flagged_count: i64,
    pub avg_latency_ms: i32,
    pub create_time: DateTimeWithTimeZone,
}
