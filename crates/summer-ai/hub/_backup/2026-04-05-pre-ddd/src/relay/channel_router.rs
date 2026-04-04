use anyhow::Context;
use rand::RngExt;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder};
use serde::{Deserialize, Serialize};
use summer::plugin::Service;
use summer_sea_orm::DbConn;

use summer_ai_model::entity::ability;
use summer_ai_model::entity::channel::{self, ChannelStatus};
use summer_ai_model::entity::channel_account::{self, AccountStatus};
use summer_common::error::{ApiErrors, ApiResult};

use crate::service::route_health::{RouteHealthService, RouteHealthSnapshot};
use crate::service::runtime_cache::RuntimeCacheService;
use summer_ai_core::provider::provider_scope_allowlist;

const ROUTE_CACHE_TTL_SECONDS: u64 = 30;

#[derive(Debug, Clone)]
pub struct SelectedChannel {
    pub channel_id: i64,
    pub channel_name: String,
    pub channel_type: i16,
    pub base_url: String,
    pub model_mapping: serde_json::Value,
    pub api_key: String,
    pub account_id: i64,
    pub account_name: String,
}

#[derive(Debug, Clone, Default)]
pub struct RouteSelectionPlan {
    ordered: Vec<SelectedChannel>,
    exclusions: RouteSelectionExclusions,
    next_index: usize,
}

pub trait RouteSelectionState {
    fn exclude_selected_channel(&mut self, channel: &SelectedChannel);
    fn exclude_selected_account(&mut self, channel: &SelectedChannel);
}

impl RouteSelectionPlan {
    pub fn new(ordered: Vec<SelectedChannel>, exclusions: RouteSelectionExclusions) -> Self {
        Self {
            ordered,
            exclusions,
            next_index: 0,
        }
    }

    pub fn exclude_selected_channel(&mut self, channel: &SelectedChannel) {
        self.exclusions.exclude_selected_channel(channel);
    }

    pub fn exclude_selected_account(&mut self, channel: &SelectedChannel) {
        self.exclusions.exclude_selected_account(channel);
    }
}

impl Iterator for RouteSelectionPlan {
    type Item = SelectedChannel;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(selected) = self.ordered.get(self.next_index).cloned() {
            self.next_index += 1;
            if !self.exclusions.selected_is_excluded(&selected) {
                return Some(selected);
            }
        }
        None
    }
}

impl RouteSelectionState for RouteSelectionPlan {
    fn exclude_selected_channel(&mut self, channel: &SelectedChannel) {
        RouteSelectionPlan::exclude_selected_channel(self, channel);
    }

