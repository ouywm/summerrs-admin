use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use anyhow::Context;
use chrono::Timelike;
use sea_orm::prelude::BigDecimal;
use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect};
use summer::plugin::Service;
use summer_common::error::{ApiErrors, ApiResult};
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};

use summer_ai_core::types::common::Usage;
use summer_ai_model::dto::log::{CreateLogDto, LogStatsQueryDto, QueryLogDto};
use summer_ai_model::entity::channel::{self, ChannelStatus};
use summer_ai_model::entity::channel_account::{self, AccountStatus};
use summer_ai_model::entity::log::{self, LogStatus, LogType};
use summer_ai_model::entity::token;
use summer_ai_model::vo::dashboard::{
    DashboardBreakdownVo, DashboardOverviewVo, DashboardTrendPointVo, FailureHotspotVo,
    RecentFailureVo, TopRequestVo,
};
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
    pub status_code: i32,
    pub content: String,
    pub status: LogStatus,
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

pub struct AiFailureLogRecord {
    pub endpoint: String,
    pub request_format: String,
    pub request_id: String,
    pub upstream_request_id: String,
    pub requested_model: String,
    pub upstream_model: String,
    pub model_name: String,
    pub elapsed_time: i32,
    pub is_stream: bool,
    pub client_ip: String,
    pub user_agent: String,
    pub status_code: i32,
    pub content: String,
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
                status_code: 200,
                content: String::new(),
                status: LogStatus::Success,
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
        self.record_async(build_usage_log_dto(token_info, channel, usage, record));
    }

    pub fn record_failure_async(
        &self,
        token_info: &TokenInfo,
        channel: &SelectedChannel,
        record: AiFailureLogRecord,
    ) {
        self.record_async(build_failure_log_dto(token_info, channel, record));
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
            .expect("valid start of day") // chrono: constructing midnight from valid date components — cannot fail
            .and_local_timezone(*now.offset())
            .single()
            .expect("single timezone conversion"); // chrono: converting back to same offset — cannot fail

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
        let channels = channel::Entity::find()
            .filter(channel::Column::DeletedAt.is_null())
            .all(&self.db)
            .await
            .context("查询 AI 渠道运行时状态失败")
            .map_err(ApiErrors::Internal)?;
        let accounts = channel_account::Entity::find()
            .filter(channel_account::Column::DeletedAt.is_null())
            .all(&self.db)
            .await
            .context("查询 AI 渠道账号运行时状态失败")
            .map_err(ApiErrors::Internal)?;

        Ok(summarize_dashboard_overview(
            logs,
            token_count,
            channels,
            accounts,
            now,
        ))
    }

    pub async fn recent_failures(
        &self,
        limit: Option<u64>,
        start_time: Option<chrono::DateTime<chrono::FixedOffset>>,
        end_time: Option<chrono::DateTime<chrono::FixedOffset>>,
    ) -> ApiResult<Vec<RecentFailureVo>> {
        let now = chrono::Local::now().fixed_offset();
        let limit = clamp_recent_failures_limit(limit);
        let (start_time, end_time) =
            resolve_dashboard_window(now, chrono::Duration::days(1), start_time, end_time);
        let items = log::Entity::find()
            .filter(log::Column::Status.eq(LogStatus::Failed))
            .filter(log::Column::CreateTime.gte(start_time))
            .filter(log::Column::CreateTime.lte(end_time))
            .order_by_desc(log::Column::CreateTime)
            .order_by_desc(log::Column::Id)
            .limit(limit)
            .all(&self.db)
            .await
            .context("查询最近失败 AI 请求失败")
            .map_err(ApiErrors::Internal)?;

        Ok(items.into_iter().map(RecentFailureVo::from_model).collect())
    }

    pub async fn failure_hotspots(
        &self,
        group_by: Option<String>,
        limit: Option<u64>,
        start_time: Option<chrono::DateTime<chrono::FixedOffset>>,
        end_time: Option<chrono::DateTime<chrono::FixedOffset>>,
    ) -> ApiResult<Vec<FailureHotspotVo>> {
        let now = chrono::Local::now().fixed_offset();
        let (start_time, end_time) =
            resolve_dashboard_window(now, chrono::Duration::days(1), start_time, end_time);
        let logs = log::Entity::find()
            .filter(log::Column::Status.eq(LogStatus::Failed))
            .filter(log::Column::CreateTime.gte(start_time))
            .filter(log::Column::CreateTime.lte(end_time))
            .order_by_desc(log::Column::CreateTime)
            .order_by_desc(log::Column::Id)
            .all(&self.db)
            .await
            .context("查询失败热点统计失败")
            .map_err(ApiErrors::Internal)?;

        Ok(summarize_failure_hotspots(
            logs,
            normalize_failure_hotspot_group_by(group_by.as_deref()),
            clamp_recent_failures_limit(limit),
        ))
    }

    pub async fn dashboard_trends(
        &self,
        period: Option<String>,
        limit: Option<u64>,
        start_time: Option<chrono::DateTime<chrono::FixedOffset>>,
        end_time: Option<chrono::DateTime<chrono::FixedOffset>>,
    ) -> ApiResult<Vec<DashboardTrendPointVo>> {
        let now = chrono::Local::now().fixed_offset();
        let period = normalize_dashboard_trend_period(period.as_deref());
        let limit = clamp_dashboard_trend_limit(limit);
        let query_end_time = end_time.unwrap_or(now);
        let (start_time, end_time) =
            resolve_dashboard_trend_window(now, period, limit, start_time, end_time);
        let query_end_time = if query_end_time < start_time {
            end_time
        } else {
            query_end_time
        };

        let logs = log::Entity::find()
            .filter(log::Column::CreateTime.gte(start_time))
            .filter(log::Column::CreateTime.lte(query_end_time))
            .order_by_asc(log::Column::CreateTime)
            .all(&self.db)
            .await
            .context("查询 AI 仪表盘趋势失败")
            .map_err(ApiErrors::Internal)?;

        Ok(summarize_dashboard_trends(
            logs, period, limit, start_time, end_time,
        ))
    }

    pub async fn top_slow_requests(
        &self,
        limit: Option<u64>,
        start_time: Option<chrono::DateTime<chrono::FixedOffset>>,
        end_time: Option<chrono::DateTime<chrono::FixedOffset>>,
    ) -> ApiResult<Vec<TopRequestVo>> {
        let now = chrono::Local::now().fixed_offset();
        let (start_time, end_time) =
            resolve_dashboard_window(now, chrono::Duration::days(1), start_time, end_time);
        let logs = log::Entity::find()
            .filter(log::Column::CreateTime.gte(start_time))
            .filter(log::Column::CreateTime.lte(end_time))
            .order_by_desc(log::Column::ElapsedTime)
            .order_by_desc(log::Column::FirstTokenTime)
            .order_by_desc(log::Column::CreateTime)
            .all(&self.db)
            .await
            .context("查询慢请求排行失败")
            .map_err(ApiErrors::Internal)?;

        Ok(summarize_top_slow_requests(
            logs,
            clamp_top_requests_limit(limit),
        ))
    }

    pub async fn top_usage_requests(
        &self,
        limit: Option<u64>,
        start_time: Option<chrono::DateTime<chrono::FixedOffset>>,
        end_time: Option<chrono::DateTime<chrono::FixedOffset>>,
    ) -> ApiResult<Vec<TopRequestVo>> {
        let now = chrono::Local::now().fixed_offset();
        let (start_time, end_time) =
            resolve_dashboard_window(now, chrono::Duration::days(1), start_time, end_time);
        let logs = log::Entity::find()
            .filter(log::Column::Status.eq(LogStatus::Success))
            .filter(log::Column::CreateTime.gte(start_time))
            .filter(log::Column::CreateTime.lte(end_time))
            .order_by_desc(log::Column::Quota)
            .order_by_desc(log::Column::TotalTokens)
            .order_by_desc(log::Column::CreateTime)
            .all(&self.db)
            .await
            .context("查询高消耗请求排行失败")
            .map_err(ApiErrors::Internal)?;

        Ok(summarize_top_usage_requests(
            logs,
            clamp_top_requests_limit(limit),
        ))
    }

    pub async fn top_cost_requests(
        &self,
        limit: Option<u64>,
        start_time: Option<chrono::DateTime<chrono::FixedOffset>>,
        end_time: Option<chrono::DateTime<chrono::FixedOffset>>,
    ) -> ApiResult<Vec<TopRequestVo>> {
        let now = chrono::Local::now().fixed_offset();
        let (start_time, end_time) =
            resolve_dashboard_window(now, chrono::Duration::days(1), start_time, end_time);
        let logs = log::Entity::find()
            .filter(log::Column::Status.eq(LogStatus::Success))
            .filter(log::Column::CreateTime.gte(start_time))
            .filter(log::Column::CreateTime.lte(end_time))
            .order_by_desc(log::Column::CostTotal)
            .order_by_desc(log::Column::Quota)
            .order_by_desc(log::Column::CreateTime)
            .all(&self.db)
            .await
            .context("查询高成本请求排行失败")
            .map_err(ApiErrors::Internal)?;

        Ok(summarize_top_cost_requests(
            logs,
            clamp_top_requests_limit(limit),
        ))
    }

    pub async fn top_first_token_requests(
        &self,
        limit: Option<u64>,
        start_time: Option<chrono::DateTime<chrono::FixedOffset>>,
        end_time: Option<chrono::DateTime<chrono::FixedOffset>>,
    ) -> ApiResult<Vec<TopRequestVo>> {
        let now = chrono::Local::now().fixed_offset();
        let (start_time, end_time) =
            resolve_dashboard_window(now, chrono::Duration::days(1), start_time, end_time);
        let logs = log::Entity::find()
            .filter(log::Column::IsStream.eq(true))
            .filter(log::Column::FirstTokenTime.gt(0))
            .filter(log::Column::CreateTime.gte(start_time))
            .filter(log::Column::CreateTime.lte(end_time))
            .order_by_desc(log::Column::FirstTokenTime)
            .order_by_desc(log::Column::ElapsedTime)
            .order_by_desc(log::Column::CreateTime)
            .all(&self.db)
            .await
            .context("查询首 Token 最慢请求排行失败")
            .map_err(ApiErrors::Internal)?;

        Ok(summarize_top_first_token_requests(
            logs,
            clamp_top_requests_limit(limit),
        ))
    }

    pub async fn dashboard_breakdown(
        &self,
        group_by: Option<String>,
        limit: Option<u64>,
        start_time: Option<chrono::DateTime<chrono::FixedOffset>>,
        end_time: Option<chrono::DateTime<chrono::FixedOffset>>,
    ) -> ApiResult<Vec<DashboardBreakdownVo>> {
        let now = chrono::Local::now().fixed_offset();
        let (start_time, end_time) =
            resolve_dashboard_window(now, chrono::Duration::days(1), start_time, end_time);
        let group_by = normalize_dashboard_breakdown_group_by(group_by.as_deref());
        let limit = clamp_dashboard_breakdown_limit(limit);

        let logs = log::Entity::find()
            .filter(log::Column::CreateTime.gte(start_time))
            .filter(log::Column::CreateTime.lte(end_time))
            .order_by_desc(log::Column::CreateTime)
            .all(&self.db)
            .await
            .context("查询仪表盘分组汇总失败")
            .map_err(ApiErrors::Internal)?;

        Ok(summarize_dashboard_breakdown(logs, group_by, limit))
    }
}

