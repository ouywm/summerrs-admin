use std::collections::{HashMap, HashSet};

use anyhow::Context;
use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder};
use summer::plugin::Service;
use summer_common::error::{ApiErrors, ApiResult};
use summer_sea_orm::DbConn;

use summer_ai_model::entity::ability;
use summer_ai_model::entity::channel::{self, ChannelStatus};
use summer_ai_model::entity::channel_account::{self, AccountStatus};
use summer_ai_model::entity::log::{self, LogStatus};
use summer_ai_model::entity::token;
use summer_ai_model::vo::runtime::{
    AiRuntimeAccountVo, AiRuntimeChannelHealthVo, AiRuntimeProviderSummaryVo,
    AiRuntimeRouteCandidateVo, AiRuntimeRouteVo, AiRuntimeSummaryVo,
};

use crate::service::channel::ChannelService;
use crate::service::route_health::{RouteHealthService, RouteHealthSnapshot};
use crate::service::runtime_ops::{RuntimeOperationalSummary, RuntimeOpsService};

#[derive(Clone, Service)]
pub struct RuntimeService {
    #[inject(component)]
    db: DbConn,
    #[inject(component)]
    ops: RuntimeOpsService,
    #[inject(component)]
    route_health: RouteHealthService,
}

impl RuntimeService {
    pub fn new(db: DbConn, ops: RuntimeOpsService, route_health: RouteHealthService) -> Self {
        Self {
            db,
            ops,
            route_health,
        }
    }

    pub async fn health(&self) -> ApiResult<Vec<AiRuntimeChannelHealthVo>> {
        let channels = channel::Entity::find()
            .filter(channel::Column::DeletedAt.is_null())
            .order_by_desc(channel::Column::Priority)
            .order_by_desc(channel::Column::Id)
            .all(&self.db)
            .await
            .context("failed to query AI runtime channels")
            .map_err(ApiErrors::Internal)?;
        let accounts = channel_account::Entity::find()
            .filter(channel_account::Column::DeletedAt.is_null())
            .order_by_desc(channel_account::Column::Priority)
            .order_by_desc(channel_account::Column::Id)
            .all(&self.db)
            .await
            .context("failed to query AI runtime channel accounts")
            .map_err(ApiErrors::Internal)?;
        let channel_ids: Vec<i64> = channels.iter().map(|item| item.id).collect();
        let account_ids: Vec<i64> = accounts.iter().map(|item| item.id).collect();
        let channel_health = self.load_channel_health_map(channel_ids).await?;
        let account_health = self.load_account_health_map(account_ids).await?;

        Ok(build_runtime_health_items(
            channels,
            accounts,
            chrono::Utc::now().fixed_offset(),
            &channel_health,
            &account_health,
        ))
    }

    pub async fn routes(&self) -> ApiResult<Vec<AiRuntimeRouteVo>> {
        let abilities = ability::Entity::find()
            .filter(ability::Column::Enabled.eq(true))
            .order_by_asc(ability::Column::ChannelGroup)
            .order_by_asc(ability::Column::Model)
            .order_by_asc(ability::Column::EndpointScope)
            .order_by_desc(ability::Column::Priority)
            .order_by_desc(ability::Column::Weight)
            .order_by_asc(ability::Column::ChannelId)
            .all(&self.db)
            .await
            .context("failed to query AI runtime abilities")
            .map_err(ApiErrors::Internal)?;
        let channels = channel::Entity::find()
            .filter(channel::Column::DeletedAt.is_null())
            .all(&self.db)
            .await
            .context("failed to query AI runtime channels")
            .map_err(ApiErrors::Internal)?;
        let accounts = channel_account::Entity::find()
            .filter(channel_account::Column::DeletedAt.is_null())
            .all(&self.db)
            .await
            .context("failed to query AI runtime channel accounts")
            .map_err(ApiErrors::Internal)?;
        let channel_ids: Vec<i64> = channels.iter().map(|item| item.id).collect();
        let account_ids: Vec<i64> = accounts.iter().map(|item| item.id).collect();
        let channel_health = self.load_channel_health_map(channel_ids).await?;
        let account_health = self.load_account_health_map(account_ids).await?;

        Ok(build_runtime_route_items(
            abilities,
            channels,
            accounts,
            chrono::Utc::now().fixed_offset(),
            &channel_health,
            &account_health,
        ))
    }

