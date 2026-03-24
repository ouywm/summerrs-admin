use chrono::{DateTime, FixedOffset};
use schemars::JsonSchema;
use sea_orm::Set;
use sea_orm::prelude::BigDecimal;
use serde::{Deserialize, Serialize};

use crate::entity::log::{self, LogStatus, LogType};

/// 创建日志（内部使用，由 service 层构造）
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateLogDto {
    pub user_id: i64,
    pub token_id: i64,
    pub token_name: String,
    pub project_id: i64,
    pub conversation_id: i64,
    pub message_id: i64,
    pub session_id: i64,
    pub thread_id: i64,
    pub trace_id: i64,
    pub channel_id: i64,
    pub channel_name: String,
    pub account_id: i64,
    pub account_name: String,
    pub execution_id: i64,
    pub endpoint: String,
    pub request_format: String,
    pub requested_model: String,
    pub upstream_model: String,
    pub model_name: String,
    pub prompt_tokens: i32,
    pub completion_tokens: i32,
    pub total_tokens: i32,
    pub cached_tokens: i32,
    pub reasoning_tokens: i32,
    pub quota: i64,
    pub cost_total: BigDecimal,
    pub price_reference: String,
    pub elapsed_time: i32,
    pub first_token_time: i32,
    pub is_stream: bool,
    pub request_id: String,
    pub upstream_request_id: String,
    pub status_code: i32,
    pub client_ip: String,
    pub user_agent: String,
    pub content: String,
    pub log_type: LogType,
    pub status: LogStatus,
}

impl From<CreateLogDto> for log::ActiveModel {
    fn from(item: CreateLogDto) -> Self {
        let now = chrono::Utc::now().fixed_offset();
        log::ActiveModel {
            user_id: Set(item.user_id),
            token_id: Set(item.token_id),
            token_name: Set(item.token_name),
            project_id: Set(item.project_id),
            conversation_id: Set(item.conversation_id),
            message_id: Set(item.message_id),
            session_id: Set(item.session_id),
            thread_id: Set(item.thread_id),
            trace_id: Set(item.trace_id),
            channel_id: Set(item.channel_id),
            channel_name: Set(item.channel_name),
            account_id: Set(item.account_id),
            account_name: Set(item.account_name),
            execution_id: Set(item.execution_id),
            endpoint: Set(item.endpoint),
            request_format: Set(item.request_format),
            requested_model: Set(item.requested_model),
            upstream_model: Set(item.upstream_model),
            model_name: Set(item.model_name),
            prompt_tokens: Set(item.prompt_tokens),
            completion_tokens: Set(item.completion_tokens),
            total_tokens: Set(item.total_tokens),
            cached_tokens: Set(item.cached_tokens),
            reasoning_tokens: Set(item.reasoning_tokens),
            quota: Set(item.quota),
            cost_total: Set(item.cost_total),
            price_reference: Set(item.price_reference),
            elapsed_time: Set(item.elapsed_time),
            first_token_time: Set(item.first_token_time),
            is_stream: Set(item.is_stream),
            request_id: Set(item.request_id),
            upstream_request_id: Set(item.upstream_request_id),
            status_code: Set(item.status_code),
            client_ip: Set(item.client_ip),
            user_agent: Set(item.user_agent),
            content: Set(item.content),
            log_type: Set(item.log_type),
            status: Set(item.status),
            create_time: Set(now),
            ..Default::default()
        }
    }
}

/// 查询日志
#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct QueryLogDto {
    pub user_id: Option<i64>,
    pub token_id: Option<i64>,
    pub channel_id: Option<i64>,
    pub model_name: Option<String>,
    pub log_type: Option<LogType>,
    pub status: Option<LogStatus>,
    pub start_time: Option<DateTime<FixedOffset>>,
    pub end_time: Option<DateTime<FixedOffset>>,
    pub client_ip: Option<String>,
    pub request_id: Option<String>,
}

impl From<QueryLogDto> for sea_orm::Condition {
    fn from(dto: QueryLogDto) -> Self {
        use sea_orm::ColumnTrait;
        let mut cond = sea_orm::Condition::all();
        if let Some(v) = dto.user_id {
            cond = cond.add(log::Column::UserId.eq(v));
        }
        if let Some(v) = dto.token_id {
            cond = cond.add(log::Column::TokenId.eq(v));
        }
        if let Some(v) = dto.channel_id {
            cond = cond.add(log::Column::ChannelId.eq(v));
        }
        if let Some(v) = dto.model_name {
            cond = cond.add(log::Column::ModelName.contains(&v));
        }
        if let Some(v) = dto.log_type {
            cond = cond.add(log::Column::LogType.eq(v));
        }
        if let Some(v) = dto.status {
            cond = cond.add(log::Column::Status.eq(v));
        }
        if let Some(v) = dto.start_time {
            cond = cond.add(log::Column::CreateTime.gte(v));
        }
        if let Some(v) = dto.end_time {
            cond = cond.add(log::Column::CreateTime.lte(v));
        }
        if let Some(v) = dto.client_ip {
            cond = cond.add(log::Column::ClientIp.eq(v));
        }
        if let Some(v) = dto.request_id {
            cond = cond.add(log::Column::RequestId.eq(v));
        }
        cond
    }
}

/// 日志统计查询
#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LogStatsQueryDto {
    pub start_time: Option<DateTime<FixedOffset>>,
    pub end_time: Option<DateTime<FixedOffset>>,
    pub group_by: Option<String>,
    pub user_id: Option<i64>,
    pub model_name: Option<String>,
    pub channel_id: Option<i64>,
}