fn summarize_dashboard_overview(
    logs: Vec<log::Model>,
    token_count: i64,
    channels: Vec<channel::Model>,
    accounts: Vec<channel_account::Model>,
    now: chrono::DateTime<chrono::FixedOffset>,
) -> DashboardOverviewVo {
    let mut users = HashSet::new();
    let mut active_tokens = HashSet::new();
    let mut success_request_count = 0_i64;
    let mut failed_request_count = 0_i64;
    let mut stream_request_count = 0_i64;
    let mut upstream_request_id_coverage_count = 0_i64;
    let mut today_total_quota = 0_i64;
    let mut today_total_tokens = 0_i64;
    let mut total_elapsed_time = 0_i64;
    let mut total_stream_first_token_time = 0_i64;
    let mut stream_first_token_samples = 0_i64;

    for item in &logs {
        users.insert(item.user_id);
        active_tokens.insert(item.token_id);
        today_total_quota += item.quota;
        today_total_tokens += item.total_tokens as i64;
        total_elapsed_time += item.elapsed_time as i64;

        if item.status == LogStatus::Success {
            success_request_count += 1;
        } else {
            failed_request_count += 1;
        }
        if item.is_stream {
            stream_request_count += 1;
            if item.first_token_time > 0 {
                total_stream_first_token_time += item.first_token_time as i64;
                stream_first_token_samples += 1;
            }
        }
        if !item.upstream_request_id.trim().is_empty() {
            upstream_request_id_coverage_count += 1;
        }
    }

    let total_account_count = accounts.len() as i64;
    let enabled_account_count = accounts
        .iter()
        .filter(|item| item.status == AccountStatus::Enabled)
        .count() as i64;
    let available_account_count = accounts
        .iter()
        .filter(|item| dashboard_account_is_available(item, now))
        .count() as i64;
    let rate_limited_account_count = accounts
        .iter()
        .filter(|item| {
            item.rate_limited_until
                .is_some_and(|recover_at| recover_at > now)
        })
        .count() as i64;
    let overloaded_account_count = accounts
        .iter()
        .filter(|item| {
            item.overload_until
                .is_some_and(|recover_at| recover_at > now)
        })
        .count() as i64;
    let disabled_account_count = accounts
        .iter()
        .filter(|item| item.status == AccountStatus::Disabled)
        .count() as i64;
    let unschedulable_account_count =
        accounts.iter().filter(|item| !item.schedulable).count() as i64;

    let mut available_channel_ids = HashSet::new();
    for account in &accounts {
        if dashboard_account_is_available(account, now) {
            available_channel_ids.insert(account.channel_id);
        }
    }

    let total_channel_count = channels.len() as i64;
    let enabled_channel_count = channels
        .iter()
        .filter(|item| item.status == ChannelStatus::Enabled)
        .count() as i64;
    let available_channel_count = channels
        .iter()
        .filter(|item| {
            item.status == ChannelStatus::Enabled && available_channel_ids.contains(&item.id)
        })
        .count() as i64;
    let auto_disabled_channel_count = channels
        .iter()
        .filter(|item| item.status == ChannelStatus::AutoDisabled)
        .count() as i64;

    DashboardOverviewVo {
        today_request_count: logs.len() as i64,
        today_total_quota,
        today_total_tokens,
        active_user_count: users.len() as i64,
        active_token_count: if active_tokens.is_empty() {
            token_count
        } else {
            active_tokens.len() as i64
        },
        success_request_count,
        failed_request_count,
        stream_request_count,
        upstream_request_id_coverage_count,
        avg_elapsed_time: if logs.is_empty() {
            0.0
        } else {
            total_elapsed_time as f64 / logs.len() as f64
        },
        avg_stream_first_token_time: if stream_first_token_samples == 0 {
            0.0
        } else {
            total_stream_first_token_time as f64 / stream_first_token_samples as f64
        },
        total_channel_count,
        enabled_channel_count,
        available_channel_count,
        auto_disabled_channel_count,
        total_account_count,
        enabled_account_count,
        available_account_count,
        rate_limited_account_count,
        overloaded_account_count,
        disabled_account_count,
        unschedulable_account_count,
    }
}