    pub async fn summary(&self) -> ApiResult<AiRuntimeSummaryVo> {
        let now = chrono::Utc::now().fixed_offset();
        let window_start = now - chrono::Duration::hours(24);
        let channels = channel::Entity::find()
            .filter(channel::Column::DeletedAt.is_null())
            .all(&self.db)
            .await
            .context("failed to query AI runtime channels")
            .map_err(ApiErrors::Internal)?;
        let accounts = channel_account::Entity::find()
            .filter(channel_account::Column::DeletedAt.is_null())
            .all(&self.db)
            .await
            .context("failed to query AI runtime channel accounts")
            .map_err(ApiErrors::Internal)?;
        let logs = log::Entity::find()
            .filter(log::Column::CreateTime.gte(window_start))
            .filter(log::Column::CreateTime.lte(now))
            .all(&self.db)
            .await
            .context("failed to query AI runtime logs")
            .map_err(ApiErrors::Internal)?;
        let token_count = token::Entity::find()
            .count(&self.db)
            .await
            .context("failed to query AI tokens")
            .map_err(ApiErrors::Internal)? as i64;
        let operational_summary = self.ops.summary(window_start, now).await?;

        Ok(build_runtime_summary(
            channels,
            accounts,
            logs,
            token_count,
            window_start,
            now,
            operational_summary,
        ))
    }

    async fn load_channel_health_map<I>(
        &self,
        channel_ids: I,
    ) -> ApiResult<HashMap<i64, RouteHealthSnapshot>>
    where
        I: IntoIterator<Item = i64>,
    {
        let mut result = HashMap::new();
        for channel_id in channel_ids {
            result.insert(
                channel_id,
                self.route_health.load_channel_snapshot(channel_id).await?,
            );
        }
        Ok(result)
    }

    async fn load_account_health_map<I>(
        &self,
        account_ids: I,
    ) -> ApiResult<HashMap<i64, RouteHealthSnapshot>>
    where
        I: IntoIterator<Item = i64>,
    {
        let mut result = HashMap::new();
        for account_id in account_ids {
            result.insert(
                account_id,
                self.route_health.load_account_snapshot(account_id).await?,
            );
        }
        Ok(result)
    }
}

fn build_runtime_health_items(
    channels: Vec<channel::Model>,
    accounts: Vec<channel_account::Model>,
    now: chrono::DateTime<chrono::FixedOffset>,
    channel_health: &HashMap<i64, RouteHealthSnapshot>,
    account_health: &HashMap<i64, RouteHealthSnapshot>,
) -> Vec<AiRuntimeChannelHealthVo> {
    let accounts_by_channel = group_accounts_by_channel(accounts, now, account_health);

    channels
        .into_iter()
        .map(|channel| {
            let account_items = accounts_by_channel
                .get(&channel.id)
                .cloned()
                .unwrap_or_default();
            let available_account_count =
                account_items.iter().filter(|item| item.available).count();

            AiRuntimeChannelHealthVo {
                id: channel.id,
                name: channel.name,
                channel_type: channel.channel_type,
                channel_group: channel.channel_group,
                status: channel.status,
                priority: channel.priority,
                weight: channel.weight,
                auto_ban: channel.auto_ban,
                response_time: channel.response_time,
                failure_streak: channel.failure_streak,
                last_health_status: channel.last_health_status,
                recent_penalty_count: channel_health
                    .get(&channel.id)
                    .map(|snapshot| snapshot.recent_penalty_count)
                    .unwrap_or_default(),
                recent_rate_limit_count: channel_health
                    .get(&channel.id)
                    .map(|snapshot| snapshot.recent_rate_limit_count)
                    .unwrap_or_default(),
                recent_overload_count: channel_health
                    .get(&channel.id)
                    .map(|snapshot| snapshot.recent_overload_count)
                    .unwrap_or_default(),
                available: channel.status == ChannelStatus::Enabled && available_account_count > 0,
                available_account_count,
                last_used_at: channel.last_used_at,
                last_error_at: channel.last_error_at,
                last_error_code: channel.last_error_code,
                last_error_message: channel.last_error_message,
                accounts: account_items,
            }
        })
        .collect()
}

