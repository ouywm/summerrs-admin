//! AI 日度统计表实体
//! 由定时任务从 ai.log 聚合生成

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(schema_name = "ai", table_name = "daily_stats")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub stats_date: Date,
    pub user_id: i64,
    pub project_id: i64,
    pub channel_id: i64,
    pub account_id: i64,
    pub model_name: String,
    pub request_count: i64,
    pub success_count: i64,
    pub fail_count: i64,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
    pub cached_tokens: i64,
    pub reasoning_tokens: i64,
    pub quota: i64,
    #[sea_orm(column_type = "Decimal(Some((20, 10)))")]
    pub cost_total: Decimal,
    pub avg_elapsed_time: i32,
    pub avg_first_token_time: i32,
    pub create_time: DateTimeWithTimeZone,
}