fn dashboard_account_is_available(
    account: &channel_account::Model,
    now: chrono::DateTime<chrono::FixedOffset>,
) -> bool {
    account.status == AccountStatus::Enabled
        && account.schedulable
        && account.deleted_at.is_none()
        && !crate::service::channel::ChannelService::extract_api_key(&account.credentials)
            .is_empty()
        && account.expires_at.is_none_or(|expires_at| expires_at > now)
        && account
            .rate_limited_until
            .is_none_or(|recover_at| recover_at <= now)
        && account
            .overload_until
            .is_none_or(|recover_at| recover_at <= now)
}

fn clamp_recent_failures_limit(limit: Option<u64>) -> u64 {
    limit.unwrap_or(20).clamp(1, 100)
}

fn normalize_failure_hotspot_group_by(group_by: Option<&str>) -> &'static str {
    match group_by.unwrap_or("channel") {
        "account" => "account",
        "model" => "model",
        "endpoint" => "endpoint",
        _ => "channel",
    }
}

fn normalize_dashboard_trend_period(period: Option<&str>) -> &'static str {
    match period.unwrap_or("hour") {
        "day" => "day",
        _ => "hour",
    }
}

fn clamp_dashboard_trend_limit(limit: Option<u64>) -> u64 {
    limit.unwrap_or(24).clamp(1, 168)
}