fn build_runtime_route_items(
    abilities: Vec<ability::Model>,
    channels: Vec<channel::Model>,
    accounts: Vec<channel_account::Model>,
    now: chrono::DateTime<chrono::FixedOffset>,
    channel_health: &HashMap<i64, RouteHealthSnapshot>,
    account_health: &HashMap<i64, RouteHealthSnapshot>,
) -> Vec<AiRuntimeRouteVo> {
    let channel_map: HashMap<i64, channel::Model> =
        channels.into_iter().map(|item| (item.id, item)).collect();
    let accounts_by_channel = group_accounts_by_channel(accounts, now, account_health);
    let mut grouped: HashMap<(String, String, String), Vec<AiRuntimeRouteCandidateVo>> =
        HashMap::new();

    for ability in abilities {
        let Some(channel) = channel_map.get(&ability.channel_id) else {
            continue;
        };
        let account_items = accounts_by_channel
            .get(&ability.channel_id)
            .cloned()
            .unwrap_or_default();
        let available_account_count = account_items.iter().filter(|item| item.available).count();
        grouped
            .entry((
                ability.channel_group.clone(),
                ability.model.clone(),
                ability.endpoint_scope.clone(),
            ))
            .or_default()
            .push(AiRuntimeRouteCandidateVo {
                channel_id: channel.id,
                channel_name: channel.name.clone(),
                channel_type: channel.channel_type,
                channel_status: channel.status,
                route_priority: ability.priority,
                route_weight: ability.weight,
                failure_streak: channel.failure_streak,
                last_health_status: channel.last_health_status,
                recent_penalty_count: channel_health
                    .get(&ability.channel_id)
                    .map(|snapshot| snapshot.recent_penalty_count)
                    .unwrap_or_default(),
                recent_rate_limit_count: channel_health
                    .get(&ability.channel_id)
                    .map(|snapshot| snapshot.recent_rate_limit_count)
                    .unwrap_or_default(),
                recent_overload_count: channel_health
                    .get(&ability.channel_id)
                    .map(|snapshot| snapshot.recent_overload_count)
                    .unwrap_or_default(),
                available: channel.status == ChannelStatus::Enabled && available_account_count > 0,
                available_account_count,
                accounts: account_items,
            });
    }

    let mut items: Vec<AiRuntimeRouteVo> = grouped
        .into_iter()
        .map(|((channel_group, model, endpoint_scope), mut candidates)| {
            candidates.sort_by(|left, right| {
                right
                    .route_priority
                    .cmp(&left.route_priority)
                    .then_with(|| right.route_weight.cmp(&left.route_weight))
                    .then_with(|| left.channel_id.cmp(&right.channel_id))
            });
            AiRuntimeRouteVo {
                channel_group,
                model,
                endpoint_scope,
                candidates,
            }
        })
        .collect();
    items.sort_by(|left, right| {
        left.channel_group
            .cmp(&right.channel_group)
            .then_with(|| left.model.cmp(&right.model))
            .then_with(|| left.endpoint_scope.cmp(&right.endpoint_scope))
    });
    items
}

