use anyhow::Context;
use sea_orm::prelude::BigDecimal;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder};
use summer::plugin::Service;
use summer_common::error::{ApiErrors, ApiResult};
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};

use summer_ai_core::types::common::Usage;
use summer_ai_model::dto::log::{CreateLogDto, LogStatsQueryDto, QueryLogDto};
use summer_ai_model::entity::log::{self, LogStatus, LogType};
use summer_ai_model::vo::log::{DashboardOverviewVo, LogStatsVo, LogVo};

use crate::relay::channel_router::SelectedChannel;
use crate::service::log_batch::AiLogBatchQueue;
use crate::service::token::TokenInfo;

pub struct ChatCompletionLogRecord {
    pub endpoint: String,
    pub requested_model: String,
    pub upstream_model: String,
    pub model_name: String,
    pub quota: i64,
    pub elapsed_time: i32,
    pub first_token_time: i32,
    pub is_stream: bool,
    pub client_ip: String,
}

pub struct EmbeddingLogRecord {
    pub requested_model: String,
    pub upstream_model: String,
    pub model_name: String,
    pub quota: i64,
    pub elapsed_time: i32,
    pub client_ip: String,
}

pub struct EndpointUsageLogRecord {
    pub endpoint: String,
    pub requested_model: String,
    pub upstream_model: String,
    pub model_name: String,
    pub quota: i64,
    pub elapsed_time: i32,
    pub first_token_time: i32,
    pub is_stream: bool,
    pub client_ip: String,
}

#[derive(Clone, Service)]
pub struct LogService {
    #[inject(component)]
    db: DbConn,
    #[inject(component)]
    queue: AiLogBatchQueue,
}

impl LogService {
    /// Queue one AI usage log for batched persistence.
    pub fn record_async(&self, dto: CreateLogDto) {
        self.queue.push(dto.into());
    }

    pub async fn list_logs(
        &self,
        query: QueryLogDto,
        pagination: Pagination,
    ) -> ApiResult<Page<LogVo>> {
        let page = log::Entity::find()
            .filter(query)
            .order_by_desc(log::Column::CreateTime)
            .order_by_desc(log::Column::Id)
            .page(&self.db, &pagination)
            .await
            .context("failed to list AI logs")
            .map_err(ApiErrors::Internal)?;

        Ok(page.map(LogVo::from_model))
    }

    pub async fn stats(&self, query: LogStatsQueryDto) -> ApiResult<Vec<LogStatsVo>> {
        let group_by = normalize_log_group_by(query.group_by.as_deref());
        let logs = log::Entity::find()
            .filter(build_log_stats_condition(&query))
            .order_by_desc(log::Column::CreateTime)
            .all(&self.db)
            .await
            .context("failed to query AI logs for stats")
            .map_err(ApiErrors::Internal)?;

        let mut groups: std::collections::BTreeMap<String, LogStatsAccumulator> =
            std::collections::BTreeMap::new();
        for item in logs {
            let key = match group_by {
                LogStatsGroupBy::User => item.user_id.to_string(),
                LogStatsGroupBy::Channel => {
                    if item.channel_name.is_empty() {
                        item.channel_id.to_string()
                    } else {
                        item.channel_name.clone()
                    }
                }
                LogStatsGroupBy::Model => item.model_name.clone(),
                LogStatsGroupBy::Day => item.create_time.format("%Y-%m-%d").to_string(),
            };

            let entry = groups.entry(key).or_default();
            entry.request_count += 1;
            entry.total_tokens += i64::from(item.total_tokens);
            entry.total_quota += item.quota;
            entry.total_elapsed_time += i64::from(item.elapsed_time);
        }

        let mut stats: Vec<LogStatsVo> = groups
            .into_iter()
            .map(|(group_key, acc)| LogStatsVo {
                group_key,
                request_count: acc.request_count,
                total_tokens: acc.total_tokens,
                total_quota: acc.total_quota,
                avg_elapsed_time: if acc.request_count == 0 {
                    0.0
                } else {
                    acc.total_elapsed_time as f64 / acc.request_count as f64
                },
            })
            .collect();
        stats.sort_by(|left, right| {
            right
                .request_count
                .cmp(&left.request_count)
                .then_with(|| left.group_key.cmp(&right.group_key))
        });
        Ok(stats)
    }

    pub async fn overview(&self) -> ApiResult<DashboardOverviewVo> {
        let now = chrono::Utc::now().fixed_offset();
        let today_start = now
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_local_timezone(*now.offset())
            .unwrap();
        let logs = log::Entity::find()
            .filter(log::Column::CreateTime.gte(today_start))
            .filter(log::Column::CreateTime.lte(now))
            .all(&self.db)
            .await
            .context("failed to query AI logs for overview")
            .map_err(ApiErrors::Internal)?;

        let mut active_users = std::collections::BTreeSet::new();
        let mut active_tokens = std::collections::BTreeSet::new();
        let mut today_request_count = 0_i64;
        let mut today_total_tokens = 0_i64;
        let mut today_total_quota = 0_i64;

        for item in logs {
            today_request_count += 1;
            today_total_tokens += i64::from(item.total_tokens);
            today_total_quota += item.quota;
            active_users.insert(item.user_id);
            active_tokens.insert(item.token_id);
        }

        Ok(DashboardOverviewVo {
            today_request_count,
            today_total_tokens,
            today_total_quota,
            active_user_count: active_users.len() as i64,
            active_token_count: active_tokens.len() as i64,
        })
    }