fn clamp_top_requests_limit(limit: Option<u64>) -> u64 {
    limit.unwrap_or(20).clamp(1, 100)
}

fn normalize_dashboard_breakdown_group_by(group_by: Option<&str>) -> &'static str {
    match group_by.unwrap_or("channel") {
        "account" => "account",
        "model" => "model",
        "endpoint" => "endpoint",
        _ => "channel",
    }
}

fn clamp_dashboard_breakdown_limit(limit: Option<u64>) -> u64 {
    limit.unwrap_or(20).clamp(1, 100)
}

fn resolve_dashboard_window(
    now: chrono::DateTime<chrono::FixedOffset>,
    default_span: chrono::Duration,
    start_time: Option<chrono::DateTime<chrono::FixedOffset>>,
    end_time: Option<chrono::DateTime<chrono::FixedOffset>>,
) -> (
    chrono::DateTime<chrono::FixedOffset>,
    chrono::DateTime<chrono::FixedOffset>,
) {
    let end_time = end_time.unwrap_or(now);
    let start_time = start_time.unwrap_or(end_time - default_span);
    if start_time <= end_time {
        (start_time, end_time)
    } else {
        (end_time, start_time)
    }
}

fn resolve_dashboard_trend_window(
    now: chrono::DateTime<chrono::FixedOffset>,
    period: &'static str,
    limit: u64,
    start_time: Option<chrono::DateTime<chrono::FixedOffset>>,
    end_time: Option<chrono::DateTime<chrono::FixedOffset>>,
) -> (
    chrono::DateTime<chrono::FixedOffset>,
    chrono::DateTime<chrono::FixedOffset>,
) {
    let end_time = end_time.unwrap_or(now);
    let bucket_end = truncate_dashboard_bucket(end_time, period);
    let start_time = start_time.unwrap_or_else(|| {
        let span = match period {
            "day" => chrono::Duration::days(limit.saturating_sub(1) as i64),
            _ => chrono::Duration::hours(limit.saturating_sub(1) as i64),
        };
        bucket_end - span
    });

    (
        truncate_dashboard_bucket(start_time, period),
        truncate_dashboard_bucket(end_time, period),
    )
}