fn build_runtime_summary(
    channels: Vec<channel::Model>,
    accounts: Vec<channel_account::Model>,
    logs: Vec<log::Model>,
    token_count: i64,
    window_start: chrono::DateTime<chrono::FixedOffset>,
    now: chrono::DateTime<chrono::FixedOffset>,
    operational_summary: RuntimeOperationalSummary,
) -> AiRuntimeSummaryVo {
    #[derive(Default)]
    struct ProviderAggregate {
        channel_count: i64,
        available_channel_count: i64,
        auto_disabled_channel_count: i64,
        account_count: i64,
        available_account_count: i64,
        rate_limited_account_count: i64,
        overloaded_account_count: i64,
        recent_request_count: i64,
        recent_success_request_count: i64,
        recent_failed_request_count: i64,
        recent_auth_failure_count: i64,
        recent_rate_limit_hit_count: i64,
        recent_overload_failure_count: i64,
    }

    let mut provider_aggregates: HashMap<i16, ProviderAggregate> = HashMap::new();
    let channel_type_by_id: HashMap<i64, channel::ChannelType> = channels
        .iter()
        .map(|item| (item.id, item.channel_type))
        .collect();
    let mut available_accounts_by_channel: HashMap<i64, i64> = HashMap::new();

    let total_account_count = accounts.len() as i64;
    let mut available_account_count = 0_i64;
    let mut rate_limited_account_count = 0_i64;
    let mut overloaded_account_count = 0_i64;
    let mut disabled_account_count = 0_i64;

    for account in &accounts {
        let channel_type = channel_type_by_id
            .get(&account.channel_id)
            .copied()
            .unwrap_or(channel::ChannelType::OpenAi);
        let provider = provider_aggregates.entry(channel_type as i16).or_default();
        provider.account_count += 1;

        if account_is_available(account, now) {
            available_account_count += 1;
            provider.available_account_count += 1;
            *available_accounts_by_channel
                .entry(account.channel_id)
                .or_default() += 1;
        }
        if account
            .rate_limited_until
            .is_some_and(|recover_at| recover_at > now)
        {
            rate_limited_account_count += 1;
            provider.rate_limited_account_count += 1;
        }
        if account
            .overload_until
            .is_some_and(|recover_at| recover_at > now)
        {
            overloaded_account_count += 1;
            provider.overloaded_account_count += 1;
        }
        if account.status == AccountStatus::Disabled {
            disabled_account_count += 1;
        }
    }

    let total_channel_count = channels.len() as i64;
    let mut available_channel_count = 0_i64;
    let mut auto_disabled_channel_count = 0_i64;

    for channel in &channels {
        let provider = provider_aggregates
            .entry(channel.channel_type as i16)
            .or_default();
        provider.channel_count += 1;
        if channel.status == ChannelStatus::AutoDisabled {
            auto_disabled_channel_count += 1;
            provider.auto_disabled_channel_count += 1;
        }
        if channel.status == ChannelStatus::Enabled
            && available_accounts_by_channel
                .get(&channel.id)
                .copied()
                .unwrap_or_default()
                > 0
        {
            available_channel_count += 1;
            provider.available_channel_count += 1;
        }
    }

    let mut recent_active_tokens = HashSet::new();
    let mut recent_request_count = 0_i64;
    let mut recent_success_request_count = 0_i64;
    let mut recent_failed_request_count = 0_i64;
    let mut recent_auth_failure_count = 0_i64;
    let mut recent_rate_limit_hit_count = 0_i64;
    let mut recent_overload_failure_count = 0_i64;

    for log in logs {
        recent_request_count += 1;
        recent_active_tokens.insert(log.token_id);
        let channel_type = channel_type_by_id
            .get(&log.channel_id)
            .copied()
            .unwrap_or(channel::ChannelType::OpenAi);
        let provider = provider_aggregates.entry(channel_type as i16).or_default();
        provider.recent_request_count += 1;

        if log.status == LogStatus::Success {
            recent_success_request_count += 1;
            provider.recent_success_request_count += 1;
            continue;
        }

        recent_failed_request_count += 1;
        provider.recent_failed_request_count += 1;
        match log.status_code {
            401 | 403 => {
                recent_auth_failure_count += 1;
                provider.recent_auth_failure_count += 1;
            }
            429 => {
                recent_rate_limit_hit_count += 1;
                provider.recent_rate_limit_hit_count += 1;
            }
            500..=599 => {
                recent_overload_failure_count += 1;
                provider.recent_overload_failure_count += 1;
            }
            _ => {}
        }
    }

    let mut provider_summaries: Vec<_> = provider_aggregates
        .into_iter()
        .map(|(channel_type_key, aggregate)| AiRuntimeProviderSummaryVo {
            channel_type: match channel_type_key {
                3 => channel::ChannelType::Anthropic,
                14 => channel::ChannelType::Azure,
                15 => channel::ChannelType::Baidu,
                17 => channel::ChannelType::Ali,
                24 => channel::ChannelType::Gemini,
                28 => channel::ChannelType::Ollama,
                _ => channel::ChannelType::OpenAi,
            },
            channel_count: aggregate.channel_count,
            available_channel_count: aggregate.available_channel_count,
            auto_disabled_channel_count: aggregate.auto_disabled_channel_count,
            account_count: aggregate.account_count,
            available_account_count: aggregate.available_account_count,
            rate_limited_account_count: aggregate.rate_limited_account_count,
            overloaded_account_count: aggregate.overloaded_account_count,
            recent_request_count: aggregate.recent_request_count,
            recent_success_request_count: aggregate.recent_success_request_count,
            recent_failed_request_count: aggregate.recent_failed_request_count,
            recent_auth_failure_count: aggregate.recent_auth_failure_count,
            recent_rate_limit_hit_count: aggregate.recent_rate_limit_hit_count,
            recent_overload_failure_count: aggregate.recent_overload_failure_count,
        })
        .collect();
    provider_summaries.sort_by_key(|item| item.channel_type as i16);

    AiRuntimeSummaryVo {
        generated_at: now,
        window_start,
        window_end: now,
        total_channel_count,
        available_channel_count,
        auto_disabled_channel_count,
        total_account_count,
        available_account_count,
        rate_limited_account_count,
        overloaded_account_count,
        disabled_account_count,
        total_token_count: token_count,
        recent_active_token_count: recent_active_tokens.len() as i64,
        recent_request_count,
        recent_success_request_count,
        recent_failed_request_count,
        recent_auth_failure_count,
        recent_rate_limit_hit_count,
        recent_overload_failure_count,
        recent_retry_count: operational_summary.total.retry_count,
        recent_fallback_count: operational_summary.total.fallback_count,
        recent_refund_count: operational_summary.total.refund_count,
        recent_settlement_failure_count: operational_summary.total.settlement_failure_count,
        provider_summaries,
    }
}