    fn exclude_selected_account(&mut self, channel: &SelectedChannel) {
        RouteSelectionPlan::exclude_selected_account(self, channel);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RouteSelectionExclusions {
    channel_ids: Vec<i64>,
    account_ids: Vec<i64>,
}

impl RouteSelectionExclusions {
    pub fn exclude_channel(&mut self, channel_id: i64) {
        if !self.channel_ids.contains(&channel_id) {
            self.channel_ids.push(channel_id);
        }
    }

    pub fn exclude_account(&mut self, account_id: i64) {
        if !self.account_ids.contains(&account_id) {
            self.account_ids.push(account_id);
        }
    }

    pub fn exclude_selected_channel(&mut self, channel: &SelectedChannel) {
        self.exclude_channel(channel.channel_id);
    }

    pub fn exclude_selected_account(&mut self, channel: &SelectedChannel) {
        self.exclude_account(channel.account_id);
    }

    pub fn selected_is_excluded(&self, channel: &SelectedChannel) -> bool {
        self.channel_is_excluded(channel.channel_id) || self.account_is_excluded(channel.account_id)
    }

    fn channel_is_excluded(&self, channel_id: i64) -> bool {
        self.channel_ids.contains(&channel_id)
    }

    fn account_is_excluded(&self, account_id: i64) -> bool {
        self.account_ids.contains(&account_id)
    }
}

impl RouteSelectionState for RouteSelectionExclusions {
    fn exclude_selected_channel(&mut self, channel: &SelectedChannel) {
        RouteSelectionExclusions::exclude_selected_channel(self, channel);
    }

    fn exclude_selected_account(&mut self, channel: &SelectedChannel) {
        RouteSelectionExclusions::exclude_selected_account(self, channel);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedRouteCandidate {
    channel_id: i64,
    channel_name: String,
    channel_type: i16,
    base_url: String,
    model_mapping: serde_json::Value,
    priority: i32,
    weight: i32,
    #[serde(default)]
    channel_failure_streak: i32,
    #[serde(default)]
    channel_response_time: i32,
    #[serde(default = "default_route_health_status")]
    last_health_status: i16,
    #[serde(default)]
    recent_penalty_count: i32,
    #[serde(default)]
    recent_rate_limit_count: i32,
    #[serde(default)]
    recent_overload_count: i32,
    accounts: Vec<CachedRouteAccount>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedRouteAccount {
    account_id: i64,
    account_name: String,
    weight: i32,
    priority: i32,
    #[serde(default)]
    failure_streak: i32,
    #[serde(default)]
    response_time: i32,
    #[serde(default)]
    recent_penalty_count: i32,
    #[serde(default)]
    recent_rate_limit_count: i32,
    #[serde(default)]
    recent_overload_count: i32,
    api_key: String,
}

struct LoadedSchedulableAccounts {
    grouped: std::collections::HashMap<i64, Vec<CachedRouteAccount>>,
    channel_health: std::collections::HashMap<i64, RouteHealthSnapshot>,
    ttl_seconds: u64,
}

#[derive(Clone, Service)]
pub struct ChannelRouter {
    #[inject(component)]
    db: DbConn,
    #[inject(component)]
    cache: RuntimeCacheService,
    #[inject(component)]
    route_health: RouteHealthService,
}

impl ChannelRouter {
    pub fn new(db: DbConn, cache: RuntimeCacheService, route_health: RouteHealthService) -> Self {
        Self {
            db,
            cache,
            route_health,
        }
    }

    pub async fn select_channel(
        &self,
        group: &str,
        model: &str,
        endpoint_scope: &str,
        exclude: &[i64],
    ) -> ApiResult<Option<SelectedChannel>> {
        let mut exclusions = RouteSelectionExclusions::default();
        for channel_id in exclude {
            exclusions.exclude_channel(*channel_id);
        }
        self.select_channel_with_exclusions(group, model, endpoint_scope, &exclusions)
            .await
    }

    pub async fn select_channel_with_exclusions(
        &self,
        group: &str,
        model: &str,
        endpoint_scope: &str,
        exclusions: &RouteSelectionExclusions,
    ) -> ApiResult<Option<SelectedChannel>> {
        Ok(self
            .build_channel_plan_with_exclusions(group, model, endpoint_scope, exclusions)
            .await?
            .next())
    }

    pub async fn build_channel_plan_with_exclusions(
        &self,
        group: &str,
        model: &str,
        endpoint_scope: &str,
        exclusions: &RouteSelectionExclusions,
    ) -> ApiResult<RouteSelectionPlan> {
        let candidates = self
            .load_cached_route_candidates(group, model, endpoint_scope)
            .await?;
        Ok(RouteSelectionPlan::new(
            build_route_plan_from_candidates(&candidates, exclusions),
            exclusions.clone(),
        ))
    }

    /// Build a channel plan using a custom routing strategy.
    pub async fn build_channel_plan_with_strategy(
        &self,
        group: &str,
        model: &str,
        endpoint_scope: &str,
        exclusions: &RouteSelectionExclusions,
        strategy: &dyn crate::relay::routing_strategy::RoutingStrategy,
    ) -> ApiResult<RouteSelectionPlan> {
        let candidates = self
            .load_cached_route_candidates(group, model, endpoint_scope)
            .await?;
        Ok(RouteSelectionPlan::new(
            build_route_plan_from_candidates_with_strategy(&candidates, exclusions, Some(strategy)),
            exclusions.clone(),
        ))
    }

    pub async fn select_default_channel(
        &self,
        group: &str,
        endpoint_scope: &str,
        exclude: &[i64],
    ) -> ApiResult<Option<SelectedChannel>> {
        let mut exclusions = RouteSelectionExclusions::default();
        for channel_id in exclude {
            exclusions.exclude_channel(*channel_id);
        }
        self.select_default_channel_with_exclusions(group, endpoint_scope, &exclusions)
            .await
    }

    pub async fn select_default_channel_with_exclusions(
        &self,
        group: &str,
        endpoint_scope: &str,
        exclusions: &RouteSelectionExclusions,
    ) -> ApiResult<Option<SelectedChannel>> {
        Ok(self
            .build_default_channel_plan_with_exclusions(group, endpoint_scope, exclusions)
            .await?
            .next())
    }

    pub async fn build_default_channel_plan_with_exclusions(
        &self,
        group: &str,
        endpoint_scope: &str,
        exclusions: &RouteSelectionExclusions,
    ) -> ApiResult<RouteSelectionPlan> {
        let candidates = self
            .load_cached_default_candidates(group, endpoint_scope)
            .await?;
        Ok(RouteSelectionPlan::new(
            build_route_plan_from_candidates(&candidates, exclusions),
            exclusions.clone(),
        ))
    }

    async fn load_cached_route_candidates(
        &self,
        group: &str,
        model: &str,
        endpoint_scope: &str,
    ) -> ApiResult<Vec<CachedRouteCandidate>> {
        let version = self
            .cache
            .get_i64(route_cache_version_key())
            .await?
            .unwrap_or(0);
        let cache_key = route_cache_key(version, group, endpoint_scope, model);

        if let Some(candidates) = self
            .cache
            .get_json::<Vec<CachedRouteCandidate>>(&cache_key)
            .await?
        {
            return Ok(candidates);
        }

        let abilities = ability::Entity::find()
            .filter(ability::Column::ChannelGroup.eq(group))
            .filter(ability::Column::Model.eq(model))
            .filter(ability::Column::EndpointScope.eq(endpoint_scope))
            .filter(ability::Column::Enabled.eq(true))
            .order_by_desc(ability::Column::Priority)
            .all(&self.db)
            .await
            .context("查询渠道路由失败")
            .map_err(ApiErrors::Internal)?;

        if abilities.is_empty() {
            return Ok(Vec::new());
        }

        let channel_ids: Vec<i64> = abilities.iter().map(|ability| ability.channel_id).collect();

        let channels = channel::Entity::find()
            .filter(channel::Column::Id.is_in(channel_ids.clone()))
            .filter(channel::Column::Status.eq(ChannelStatus::Enabled))
            .filter(channel::Column::DeletedAt.is_null())
            .all(&self.db)
            .await
            .context("查询渠道详情失败")
            .map_err(ApiErrors::Internal)?;
        let channel_map: std::collections::HashMap<i64, channel::Model> = channels
            .into_iter()
            .map(|channel| (channel.id, channel))
            .collect();

        let loaded_accounts = self.load_schedulable_accounts(channel_ids).await?;
        let candidates: Vec<CachedRouteCandidate> = abilities
            .into_iter()
            .filter_map(|ability| {
                let channel = channel_map.get(&ability.channel_id)?;
                let accounts = loaded_accounts.grouped.get(&ability.channel_id)?.clone();
                if accounts.is_empty() {
                    return None;
                }
                let route_health = loaded_accounts
                    .channel_health
                    .get(&ability.channel_id)
                    .cloned()
                    .unwrap_or_default();

                Some(CachedRouteCandidate {
                    channel_id: channel.id,
                    channel_name: channel.name.clone(),
                    channel_type: channel.channel_type as i16,
                    base_url: channel.base_url.clone(),
                    model_mapping: channel.model_mapping.clone(),
                    priority: ability.priority,
                    weight: ability.weight,
                    channel_failure_streak: channel.failure_streak,
                    channel_response_time: channel.response_time,
                    last_health_status: channel.last_health_status,
                    recent_penalty_count: route_health.recent_penalty_count,
                    recent_rate_limit_count: route_health.recent_rate_limit_count,
                    recent_overload_count: route_health.recent_overload_count,
                    accounts,
                })
            })
            .collect();

        let _ = self
            .cache
            .set_json(&cache_key, &candidates, loaded_accounts.ttl_seconds)
            .await;

        Ok(candidates)
    }

    async fn load_cached_default_candidates(
        &self,
        group: &str,
        endpoint_scope: &str,
    ) -> ApiResult<Vec<CachedRouteCandidate>> {
        let version = self
            .cache
            .get_i64(route_cache_version_key())
            .await?
            .unwrap_or(0);
        let cache_key = default_route_cache_key(version, group, endpoint_scope);

        if let Some(candidates) = self
            .cache
            .get_json::<Vec<CachedRouteCandidate>>(&cache_key)
            .await?
        {
            return Ok(candidates);
        }

        let abilities = ability::Entity::find()
            .filter(ability::Column::ChannelGroup.eq(group))
            .filter(ability::Column::EndpointScope.eq(endpoint_scope))
            .filter(ability::Column::Enabled.eq(true))
            .order_by_desc(ability::Column::Priority)
            .all(&self.db)
            .await
            .context("查询默认渠道路由失败")
            .map_err(ApiErrors::Internal)?;

        if abilities.is_empty() {
            let (candidates, ttl_seconds) = self
                .load_group_default_fallback_candidates(group, endpoint_scope)
                .await?;
            let _ = self
                .cache
                .set_json(&cache_key, &candidates, ttl_seconds)
                .await;
            return Ok(candidates);
        }

        let mut best_ability_by_channel: std::collections::HashMap<i64, ability::Model> =
            std::collections::HashMap::new();
        for item in abilities {
            best_ability_by_channel
                .entry(item.channel_id)
                .and_modify(|existing| {
                    if item.priority > existing.priority
                        || (item.priority == existing.priority && item.weight > existing.weight)
                    {
                        *existing = item.clone();
                    }
                })
                .or_insert(item);
        }

        let channel_ids: Vec<i64> = best_ability_by_channel.keys().copied().collect();
        let channels = channel::Entity::find()
            .filter(channel::Column::Id.is_in(channel_ids.clone()))
            .filter(channel::Column::Status.eq(ChannelStatus::Enabled))
            .filter(channel::Column::DeletedAt.is_null())
            .all(&self.db)
            .await
            .context("查询默认渠道详情失败")
            .map_err(ApiErrors::Internal)?;
        let channel_map: std::collections::HashMap<i64, channel::Model> = channels
            .into_iter()
            .map(|channel| (channel.id, channel))
            .collect();

        let loaded_accounts = self.load_schedulable_accounts(channel_ids).await?;
        let mut candidates: Vec<CachedRouteCandidate> = best_ability_by_channel
            .into_values()
            .filter_map(|ability| {
                let channel = channel_map.get(&ability.channel_id)?;
                let accounts = loaded_accounts.grouped.get(&ability.channel_id)?.clone();
                if accounts.is_empty() {
                    return None;
                }
                let route_health = loaded_accounts
                    .channel_health
                    .get(&ability.channel_id)
                    .cloned()
                    .unwrap_or_default();

                Some(CachedRouteCandidate {
                    channel_id: channel.id,
                    channel_name: channel.name.clone(),
                    channel_type: channel.channel_type as i16,
                    base_url: channel.base_url.clone(),
                    model_mapping: channel.model_mapping.clone(),
                    priority: ability.priority,
                    weight: ability.weight,
                    channel_failure_streak: channel.failure_streak,
                    channel_response_time: channel.response_time,
                    last_health_status: channel.last_health_status,
                    recent_penalty_count: route_health.recent_penalty_count,
                    recent_rate_limit_count: route_health.recent_rate_limit_count,
                    recent_overload_count: route_health.recent_overload_count,
                    accounts,
                })
            })
            .collect();
        candidates.sort_by(|left, right| {
            right
                .priority
                .cmp(&left.priority)
                .then_with(|| right.weight.cmp(&left.weight))
                .then_with(|| left.channel_id.cmp(&right.channel_id))
        });
        let (fallback_candidates, fallback_ttl_seconds) = self
            .load_group_default_fallback_candidates(group, endpoint_scope)
            .await?;
        let candidates = merge_default_route_candidates(candidates, fallback_candidates);
        let ttl_seconds = loaded_accounts.ttl_seconds.min(fallback_ttl_seconds);

        let _ = self
            .cache
            .set_json(&cache_key, &candidates, ttl_seconds)
            .await;

        Ok(candidates)
    }

    async fn load_group_default_fallback_candidates(
        &self,
        group: &str,
        endpoint_scope: &str,
    ) -> ApiResult<(Vec<CachedRouteCandidate>, u64)> {
        let channels = channel::Entity::find()
            .filter(channel::Column::ChannelGroup.eq(group))
            .filter(channel::Column::Status.eq(ChannelStatus::Enabled))
            .filter(channel::Column::DeletedAt.is_null())
            .order_by_desc(channel::Column::Priority)
            .all(&self.db)
            .await
            .context("查询默认回退渠道失败")
            .map_err(ApiErrors::Internal)?;

        let channel_ids: Vec<i64> = channels.iter().map(|channel| channel.id).collect();
        let loaded_accounts = self.load_schedulable_accounts(channel_ids).await?;

        let candidates = channels
            .into_iter()
            .filter(|channel| {
                channel_supports_endpoint_scope(
                    channel.channel_type as i16,
                    &channel.endpoint_scopes,
                    endpoint_scope,
                )
            })
            .filter_map(|channel| {
                let accounts = loaded_accounts.grouped.get(&channel.id)?.clone();
                if accounts.is_empty() {
                    return None;
                }
                let route_health = loaded_accounts
                    .channel_health
                    .get(&channel.id)
                    .cloned()
                    .unwrap_or_default();

                Some(CachedRouteCandidate {
                    channel_id: channel.id,
                    channel_name: channel.name,
                    channel_type: channel.channel_type as i16,
                    base_url: channel.base_url,
                    model_mapping: channel.model_mapping,
                    priority: channel.priority,
                    weight: channel.weight,
                    channel_failure_streak: channel.failure_streak,
                    channel_response_time: channel.response_time,
                    last_health_status: channel.last_health_status,
                    recent_penalty_count: route_health.recent_penalty_count,
                    recent_rate_limit_count: route_health.recent_rate_limit_count,
                    recent_overload_count: route_health.recent_overload_count,
                    accounts,
                })
            })
            .collect();

        Ok((candidates, loaded_accounts.ttl_seconds))
    }

    async fn load_schedulable_accounts(
        &self,
        channel_ids: Vec<i64>,
    ) -> ApiResult<LoadedSchedulableAccounts> {
        if channel_ids.is_empty() {
            return Ok(LoadedSchedulableAccounts {
                grouped: std::collections::HashMap::new(),
                channel_health: std::collections::HashMap::new(),
                ttl_seconds: ROUTE_CACHE_TTL_SECONDS,
            });
        }

        let now = chrono::Utc::now().fixed_offset();
        let accounts = channel_account::Entity::find()
            .filter(channel_account::Column::ChannelId.is_in(channel_ids))
            .filter(channel_account::Column::Status.eq(AccountStatus::Enabled))
            .filter(channel_account::Column::Schedulable.eq(true))
            .filter(channel_account::Column::DeletedAt.is_null())
            .order_by_desc(channel_account::Column::Priority)
            .order_by_desc(channel_account::Column::Id)
            .all(&self.db)
            .await
            .context("查询渠道账号失败")
            .map_err(ApiErrors::Internal)?;

        let mut grouped: std::collections::HashMap<i64, Vec<CachedRouteAccount>> =
            std::collections::HashMap::new();
        let mut channel_health = std::collections::HashMap::new();
        let mut next_refresh_at = None;

        // First pass: filter eligible accounts and collect IDs for batch health load.
        struct EligibleAccount {
            id: i64,
            channel_id: i64,
            name: String,
            weight: i32,
            priority: i32,
            failure_streak: i32,
            response_time: i32,
            api_key: String,
        }

        let mut eligible = Vec::new();
        for account in accounts {
            next_refresh_at =
                pick_earlier_route_refresh_at(next_refresh_at, account.expires_at, now);
            next_refresh_at =
                pick_earlier_route_refresh_at(next_refresh_at, account.rate_limited_until, now);
            next_refresh_at =
                pick_earlier_route_refresh_at(next_refresh_at, account.overload_until, now);

            if account
                .expires_at
                .is_some_and(|expires_at| expires_at <= now)
                || account
                    .rate_limited_until
                    .is_some_and(|recover_at| recover_at > now)
                || account
                    .overload_until
                    .is_some_and(|recover_at| recover_at > now)
            {
                continue;
            }

            let api_key =
                crate::service::channel::ChannelService::extract_api_key(&account.credentials);
            if api_key.is_empty() {
                continue;
            }

            eligible.push(EligibleAccount {
                id: account.id,
                channel_id: account.channel_id,
                name: account.name,
                weight: account.weight,
                priority: account.priority,
                failure_streak: account.failure_streak,
                response_time: account.response_time,
                api_key,
            });
        }

        // Batch-load all health snapshots in a single Redis pipeline.
        let account_ids: Vec<i64> = eligible.iter().map(|a| a.id).collect();
        let unique_channel_ids: Vec<i64> = {
            let mut ids: Vec<i64> = eligible.iter().map(|a| a.channel_id).collect();
            ids.sort_unstable();
            ids.dedup();
            ids
        };
        let batch_health = self
            .route_health
            .batch_load_snapshots(&account_ids, &unique_channel_ids)
            .await?;

        for ch_id in &unique_channel_ids {
            channel_health.insert(
                *ch_id,
                batch_health
                    .channels
                    .get(ch_id)
                    .cloned()
                    .unwrap_or_default(),
            );
        }

        for account in eligible {
            let account_health = batch_health
                .accounts
                .get(&account.id)
                .cloned()
                .unwrap_or_default();

            grouped
                .entry(account.channel_id)
                .or_default()
                .push(CachedRouteAccount {
                    account_id: account.id,
                    account_name: account.name,
                    weight: account.weight,
                    priority: account.priority,
                    failure_streak: account.failure_streak,
                    response_time: account.response_time,
                    recent_penalty_count: account_health.recent_penalty_count,
                    recent_rate_limit_count: account_health.recent_rate_limit_count,
                    recent_overload_count: account_health.recent_overload_count,
                    api_key: account.api_key,
                });
        }

        Ok(LoadedSchedulableAccounts {
            grouped,
            channel_health,
            ttl_seconds: compute_route_cache_ttl_seconds(now, next_refresh_at),
        })
    }
}

fn pick_earlier_route_refresh_at(
    current: Option<chrono::DateTime<chrono::FixedOffset>>,
    candidate: Option<chrono::DateTime<chrono::FixedOffset>>,
    now: chrono::DateTime<chrono::FixedOffset>,
) -> Option<chrono::DateTime<chrono::FixedOffset>> {
    let candidate = candidate.filter(|deadline| *deadline > now);
    match (current, candidate) {
        (Some(current), Some(candidate)) => Some(current.min(candidate)),
        (Some(current), None) => Some(current),
        (None, Some(candidate)) => Some(candidate),
        (None, None) => None,
    }
}

fn compute_route_cache_ttl_seconds(
    now: chrono::DateTime<chrono::FixedOffset>,
    next_refresh_at: Option<chrono::DateTime<chrono::FixedOffset>>,
) -> u64 {
    let Some(next_refresh_at) = next_refresh_at else {
        return ROUTE_CACHE_TTL_SECONDS;
    };

    let refresh_in_ms = (next_refresh_at - now).num_milliseconds().max(0);
    let refresh_in_seconds = ((refresh_in_ms + 999) / 1000) as u64;
    refresh_in_seconds.clamp(1, ROUTE_CACHE_TTL_SECONDS)
}

fn weighted_random_select<T: Copy>(items: &[(T, i32)]) -> Option<T> {
    let positive_items: Vec<(T, i32)> = items
        .iter()
        .copied()
        .filter(|(_, weight)| *weight > 0)
        .collect();
    if positive_items.is_empty() {
        return items.first().map(|(item, _)| *item);
    }

    let total: i64 = positive_items
        .iter()
        .map(|(_, weight)| i64::from(*weight))
        .sum();
    let mut pick = rand::rng().random_range(0..total);
    for (item, weight) in positive_items {
        let weight = i64::from(weight);
        if pick < weight {
            return Some(item);
        }
        pick -= weight;
    }

    None
}

#[cfg(test)]
fn select_from_route_candidates(
    candidates: &[CachedRouteCandidate],
    exclusions: &RouteSelectionExclusions,
) -> Option<SelectedChannel> {
    select_from_route_candidates_with_strategy(candidates, exclusions, None)
}

fn select_from_route_candidates_with_strategy(
    candidates: &[CachedRouteCandidate],
    exclusions: &RouteSelectionExclusions,
    strategy: Option<&dyn crate::relay::routing_strategy::RoutingStrategy>,
) -> Option<SelectedChannel> {
    let available_candidates: Vec<(&CachedRouteCandidate, Vec<&CachedRouteAccount>)> = candidates
        .iter()
        .filter_map(|candidate| {
            if exclusions.channel_is_excluded(candidate.channel_id) {
                return None;
            }

            let available_accounts: Vec<&CachedRouteAccount> = candidate
                .accounts
                .iter()
                .filter(|account| !exclusions.account_is_excluded(account.account_id))
                .collect();
            if available_accounts.is_empty() {
                return None;
            }

            Some((candidate, available_accounts))
        })
        .collect();

    // If a custom strategy is provided, flatten candidates into RouteCandidate
    // structs and delegate selection entirely to the strategy.
    if let Some(strategy) = strategy {
        use crate::relay::routing_strategy::{RouteCandidate, RoutingContext};

        let mut flat: Vec<(RouteCandidate, usize, usize)> = Vec::new();
        for (ci, (candidate, accounts)) in available_candidates.iter().enumerate() {
            for (ai, account) in accounts.iter().enumerate() {
                flat.push((
                    RouteCandidate {
                        channel_id: candidate.channel_id,
                        channel_name: candidate.channel_name.clone(),
                        channel_type: candidate.channel_type,
                        base_url: candidate.base_url.clone(),
                        model_mapping: candidate.model_mapping.clone(),
                        priority: effective_candidate_priority(candidate),
                        weight: candidate.weight,
                        response_time: candidate.channel_response_time,
                        failure_streak: candidate.channel_failure_streak,
                        recent_penalty_count: candidate.recent_penalty_count,
                        account_id: account.account_id,
                        account_name: account.account_name.clone(),
                        api_key: account.api_key.clone(),
                    },
                    ci,
                    ai,
                ));
            }
        }

        let route_candidates: Vec<RouteCandidate> =
            flat.iter().map(|(rc, _, _)| rc.clone()).collect();
        let ctx = RoutingContext {
            model: "",
            endpoint_scope: "",
            estimated_tokens: 0,
        };

        let selected_index = strategy.select(&route_candidates, &ctx)?;
        let (rc, _, _) = flat.get(selected_index)?;

        return Some(SelectedChannel {
            channel_id: rc.channel_id,
            channel_name: rc.channel_name.clone(),
            channel_type: rc.channel_type,
            base_url: rc.base_url.clone(),
            model_mapping: rc.model_mapping.clone(),
            api_key: rc.api_key.clone(),
            account_id: rc.account_id,
            account_name: rc.account_name.clone(),
        });
    }

    // Default strategy: priority → health → weighted random (original logic).
    let max_priority = available_candidates
        .iter()
        .map(|(candidate, _)| effective_candidate_priority(candidate))
        .max()?;
    let best_health = available_candidates
        .iter()
        .filter(|(candidate, _)| effective_candidate_priority(candidate) == max_priority)
        .map(|(candidate, accounts)| candidate_health_key(candidate, accounts))
        .min()?;
    let weighted_candidates: Vec<(usize, i32)> = available_candidates
        .iter()
        .enumerate()
        .filter(|(_, (candidate, _))| effective_candidate_priority(candidate) == max_priority)
        .filter(|(_, (candidate, accounts))| {
            candidate_health_key(candidate, accounts) == best_health
        })
        .map(|(index, (candidate, _))| (index, candidate.weight))
        .collect();
    let channel_index = weighted_random_select(&weighted_candidates)?;
    let (candidate, accounts) = available_candidates.get(channel_index)?;
    let account = pick_schedulable_account(accounts)?;

    Some(SelectedChannel {
        channel_id: candidate.channel_id,
        channel_name: candidate.channel_name.clone(),
        channel_type: candidate.channel_type,
        base_url: candidate.base_url.clone(),
        model_mapping: candidate.model_mapping.clone(),
        api_key: account.api_key.clone(),
        account_id: account.account_id,
        account_name: account.account_name.clone(),
    })
}

fn build_route_plan_from_candidates(
    candidates: &[CachedRouteCandidate],
    exclusions: &RouteSelectionExclusions,
) -> Vec<SelectedChannel> {
    build_route_plan_from_candidates_with_strategy(candidates, exclusions, None)
}

fn build_route_plan_from_candidates_with_strategy(
    candidates: &[CachedRouteCandidate],
    exclusions: &RouteSelectionExclusions,
    strategy: Option<&dyn crate::relay::routing_strategy::RoutingStrategy>,
) -> Vec<SelectedChannel> {
    let mut plan = Vec::new();
    let mut planning_exclusions = exclusions.clone();

    while let Some(selected) =
        select_from_route_candidates_with_strategy(candidates, &planning_exclusions, strategy)
    {
        planning_exclusions.exclude_selected_account(&selected);
        plan.push(selected);
    }

    plan
}

fn pick_schedulable_account<'a>(
    accounts: &'a [&'a CachedRouteAccount],
) -> Option<&'a CachedRouteAccount> {
    let max_priority = accounts
        .iter()
        .map(|account| effective_account_priority(account))
        .max()?;
    let best_health = accounts
        .iter()
        .filter(|account| effective_account_priority(account) == max_priority)
        .map(|account| account_health_key(account))
        .min()?;
    let weighted_candidates: Vec<(usize, i32)> = accounts
        .iter()
        .enumerate()
        .filter(|(_, account)| effective_account_priority(account) == max_priority)
        .filter(|(_, account)| account_health_key(account) == best_health)
        .map(|(index, account)| (index, account.weight))
        .collect();
    let index = weighted_random_select(&weighted_candidates)?;
    accounts.get(index).copied()
}

type AccountHealthKey = (i32, i32, i32, i32, i32);
type CandidateHealthKey = (i32, i32, i32, i32, i16, i32, AccountHealthKey);

fn effective_candidate_priority(candidate: &CachedRouteCandidate) -> i32 {
    candidate.priority.saturating_sub(route_priority_penalty(
        candidate.recent_penalty_count,
        candidate.recent_rate_limit_count,
        candidate.recent_overload_count,
    ))
}

fn effective_account_priority(account: &CachedRouteAccount) -> i32 {
    account.priority.saturating_sub(route_priority_penalty(
        account.recent_penalty_count,
        account.recent_rate_limit_count,
        account.recent_overload_count,
    ))
}

fn route_priority_penalty(
    recent_penalty_count: i32,
    recent_rate_limit_count: i32,
    recent_overload_count: i32,
) -> i32 {
    recent_penalty_count
        .saturating_mul(10)
        .saturating_add(recent_rate_limit_count.saturating_mul(5))
        .saturating_add(recent_overload_count.saturating_mul(3))
}

fn candidate_health_key(
    candidate: &CachedRouteCandidate,
    accounts: &[&CachedRouteAccount],
) -> CandidateHealthKey {
    let best_account = accounts
        .iter()
        .map(|account| account_health_key(account))
        .min()
        .unwrap_or((i32::MAX, i32::MAX, i32::MAX, i32::MAX, i32::MAX));
    (
        candidate.recent_penalty_count,
        candidate.recent_rate_limit_count,
        candidate.recent_overload_count,
        candidate.channel_failure_streak,
        route_health_status_rank(candidate.last_health_status),
        candidate.channel_response_time.max(0),
        best_account,
    )
}

fn account_health_key(account: &CachedRouteAccount) -> AccountHealthKey {
    (
        account.recent_penalty_count,
        account.recent_rate_limit_count,
        account.recent_overload_count,
        account.failure_streak,
        account.response_time.max(0),
    )
}

fn route_health_status_rank(status: i16) -> i16 {
    match status {
        1 => 0,
        2 => 1,
        3 => 2,
        _ => 3,
    }
}

fn default_route_health_status() -> i16 {
    1
}

fn merge_default_route_candidates(
    mut primary: Vec<CachedRouteCandidate>,
    fallback: Vec<CachedRouteCandidate>,
) -> Vec<CachedRouteCandidate> {
    for candidate in fallback {
        if primary
            .iter()
            .any(|existing| existing.channel_id == candidate.channel_id)
        {
            continue;
        }
        primary.push(candidate);
    }
    primary
}

pub(crate) fn route_cache_version_key() -> &'static str {
    "ai:cache:route:version"
}

fn route_cache_key(version: i64, group: &str, endpoint_scope: &str, model: &str) -> String {
    format!("ai:cache:route:v{version}:{group}:{endpoint_scope}:{model}")
}

fn default_route_cache_key(version: i64, group: &str, endpoint_scope: &str) -> String {
    format!("ai:cache:default-route:v{version}:{group}:{endpoint_scope}")
}

fn channel_supports_endpoint_scope(
    channel_type: i16,
    endpoint_scopes: &serde_json::Value,
    endpoint_scope: &str,
) -> bool {
    let mut configured_scopes: Vec<String> = endpoint_scopes
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(ToOwned::to_owned))
                .collect()
        })
        .unwrap_or_default();
    if configured_scopes.is_empty() {
        configured_scopes.push("chat".to_string());
    }

    let effective_scopes = if let Some(allowlist) = provider_scope_allowlist(channel_type) {
        configured_scopes
            .into_iter()
            .filter(|scope| allowlist.contains(&scope.as_str()))
            .collect::<Vec<_>>()
    } else {
        configured_scopes
    };

    effective_scopes.iter().any(|scope| scope == endpoint_scope)
}

#[cfg(test)]
mod tests;