fn truncate_dashboard_bucket(
    value: chrono::DateTime<chrono::FixedOffset>,
    period: &'static str,
) -> chrono::DateTime<chrono::FixedOffset> {
    let naive = match period {
        "day" => value
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .expect("valid day bucket"), // chrono: constructing midnight from valid date components — cannot fail
        _ => value
            .date_naive()
            .and_hms_opt(value.hour(), 0, 0)
            .expect("valid hour bucket"), // chrono: constructing hour from valid date components — cannot fail
    };
    naive
        .and_local_timezone(*value.offset())
        .single()
        .expect("single timezone conversion") // chrono: converting back to same offset — cannot fail
}

fn summarize_dashboard_trends(
    logs: Vec<log::Model>,
    period: &'static str,
    limit: u64,
    start_time: chrono::DateTime<chrono::FixedOffset>,
    end_time: chrono::DateTime<chrono::FixedOffset>,
) -> Vec<DashboardTrendPointVo> {
    #[derive(Debug, Default)]
    struct TrendAggregate {
        request_count: i64,
        success_request_count: i64,
        failed_request_count: i64,
        stream_request_count: i64,
        auth_failure_count: i64,
        rate_limit_failure_count: i64,
        overload_failure_count: i64,
        invalid_request_failure_count: i64,
        other_failure_count: i64,
        total_elapsed_time: i64,
        total_first_token_time: i64,
        first_token_samples: i64,
    }

    let mut grouped: HashMap<chrono::DateTime<chrono::FixedOffset>, TrendAggregate> =
        HashMap::new();

    for item in logs {
        let bucket = truncate_dashboard_bucket(item.create_time, period);
        let aggregate = grouped.entry(bucket).or_default();
        aggregate.request_count += 1;
        aggregate.total_elapsed_time += item.elapsed_time as i64;
        if item.status == LogStatus::Success {
            aggregate.success_request_count += 1;
        } else {
            aggregate.failed_request_count += 1;
            match item.status_code {
                401 | 403 => aggregate.auth_failure_count += 1,
                429 => aggregate.rate_limit_failure_count += 1,
                400 | 404 | 413 | 422 => aggregate.invalid_request_failure_count += 1,
                500..=599 => aggregate.overload_failure_count += 1,
                _ => aggregate.other_failure_count += 1,
            }
        }
        if item.is_stream {
            aggregate.stream_request_count += 1;
        }
        if item.first_token_time > 0 {
            aggregate.total_first_token_time += item.first_token_time as i64;
            aggregate.first_token_samples += 1;
        }
    }

    let mut items = Vec::new();
    let mut bucket = start_time;
    while bucket <= end_time && items.len() < limit as usize {
        let aggregate = grouped.remove(&bucket).unwrap_or_default();
        items.push(DashboardTrendPointVo {
            bucket_start: bucket,
            request_count: aggregate.request_count,
            success_request_count: aggregate.success_request_count,
            failed_request_count: aggregate.failed_request_count,
            stream_request_count: aggregate.stream_request_count,
            auth_failure_count: aggregate.auth_failure_count,
            rate_limit_failure_count: aggregate.rate_limit_failure_count,
            overload_failure_count: aggregate.overload_failure_count,
            invalid_request_failure_count: aggregate.invalid_request_failure_count,
            other_failure_count: aggregate.other_failure_count,
            avg_elapsed_time: if aggregate.request_count == 0 {
                0.0
            } else {
                aggregate.total_elapsed_time as f64 / aggregate.request_count as f64
            },
            avg_first_token_time: if aggregate.first_token_samples == 0 {
                0.0
            } else {
                aggregate.total_first_token_time as f64 / aggregate.first_token_samples as f64
            },
        });
        bucket = match period {
            "day" => bucket + chrono::Duration::days(1),
            _ => bucket + chrono::Duration::hours(1),
        };
    }

    items
}