fn group_accounts_by_channel(
    accounts: Vec<channel_account::Model>,
    now: chrono::DateTime<chrono::FixedOffset>,
    account_health: &HashMap<i64, RouteHealthSnapshot>,
) -> HashMap<i64, Vec<AiRuntimeAccountVo>> {
    let mut grouped: HashMap<i64, Vec<AiRuntimeAccountVo>> = HashMap::new();
    for account in accounts {
        grouped
            .entry(account.channel_id)
            .or_default()
            .push(map_runtime_account(account, now, account_health));
    }

    for items in grouped.values_mut() {
        items.sort_by(|left, right| {
            right
                .priority
                .cmp(&left.priority)
                .then_with(|| right.id.cmp(&left.id))
        });
    }

    grouped
}

fn map_runtime_account(
    account: channel_account::Model,
    now: chrono::DateTime<chrono::FixedOffset>,
    account_health: &HashMap<i64, RouteHealthSnapshot>,
) -> AiRuntimeAccountVo {
    let available = account_is_available(&account, now);
    let route_health = account_health.get(&account.id).cloned().unwrap_or_default();
    AiRuntimeAccountVo {
        id: account.id,
        name: account.name,
        status: account.status,
        schedulable: account.schedulable,
        priority: account.priority,
        weight: account.weight,
        response_time: account.response_time,
        failure_streak: account.failure_streak,
        recent_penalty_count: route_health.recent_penalty_count,
        recent_rate_limit_count: route_health.recent_rate_limit_count,
        recent_overload_count: route_health.recent_overload_count,
        available,
        last_used_at: account.last_used_at,
        last_error_at: account.last_error_at,
        last_error_code: account.last_error_code,
        last_error_message: account.last_error_message,
        rate_limited_until: account.rate_limited_until,
        overload_until: account.overload_until,
        expires_at: account.expires_at,
    }
}