    pub fn record_chat_completion_async(
        &self,
        token_info: &TokenInfo,
        channel: &SelectedChannel,
        usage: &Usage,
        record: ChatCompletionLogRecord,
    ) {
        self.record_async(CreateLogDto {
            user_id: token_info.user_id,
            token_id: token_info.token_id,
            token_name: token_info.name.clone(),
            project_id: 0,
            conversation_id: 0,
            message_id: 0,
            session_id: 0,
            thread_id: 0,
            trace_id: 0,
            channel_id: channel.channel_id,
            channel_name: channel.channel_name.clone(),
            account_id: channel.account_id,
            account_name: channel.account_name.clone(),
            execution_id: 0,
            endpoint: record.endpoint,
            request_format: "openai".into(),
            requested_model: record.requested_model,
            upstream_model: record.upstream_model,
            model_name: record.model_name,
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: usage.completion_tokens,
            total_tokens: usage.total_tokens,
            cached_tokens: usage.cached_tokens,
            reasoning_tokens: usage.reasoning_tokens,
            quota: record.quota,
            cost_total: BigDecimal::from(0),
            price_reference: String::new(),
            elapsed_time: record.elapsed_time,
            first_token_time: record.first_token_time,
            is_stream: record.is_stream,
            request_id: String::new(),
            upstream_request_id: String::new(),
            status_code: 200,
            client_ip: record.client_ip,
            user_agent: String::new(),
            content: String::new(),
            log_type: LogType::Consume,
            status: LogStatus::Success,
        });
    }

    pub fn record_embedding_async(
        &self,
        token_info: &TokenInfo,
        channel: &SelectedChannel,
        usage: &Usage,
        record: EmbeddingLogRecord,
    ) {
        self.record_async(CreateLogDto {
            user_id: token_info.user_id,
            token_id: token_info.token_id,
            token_name: token_info.name.clone(),
            project_id: 0,
            conversation_id: 0,
            message_id: 0,
            session_id: 0,
            thread_id: 0,
            trace_id: 0,
            channel_id: channel.channel_id,
            channel_name: channel.channel_name.clone(),
            account_id: channel.account_id,
            account_name: channel.account_name.clone(),
            execution_id: 0,
            endpoint: "embeddings".into(),
            request_format: "openai".into(),
            requested_model: record.requested_model,
            upstream_model: record.upstream_model,
            model_name: record.model_name,
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: 0,
            total_tokens: usage.total_tokens,
            cached_tokens: 0,
            reasoning_tokens: 0,
            quota: record.quota,
            cost_total: BigDecimal::from(0),
            price_reference: String::new(),
            elapsed_time: record.elapsed_time,
            first_token_time: 0,
            is_stream: false,
            request_id: String::new(),
            upstream_request_id: String::new(),
            status_code: 200,
            client_ip: record.client_ip,
            user_agent: String::new(),
            content: String::new(),
            log_type: LogType::Consume,
            status: LogStatus::Success,
        });
    }

    pub fn record_endpoint_usage_async(
        &self,
        token_info: &TokenInfo,
        channel: &SelectedChannel,
        usage: &Usage,
        record: EndpointUsageLogRecord,
    ) {
        self.record_async(CreateLogDto {
            user_id: token_info.user_id,
            token_id: token_info.token_id,
            token_name: token_info.name.clone(),
            project_id: 0,
            conversation_id: 0,
            message_id: 0,
            session_id: 0,
            thread_id: 0,
            trace_id: 0,
            channel_id: channel.channel_id,
            channel_name: channel.channel_name.clone(),
            account_id: channel.account_id,
            account_name: channel.account_name.clone(),
            execution_id: 0,
            endpoint: record.endpoint,
            request_format: "openai".into(),
            requested_model: record.requested_model,
            upstream_model: record.upstream_model,
            model_name: record.model_name,
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: usage.completion_tokens,
            total_tokens: usage.total_tokens,
            cached_tokens: usage.cached_tokens,
            reasoning_tokens: usage.reasoning_tokens,
            quota: record.quota,
            cost_total: BigDecimal::from(0),
            price_reference: String::new(),
            elapsed_time: record.elapsed_time,
            first_token_time: record.first_token_time,
            is_stream: record.is_stream,
            request_id: String::new(),
            upstream_request_id: String::new(),
            status_code: 200,
            client_ip: record.client_ip,
            user_agent: String::new(),
            content: String::new(),
            log_type: LogType::Consume,
            status: LogStatus::Success,
        });
    }
}

#[derive(Debug, Clone, Copy)]
enum LogStatsGroupBy {
    User,
    Channel,
    Model,
    Day,
}

#[derive(Debug, Default)]
struct LogStatsAccumulator {
    request_count: i64,
    total_tokens: i64,
    total_quota: i64,
    total_elapsed_time: i64,
}

fn normalize_log_group_by(group_by: Option<&str>) -> LogStatsGroupBy {
    match group_by
        .unwrap_or("model")
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "user" => LogStatsGroupBy::User,
        "channel" => LogStatsGroupBy::Channel,
        "day" | "date" => LogStatsGroupBy::Day,
        _ => LogStatsGroupBy::Model,
    }
}

fn build_log_stats_condition(query: &LogStatsQueryDto) -> sea_orm::Condition {
    let mut cond = sea_orm::Condition::all();
    if let Some(v) = query.start_time {
        cond = cond.add(log::Column::CreateTime.gte(v));
    }
    if let Some(v) = query.end_time {
        cond = cond.add(log::Column::CreateTime.lte(v));
    }
    if let Some(v) = query.user_id {
        cond = cond.add(log::Column::UserId.eq(v));
    }
    if let Some(v) = query.model_name.as_ref() {
        cond = cond.add(log::Column::ModelName.contains(v));
    }
    if let Some(v) = query.channel_id {
        cond = cond.add(log::Column::ChannelId.eq(v));
    }
    cond
}