fn summarize_top_slow_requests(logs: Vec<log::Model>, limit: u64) -> Vec<TopRequestVo> {
    let mut logs = logs;
    logs.sort_by(|left, right| {
        right
            .elapsed_time
            .cmp(&left.elapsed_time)
            .then_with(|| right.first_token_time.cmp(&left.first_token_time))
            .then_with(|| right.create_time.cmp(&left.create_time))
            .then_with(|| left.request_id.cmp(&right.request_id))
    });
    logs.truncate(limit as usize);
    logs.into_iter().map(TopRequestVo::from_model).collect()
}

fn summarize_top_usage_requests(logs: Vec<log::Model>, limit: u64) -> Vec<TopRequestVo> {
    let mut logs = logs;
    logs.sort_by(|left, right| {
        right
            .quota
            .cmp(&left.quota)
            .then_with(|| right.total_tokens.cmp(&left.total_tokens))
            .then_with(|| right.create_time.cmp(&left.create_time))
            .then_with(|| left.request_id.cmp(&right.request_id))
    });
    logs.truncate(limit as usize);
    logs.into_iter().map(TopRequestVo::from_model).collect()
}

fn summarize_top_cost_requests(logs: Vec<log::Model>, limit: u64) -> Vec<TopRequestVo> {
    let mut logs = logs;
    logs.sort_by(|left, right| {
        right
            .cost_total
            .partial_cmp(&left.cost_total)
            .unwrap_or(Ordering::Equal)
            .then_with(|| right.quota.cmp(&left.quota))
            .then_with(|| right.total_tokens.cmp(&left.total_tokens))
            .then_with(|| right.create_time.cmp(&left.create_time))
            .then_with(|| left.request_id.cmp(&right.request_id))
    });
    logs.truncate(limit as usize);
    logs.into_iter().map(TopRequestVo::from_model).collect()
}

