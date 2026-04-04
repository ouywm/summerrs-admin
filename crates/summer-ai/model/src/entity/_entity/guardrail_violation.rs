//! AI Guardrail 命中记录表实体

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "guardrail_violation")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub organization_id: i64,
    pub project_id: i64,
    pub user_id: i64,
    pub token_id: i64,
    pub service_account_id: i64,
    pub rule_id: i64,
    pub request_id: String,
    pub execution_id: i64,
    pub log_id: i64,
    pub task_id: i64,
    pub phase: String,
    pub category: String,
    pub action_taken: String,
    pub model_name: String,
    pub endpoint: String,
    pub matched_pattern: String,
    pub matched_content_hash: String,
    #[sea_orm(column_type = "Text")]
    pub sample_excerpt: String,
    pub severity: i16,
    pub latency_ms: i32,
    #[sea_orm(column_type = "JsonBinary")]
    pub metadata: serde_json::Value,
    pub create_time: DateTimeWithTimeZone,
}
