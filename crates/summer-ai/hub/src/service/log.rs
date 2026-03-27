use std::collections::{HashMap, HashSet};

use anyhow::Context;
use sea_orm::prelude::BigDecimal;
use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder};
use summer::plugin::Service;
use summer_common::error::{ApiErrors, ApiResult};
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};

use summer_ai_core::types::common::Usage;
use summer_ai_model::dto::log::{CreateLogDto, LogStatsQueryDto, QueryLogDto};
use summer_ai_model::entity::log::{self, LogStatus, LogType};
use summer_ai_model::entity::token;
use summer_ai_model::vo::dashboard::DashboardOverviewVo;
use summer_ai_model::vo::log::{LogStatsVo, LogVo};

use crate::relay::channel_router::SelectedChannel;
use crate::service::log_batch::AiLogBatchQueue;
use crate::service::token::TokenInfo;

pub struct AiUsageLogRecord {
    pub endpoint: String,
    pub request_format: String,
    pub request_id: String,
    pub upstream_request_id: String,
    pub requested_model: String,
    pub upstream_model: String,
    pub model_name: String,
    pub quota: i64,
    pub elapsed_time: i32,
    pub first_token_time: i32,
    pub is_stream: bool,
    pub client_ip: String,
    pub user_agent: String,
}

pub struct ChatCompletionLogRecord {
    pub request_id: String,
    pub upstream_request_id: String,
    pub requested_model: String,
    pub upstream_model: String,
    pub model_name: String,
    pub quota: i64,
    pub elapsed_time: i32,
    pub first_token_time: i32,
    pub is_stream: bool,
    pub client_ip: String,
    pub user_agent: String,
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

    pub fn record_chat_completion_async(
        &self,
        token_info: &TokenInfo,
        channel: &SelectedChannel,
        usage: &Usage,
        record: ChatCompletionLogRecord,
    ) {
        self.record_usage_async(
            token_info,
            channel,
            usage,
            AiUsageLogRecord {
                endpoint: "chat/completions".into(),
                request_format: "openai/chat_completions".into(),
                request_id: record.request_id,
                upstream_request_id: record.upstream_request_id,
                requested_model: record.requested_model,
                upstream_model: record.upstream_model,
                model_name: record.model_name,
                quota: record.quota,
                elapsed_time: record.elapsed_time,
                first_token_time: record.first_token_time,
                is_stream: record.is_stream,
                client_ip: record.client_ip,
                user_agent: record.user_agent,
            },
        );
    }

    pub fn record_usage_async(
        &self,
        token_info: &TokenInfo,
        channel: &SelectedChannel,
        usage: &Usage,
        record: AiUsageLogRecord,
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
            request_format: record.request_format,
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
            request_id: record.request_id,
            upstream_request_id: record.upstream_request_id,
            status_code: 200,
            client_ip: record.client_ip,
            user_agent: record.user_agent,
            content: String::new(),
            log_type: LogType::Consume,
            status: LogStatus::Success,
        });
    }

    pub async fn query_logs(
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
            .context("查询 AI 消费日志失败")?;

        Ok(page.map(LogVo::from_model))
    }

    pub async fn stats(&self, query: LogStatsQueryDto) -> ApiResult<Vec<LogStatsVo>> {
        let group_by = query.group_by.clone().unwrap_or_else(|| "day".to_string());
        let logs = log::Entity::find()
            .filter(query)
            .order_by_desc(log::Column::CreateTime)
            .all(&self.db)
            .await
            .context("查询 AI 消费统计失败")
            .map_err(ApiErrors::Internal)?;

        let mut grouped: HashMap<String, (i64, i64, i64, i64)> = HashMap::new();
        for item in logs {
            let key = match group_by.as_str() {
                "model" => item.model_name.clone(),
                "channel" => item.channel_name.clone(),
                "user" => item.user_id.to_string(),
                _ => item.create_time.format("%Y-%m-%d").to_string(),
            };

            let entry = grouped.entry(key).or_insert((0, 0, 0, 0));
            entry.0 += 1;
            entry.1 += item.total_tokens as i64;
            entry.2 += item.quota;
            entry.3 += item.elapsed_time as i64;
        }

        let mut stats: Vec<LogStatsVo> = grouped
            .into_iter()
            .map(
                |(group_key, (request_count, total_tokens, total_quota, total_elapsed_time))| {
                    LogStatsVo {
                        group_key,
                        request_count,
                        total_tokens,
                        total_quota,
                        avg_elapsed_time: if request_count == 0 {
                            0.0
                        } else {
                            total_elapsed_time as f64 / request_count as f64
                        },
                    }
                },
            )
            .collect();

        stats.sort_by(|a, b| a.group_key.cmp(&b.group_key));
        Ok(stats)
    }

    pub async fn dashboard_overview(&self) -> ApiResult<DashboardOverviewVo> {
        let now = chrono::Local::now().fixed_offset();
        let start_of_day = now
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .expect("valid start of day")
            .and_local_timezone(*now.offset())
            .single()
            .expect("single timezone conversion");

        let logs = log::Entity::find()
            .filter(log::Column::CreateTime.gte(start_of_day))
            .all(&self.db)
            .await
            .context("查询今日 AI 日志失败")
            .map_err(ApiErrors::Internal)?;

        let token_count = token::Entity::find()
            .count(&self.db)
            .await
            .context("查询令牌数量失败")
            .map_err(ApiErrors::Internal)? as i64;

        let mut users = HashSet::new();
        let mut tokens = HashSet::new();
        let mut success_request_count = 0_i64;
        let mut failed_request_count = 0_i64;
        let mut today_total_quota = 0_i64;
        let mut today_total_tokens = 0_i64;

        for item in logs.iter() {
            users.insert(item.user_id);
            tokens.insert(item.token_id);
            today_total_quota += item.quota;
            today_total_tokens += item.total_tokens as i64;
            if item.status == LogStatus::Success {
                success_request_count += 1;
            } else {
                failed_request_count += 1;
            }
        }

        Ok(DashboardOverviewVo {
            today_request_count: logs.len() as i64,
            today_total_quota,
            today_total_tokens,
            active_user_count: users.len() as i64,
            active_token_count: if tokens.is_empty() {
                token_count
            } else {
                tokens.len() as i64
            },
            success_request_count,
            failed_request_count,
        })
    }
}