fn summarize_top_first_token_requests(logs: Vec<log::Model>, limit: u64) -> Vec<TopRequestVo> {
    let mut logs = logs;
    logs.sort_by(|left, right| {
        right
            .first_token_time
            .cmp(&left.first_token_time)
            .then_with(|| right.elapsed_time.cmp(&left.elapsed_time))
            .then_with(|| right.create_time.cmp(&left.create_time))
            .then_with(|| left.request_id.cmp(&right.request_id))
    });
    logs.truncate(limit as usize);
    logs.into_iter().map(TopRequestVo::from_model).collect()
}

fn summarize_dashboard_breakdown(
    logs: Vec<log::Model>,
    group_by: &'static str,
    limit: u64,
) -> Vec<DashboardBreakdownVo> {
    #[derive(Default)]
    struct BreakdownAggregate {
        request_count: i64,
        success_request_count: i64,
        failed_request_count: i64,
        total_elapsed_time: i64,
        total_first_token_time: i64,
        first_token_samples: i64,
        total_tokens: i64,
        total_quota: i64,
    }

    let mut grouped: HashMap<String, BreakdownAggregate> = HashMap::new();

    for item in logs {
        let key = match group_by {
            "account" => item.account_name.clone(),
            "model" => item.model_name.clone(),
            "endpoint" => item.endpoint.clone(),
            _ => item.channel_name.clone(),
        };
        let aggregate = grouped.entry(key).or_default();
        aggregate.request_count += 1;
        aggregate.total_elapsed_time += item.elapsed_time as i64;
        aggregate.total_tokens += item.total_tokens as i64;
        aggregate.total_quota += item.quota;
        if item.status == LogStatus::Success {
            aggregate.success_request_count += 1;
        } else {
            aggregate.failed_request_count += 1;
        }
        if item.first_token_time > 0 {
            aggregate.total_first_token_time += item.first_token_time as i64;
            aggregate.first_token_samples += 1;
        }
    }

    let mut items: Vec<_> = grouped
        .into_iter()
        .map(|(group_key, aggregate)| DashboardBreakdownVo {
            group_key,
            request_count: aggregate.request_count,
            success_request_count: aggregate.success_request_count,
            failed_request_count: aggregate.failed_request_count,
            success_rate: if aggregate.request_count == 0 {
                0.0
            } else {
                aggregate.success_request_count as f64 / aggregate.request_count as f64
            },
            failure_rate: if aggregate.request_count == 0 {
                0.0
            } else {
                aggregate.failed_request_count as f64 / aggregate.request_count as f64
            },
            avg_elapsed_time: if aggregate.request_count == 0 {
                0.0
            } else {
                aggregate.total_elapsed_time as f64 / aggregate.request_count as f64
            },
            avg_first_token_time: if aggregate.first_token_samples == 0 {
                0.0
            } else {
                aggregate.total_first_token_time as f64 / aggregate.first_token_samples as f64
            },
            total_tokens: aggregate.total_tokens,
            total_quota: aggregate.total_quota,
        })
        .collect();

    items.sort_by(|left, right| {
        right
            .request_count
            .cmp(&left.request_count)
            .then_with(|| right.failed_request_count.cmp(&left.failed_request_count))
            .then_with(|| left.group_key.cmp(&right.group_key))
    });
    items.truncate(limit as usize);
    items
}