fn account_is_available(
    account: &channel_account::Model,
    now: chrono::DateTime<chrono::FixedOffset>,
) -> bool {
    account.status == AccountStatus::Enabled
        && account.schedulable
        && account.deleted_at.is_none()
        && !ChannelService::extract_api_key(&account.credentials).is_empty()
        && account.expires_at.is_none_or(|expires_at| expires_at > now)
        && account
            .rate_limited_until
            .is_none_or(|recover_at| recover_at <= now)
        && account
            .overload_until
            .is_none_or(|recover_at| recover_at <= now)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use sea_orm::prelude::BigDecimal;

    use super::{
        account_is_available, build_runtime_health_items, build_runtime_route_items,
        build_runtime_summary,
    };
    use crate::service::route_health::RouteHealthSnapshot;
    use crate::service::runtime_ops::{RuntimeOperationalCounts, RuntimeOperationalSummary};
    use summer_ai_model::entity::ability;
    use summer_ai_model::entity::channel::{self, ChannelStatus, ChannelType};
    use summer_ai_model::entity::channel_account::{self, AccountStatus};
    use summer_ai_model::entity::log::{self, LogStatus, LogType};

    #[test]
    fn account_is_available_requires_enabled_schedulable_unexpired_key_and_no_cooldown() {
        let now = chrono::Utc::now().fixed_offset();
        let mut account = make_account(10, 1, 10, 5);
        assert!(account_is_available(&account, now));

        account.rate_limited_until = Some(now + chrono::Duration::seconds(10));
        assert!(!account_is_available(&account, now));

        account.rate_limited_until = None;
        account.overload_until = Some(now + chrono::Duration::seconds(10));
        assert!(!account_is_available(&account, now));

        account.overload_until = None;
        account.expires_at = Some(now - chrono::Duration::seconds(1));
        assert!(!account_is_available(&account, now));

        account.expires_at = None;
        account.credentials = serde_json::json!({});
        assert!(!account_is_available(&account, now));
    }

    #[test]
    fn build_runtime_health_items_groups_accounts_and_counts_available_ones() {
        let now = chrono::Utc::now().fixed_offset();
        let channels = vec![
            make_channel(1, "primary", 100),
            make_channel(2, "secondary", 10),
        ];
        let mut limited = make_account(20, 1, 20, 2);
        limited.rate_limited_until = Some(now + chrono::Duration::seconds(30));
        let mut channel_health = HashMap::new();
        channel_health.insert(
            1,
            RouteHealthSnapshot {
                recent_penalty_count: 2,
                recent_rate_limit_count: 1,
                recent_overload_count: 0,
            },
        );
        let mut account_health = HashMap::new();
        account_health.insert(
            20,
            RouteHealthSnapshot {
                recent_penalty_count: 1,
                recent_rate_limit_count: 1,
                recent_overload_count: 0,
            },
        );

        let health = build_runtime_health_items(
            channels,
            vec![
                make_account(10, 1, 10, 5),
                limited,
                make_account(30, 2, 5, 1),
            ],
            now,
            &channel_health,
            &account_health,
        );

        assert_eq!(health.len(), 2);
        assert_eq!(health[0].id, 1);
        assert!(health[0].available);
        assert_eq!(health[0].available_account_count, 1);
        assert_eq!(health[0].accounts.len(), 2);
        assert_eq!(health[0].accounts[0].id, 20);
        assert!(!health[0].accounts[0].available);
        assert_eq!(health[0].recent_penalty_count, 2);
        assert_eq!(health[0].recent_rate_limit_count, 1);
        assert_eq!(health[0].accounts[0].recent_penalty_count, 1);
        assert_eq!(health[0].accounts[1].id, 10);
        assert!(health[0].accounts[1].available);

        assert_eq!(health[1].id, 2);
        assert!(health[1].available);
        assert_eq!(health[1].available_account_count, 1);
    }

    #[test]
    fn build_runtime_route_items_groups_by_model_scope_and_orders_candidates() {
        let now = chrono::Utc::now().fixed_offset();
        let mut cooling = make_account(31, 2, 10, 5);
        cooling.overload_until = Some(now + chrono::Duration::seconds(30));
        let mut channel_health = HashMap::new();
        channel_health.insert(
            1,
            RouteHealthSnapshot {
                recent_penalty_count: 1,
                recent_rate_limit_count: 0,
                recent_overload_count: 0,
            },
        );
        let mut account_health = HashMap::new();
        account_health.insert(
            11,
            RouteHealthSnapshot {
                recent_penalty_count: 1,
                recent_rate_limit_count: 0,
                recent_overload_count: 0,
            },
        );

        let routes = build_runtime_route_items(
            vec![
                make_ability(1, "default", "gpt-5.4", "chat", 1, 100, 20),
                make_ability(2, "default", "gpt-5.4", "chat", 2, 50, 10),
                make_ability(3, "default", "text-embedding-004", "embeddings", 2, 60, 5),
            ],
            vec![make_channel(1, "alpha", 100), make_channel(2, "beta", 80)],
            vec![make_account(11, 1, 9, 1), cooling],
            now,
            &channel_health,
            &account_health,
        );

        assert_eq!(routes.len(), 2);
        assert_eq!(routes[0].model, "gpt-5.4");
        assert_eq!(routes[0].endpoint_scope, "chat");
        assert_eq!(routes[0].candidates.len(), 2);
        assert_eq!(routes[0].candidates[0].channel_id, 1);
        assert!(routes[0].candidates[0].available);
        assert_eq!(routes[0].candidates[0].recent_penalty_count, 1);
        assert_eq!(routes[0].candidates[0].accounts[0].recent_penalty_count, 1);
        assert_eq!(routes[0].candidates[1].channel_id, 2);
        assert!(!routes[0].candidates[1].available);
        assert_eq!(routes[0].candidates[1].available_account_count, 0);

        assert_eq!(routes[1].model, "text-embedding-004");
        assert_eq!(routes[1].endpoint_scope, "embeddings");
        assert_eq!(routes[1].candidates.len(), 1);
        assert_eq!(routes[1].candidates[0].channel_id, 2);
    }

    #[test]
    fn build_runtime_summary_aggregates_recent_metrics_and_provider_health() {
        let now = chrono::Utc::now().fixed_offset();
        let mut openai_channel = make_channel(1, "openai-primary", 100);
        openai_channel.channel_type = ChannelType::OpenAi;
        let mut gemini_channel = make_channel(2, "gemini-secondary", 80);
        gemini_channel.channel_type = ChannelType::Gemini;
        gemini_channel.status = ChannelStatus::AutoDisabled;

        let mut openai_rate_limited = make_account(12, 1, 8, 1);
        openai_rate_limited.rate_limited_until = Some(now + chrono::Duration::minutes(5));
        let mut gemini_overloaded = make_account(21, 2, 7, 1);
        gemini_overloaded.overload_until = Some(now + chrono::Duration::minutes(5));

        let summary = build_runtime_summary(
            vec![openai_channel, gemini_channel],
            vec![
                make_account(11, 1, 10, 1),
                openai_rate_limited,
                gemini_overloaded,
            ],
            vec![
                make_log(
                    1001,
                    1,
                    200,
                    LogStatus::Success,
                    now - chrono::Duration::minutes(20),
                ),
                make_log(
                    1001,
                    1,
                    429,
                    LogStatus::Failed,
                    now - chrono::Duration::minutes(10),
                ),
                make_log(
                    1002,
                    2,
                    401,
                    LogStatus::Failed,
                    now - chrono::Duration::minutes(5),
                ),
            ],
            5,
            now - chrono::Duration::hours(24),
            now,
            RuntimeOperationalSummary {
                total: RuntimeOperationalCounts {
                    retry_count: 3,
                    fallback_count: 2,
                    refund_count: 1,
                    settlement_failure_count: 1,
                },
            },
        );

        assert_eq!(summary.total_channel_count, 2);
        assert_eq!(summary.available_channel_count, 1);
        assert_eq!(summary.auto_disabled_channel_count, 1);
        assert_eq!(summary.total_account_count, 3);
        assert_eq!(summary.available_account_count, 1);
        assert_eq!(summary.rate_limited_account_count, 1);
        assert_eq!(summary.overloaded_account_count, 1);
        assert_eq!(summary.disabled_account_count, 0);
        assert_eq!(summary.total_token_count, 5);
        assert_eq!(summary.recent_active_token_count, 2);
        assert_eq!(summary.recent_request_count, 3);
        assert_eq!(summary.recent_success_request_count, 1);
        assert_eq!(summary.recent_failed_request_count, 2);
        assert_eq!(summary.recent_rate_limit_hit_count, 1);
        assert_eq!(summary.recent_auth_failure_count, 1);
        assert_eq!(summary.recent_overload_failure_count, 0);
        assert_eq!(summary.recent_retry_count, 3);
        assert_eq!(summary.recent_fallback_count, 2);
        assert_eq!(summary.recent_refund_count, 1);
        assert_eq!(summary.recent_settlement_failure_count, 1);
        assert_eq!(summary.provider_summaries.len(), 2);
        assert_eq!(
            summary.provider_summaries[0].channel_type,
            ChannelType::OpenAi
        );
        assert_eq!(summary.provider_summaries[0].recent_request_count, 2);
        assert_eq!(summary.provider_summaries[0].recent_rate_limit_hit_count, 1);
        assert_eq!(summary.provider_summaries[0].available_channel_count, 1);
        assert_eq!(
            summary.provider_summaries[1].channel_type,
            ChannelType::Gemini
        );
        assert_eq!(summary.provider_summaries[1].auto_disabled_channel_count, 1);
        assert_eq!(summary.provider_summaries[1].recent_auth_failure_count, 1);
    }

    fn make_channel(id: i64, name: &str, priority: i32) -> channel::Model {
        let now = chrono::Utc::now().fixed_offset();
        channel::Model {
            id,
            name: name.into(),
            channel_type: ChannelType::OpenAi,
            vendor_code: "openai".into(),
            base_url: "https://example.com".into(),
            status: ChannelStatus::Enabled,
            models: serde_json::json!(["gpt-5.4"]),
            model_mapping: serde_json::json!({}),
            channel_group: "default".into(),
            endpoint_scopes: serde_json::json!(["chat", "embeddings"]),
            capabilities: serde_json::json!({}),
            weight: 10,
            priority,
            config: serde_json::json!({}),
            auto_ban: true,
            test_model: "gpt-5.4".into(),
            used_quota: 0,
            balance: BigDecimal::from(0),
            balance_updated_at: None,
            response_time: 120,
            success_rate: BigDecimal::from(1),
            failure_streak: 0,
            last_used_at: None,
            last_error_at: None,
            last_error_code: String::new(),
            last_error_message: None,
            last_health_status: 1,
            deleted_at: None,
            remark: String::new(),
            create_by: "test".into(),
            create_time: now,
            update_by: "test".into(),
            update_time: now,
        }
    }

    fn make_account(
        id: i64,
        channel_id: i64,
        priority: i32,
        weight: i32,
    ) -> channel_account::Model {
        let now = chrono::Utc::now().fixed_offset();
        channel_account::Model {
            id,
            channel_id,
            name: format!("account-{id}"),
            credential_type: "api_key".into(),
            credentials: serde_json::json!({ "api_key": format!("sk-{id}") }),
            secret_ref: String::new(),
            status: AccountStatus::Enabled,
            schedulable: true,
            priority,
            weight,
            rate_multiplier: BigDecimal::from(1),
            concurrency_limit: 0,
            quota_limit: BigDecimal::from(0),
            quota_used: BigDecimal::from(0),
            balance: BigDecimal::from(0),
            balance_updated_at: None,
            response_time: 80,
            failure_streak: 0,
            last_used_at: None,
            last_error_at: None,
            last_error_code: String::new(),
            last_error_message: None,
            rate_limited_until: None,
            overload_until: None,
            expires_at: Some(now + chrono::Duration::hours(1)),
            test_model: String::new(),
            test_time: None,
            extra: serde_json::json!({}),
            deleted_at: None,
            remark: String::new(),
            create_by: "test".into(),
            create_time: now,
            update_by: "test".into(),
            update_time: now,
        }
    }

    fn make_ability(
        id: i64,
        channel_group: &str,
        model: &str,
        endpoint_scope: &str,
        channel_id: i64,
        priority: i32,
        weight: i32,
    ) -> ability::Model {
        let now = chrono::Utc::now().fixed_offset();
        ability::Model {
            id,
            channel_group: channel_group.into(),
            endpoint_scope: endpoint_scope.into(),
            model: model.into(),
            channel_id,
            enabled: true,
            priority,
            weight,
            route_config: serde_json::json!({}),
            create_time: now,
            update_time: now,
        }
    }

    fn make_log(
        token_id: i64,
        channel_id: i64,
        status_code: i32,
        status: LogStatus,
        create_time: chrono::DateTime<chrono::FixedOffset>,
    ) -> log::Model {
        log::Model {
            id: token_id * 10 + channel_id,
            user_id: 1,
            token_id,
            token_name: format!("token-{token_id}"),
            project_id: 0,
            conversation_id: 0,
            message_id: 0,
            session_id: 0,
            thread_id: 0,
            trace_id: 0,
            channel_id,
            channel_name: format!("channel-{channel_id}"),
            account_id: channel_id * 10,
            account_name: format!("account-{channel_id}"),
            execution_id: 0,
            endpoint: "chat/completions".into(),
            request_format: "openai/chat_completions".into(),
            requested_model: "gpt-5.4".into(),
            upstream_model: "gpt-5.4".into(),
            model_name: "gpt-5.4".into(),
            prompt_tokens: 10,
            completion_tokens: 5,
            total_tokens: 15,
            cached_tokens: 0,
            reasoning_tokens: 0,
            quota: 15,
            cost_total: BigDecimal::from(0),
            price_reference: String::new(),
            elapsed_time: 120,
            first_token_time: 20,
            is_stream: true,
            request_id: format!("req-{token_id}-{channel_id}"),
            upstream_request_id: format!("up-{token_id}-{channel_id}"),
            status_code,
            client_ip: "127.0.0.1".into(),
            user_agent: "runtime-test".into(),
            content: String::new(),
            log_type: LogType::Consume,
            status,
            create_time,
        }
    }
}