fn summarize_failure_hotspots(
    logs: Vec<log::Model>,
    group_by: &'static str,
    limit: u64,
) -> Vec<FailureHotspotVo> {
    #[derive(Debug)]
    struct FailureAggregate {
        failed_request_count: i64,
        stream_failure_count: i64,
        auth_failure_count: i64,
        rate_limit_failure_count: i64,
        overload_failure_count: i64,
        invalid_request_failure_count: i64,
        other_failure_count: i64,
        total_elapsed_time: i64,
        latest_failure_at: chrono::DateTime<chrono::FixedOffset>,
    }

    let mut grouped: HashMap<String, FailureAggregate> = HashMap::new();

    for item in logs {
        let key = match group_by {
            "account" => item.account_name.clone(),
            "model" => item.model_name.clone(),
            "endpoint" => item.endpoint.clone(),
            _ => item.channel_name.clone(),
        };
        let aggregate = grouped.entry(key).or_insert_with(|| FailureAggregate {
            failed_request_count: 0,
            stream_failure_count: 0,
            auth_failure_count: 0,
            rate_limit_failure_count: 0,
            overload_failure_count: 0,
            invalid_request_failure_count: 0,
            other_failure_count: 0,
            total_elapsed_time: 0,
            latest_failure_at: item.create_time,
        });

        aggregate.failed_request_count += 1;
        aggregate.total_elapsed_time += item.elapsed_time as i64;
        aggregate.latest_failure_at = aggregate.latest_failure_at.max(item.create_time);
        if item.is_stream {
            aggregate.stream_failure_count += 1;
        }

        match item.status_code {
            401 | 403 => aggregate.auth_failure_count += 1,
            429 => aggregate.rate_limit_failure_count += 1,
            400 | 404 | 413 | 422 => aggregate.invalid_request_failure_count += 1,
            500..=599 => aggregate.overload_failure_count += 1,
            _ => aggregate.other_failure_count += 1,
        }
    }

    let mut items: Vec<_> = grouped
        .into_iter()
        .map(|(group_key, aggregate)| FailureHotspotVo {
            group_key,
            failed_request_count: aggregate.failed_request_count,
            stream_failure_count: aggregate.stream_failure_count,
            auth_failure_count: aggregate.auth_failure_count,
            rate_limit_failure_count: aggregate.rate_limit_failure_count,
            overload_failure_count: aggregate.overload_failure_count,
            invalid_request_failure_count: aggregate.invalid_request_failure_count,
            other_failure_count: aggregate.other_failure_count,
            avg_elapsed_time: if aggregate.failed_request_count == 0 {
                0.0
            } else {
                aggregate.total_elapsed_time as f64 / aggregate.failed_request_count as f64
            },
            latest_failure_at: aggregate.latest_failure_at,
        })
        .collect();

    items.sort_by(|left, right| {
        right
            .failed_request_count
            .cmp(&left.failed_request_count)
            .then_with(|| right.latest_failure_at.cmp(&left.latest_failure_at))
            .then_with(|| left.group_key.cmp(&right.group_key))
    });
    items.truncate(limit as usize);
    items
}

fn build_usage_log_dto(
    token_info: &TokenInfo,
    channel: &SelectedChannel,
    usage: &Usage,
    record: AiUsageLogRecord,
) -> CreateLogDto {
    CreateLogDto {
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
        status_code: record.status_code,
        client_ip: record.client_ip,
        user_agent: record.user_agent,
        content: record.content,
        log_type: LogType::Consume,
        status: record.status,
    }
}

fn build_failure_log_dto(
    token_info: &TokenInfo,
    channel: &SelectedChannel,
    record: AiFailureLogRecord,
) -> CreateLogDto {
    build_usage_log_dto(
        token_info,
        channel,
        &Usage::default(),
        AiUsageLogRecord {
            endpoint: record.endpoint,
            request_format: record.request_format,
            request_id: record.request_id,
            upstream_request_id: record.upstream_request_id,
            requested_model: record.requested_model,
            upstream_model: record.upstream_model,
            model_name: record.model_name,
            quota: 0,
            elapsed_time: record.elapsed_time,
            first_token_time: 0,
            is_stream: record.is_stream,
            client_ip: record.client_ip,
            user_agent: record.user_agent,
            status_code: record.status_code,
            content: record.content,
            status: LogStatus::Failed,
        },
    )
}

#[cfg(test)]
mod tests;
