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
    accounts: Vec<CachedRouteAccount>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedRouteAccount {
    account_id: i64,
    account_name: String,
    weight: i32,
    priority: i32,
    api_key: String,
}

struct LoadedSchedulableAccounts {
    grouped: std::collections::HashMap<i64, Vec<CachedRouteAccount>>,
    ttl_seconds: u64,
}

#[derive(Clone, Service)]
pub struct ChannelRouter {
    #[inject(component)]
    db: DbConn,
    #[inject(component)]
    cache: RuntimeCacheService,
}

impl ChannelRouter {
    pub fn new(db: DbConn, cache: RuntimeCacheService) -> Self {
        Self { db, cache }
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

                Some(CachedRouteCandidate {
                    channel_id: channel.id,
                    channel_name: channel.name.clone(),
                    channel_type: channel.channel_type as i16,
                    base_url: channel.base_url.clone(),
                    model_mapping: channel.model_mapping.clone(),
                    priority: ability.priority,
                    weight: ability.weight,
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

                Some(CachedRouteCandidate {
                    channel_id: channel.id,
                    channel_name: channel.name.clone(),
                    channel_type: channel.channel_type as i16,
                    base_url: channel.base_url.clone(),
                    model_mapping: channel.model_mapping.clone(),
                    priority: ability.priority,
                    weight: ability.weight,
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

                Some(CachedRouteCandidate {
                    channel_id: channel.id,
                    channel_name: channel.name,
                    channel_type: channel.channel_type as i16,
                    base_url: channel.base_url,
                    model_mapping: channel.model_mapping,
                    priority: channel.priority,
                    weight: channel.weight,
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
        let mut next_refresh_at = None;
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

            grouped
                .entry(account.channel_id)
                .or_default()
                .push(CachedRouteAccount {
                    account_id: account.id,
                    account_name: account.name,
                    weight: account.weight,
                    priority: account.priority,
                    api_key,
                });
        }

        Ok(LoadedSchedulableAccounts {
            grouped,
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

fn select_from_route_candidates(
    candidates: &[CachedRouteCandidate],
    exclusions: &RouteSelectionExclusions,
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

    let max_priority = available_candidates
        .iter()
        .map(|(candidate, _)| candidate.priority)
        .max()?;
    let weighted_candidates: Vec<(usize, i32)> = available_candidates
        .iter()
        .enumerate()
        .filter(|(_, (candidate, _))| candidate.priority == max_priority)
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
    let mut plan = Vec::new();
    let mut planning_exclusions = exclusions.clone();

    while let Some(selected) = select_from_route_candidates(candidates, &planning_exclusions) {
        planning_exclusions.exclude_selected_account(&selected);
        plan.push(selected);
    }

    plan
}

fn pick_schedulable_account<'a>(
    accounts: &'a [&'a CachedRouteAccount],
) -> Option<&'a CachedRouteAccount> {
    let max_priority = accounts.iter().map(|account| account.priority).max()?;
    let weighted_candidates: Vec<(usize, i32)> = accounts
        .iter()
        .enumerate()
        .filter(|(_, account)| account.priority == max_priority)
        .map(|(index, account)| (index, account.weight))
        .collect();
    let index = weighted_random_select(&weighted_candidates)?;
    accounts.get(index).copied()
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
mod tests {
    use super::*;

    #[test]
    fn select_from_route_candidates_falls_back_to_another_account_before_another_channel() {
        let mut exclusions = RouteSelectionExclusions::default();
        exclusions.exclude_account(101);

        let selected = select_from_route_candidates(&sample_candidates(), &exclusions).unwrap();

        assert_eq!(selected.channel_id, 11);
        assert_eq!(selected.account_id, 102);
    }

    #[test]
    fn select_from_route_candidates_skips_excluded_channel() {
        let mut exclusions = RouteSelectionExclusions::default();
        exclusions.exclude_channel(11);

        let selected = select_from_route_candidates(&sample_candidates(), &exclusions).unwrap();

        assert_eq!(selected.channel_id, 12);
        assert_eq!(selected.account_id, 201);
    }

    #[test]
    fn select_from_route_candidates_returns_none_when_all_accounts_are_excluded() {
        let mut exclusions = RouteSelectionExclusions::default();
        exclusions.exclude_account(101);
        exclusions.exclude_account(102);
        exclusions.exclude_channel(12);

        assert!(select_from_route_candidates(&sample_candidates(), &exclusions).is_none());
    }

    #[test]
    fn selected_is_excluded_matches_channel_or_account() {
        let selected = SelectedChannel {
            channel_id: 11,
            channel_name: "primary".into(),
            channel_type: 1,
            base_url: "https://primary.example".into(),
            model_mapping: serde_json::json!({}),
            api_key: "sk-primary".into(),
            account_id: 101,
            account_name: "primary-a".into(),
        };

        let mut exclusions = RouteSelectionExclusions::default();
        assert!(!exclusions.selected_is_excluded(&selected));

        exclusions.exclude_account(101);
        assert!(exclusions.selected_is_excluded(&selected));

        let mut exclusions = RouteSelectionExclusions::default();
        exclusions.exclude_channel(11);
        assert!(exclusions.selected_is_excluded(&selected));
    }

    #[test]
    fn select_from_route_candidates_falls_back_to_lower_priority_when_top_priority_is_exhausted() {
        let mut exclusions = RouteSelectionExclusions::default();
        exclusions.exclude_account(101);
        exclusions.exclude_account(102);

        let selected = select_from_route_candidates(&sample_candidates(), &exclusions).unwrap();

        assert_eq!(selected.channel_id, 12);
        assert_eq!(selected.account_id, 201);
    }

    #[test]
    fn channel_supports_endpoint_scope_defaults_to_chat() {
        assert!(channel_supports_endpoint_scope(
            1,
            &serde_json::json!([]),
            "chat"
        ));
        assert!(!channel_supports_endpoint_scope(
            1,
            &serde_json::json!([]),
            "responses"
        ));
    }

    #[test]
    fn channel_supports_endpoint_scope_respects_provider_allowlist() {
        assert!(channel_supports_endpoint_scope(
            3,
            &serde_json::json!(["chat", "responses"]),
            "responses"
        ));
        assert!(channel_supports_endpoint_scope(
            3,
            &serde_json::json!(["chat", "responses"]),
            "chat"
        ));
        assert!(!channel_supports_endpoint_scope(
            3,
            &serde_json::json!(["chat", "responses"]),
            "embeddings"
        ));
    }

    #[test]
    fn channel_supports_endpoint_scope_keeps_azure_responses_available() {
        assert!(channel_supports_endpoint_scope(
            14,
            &serde_json::json!(["chat", "responses", "embeddings"]),
            "responses"
        ));
        assert!(channel_supports_endpoint_scope(
            14,
            &serde_json::json!(["chat", "responses", "embeddings"]),
            "embeddings"
        ));
    }

    #[test]
    fn channel_supports_endpoint_scope_keeps_gemini_embeddings_available() {
        assert!(channel_supports_endpoint_scope(
            24,
            &serde_json::json!(["chat", "embeddings"]),
            "embeddings"
        ));
        assert!(!channel_supports_endpoint_scope(
            24,
            &serde_json::json!(["chat", "embeddings"]),
            "responses"
        ));
    }

    #[test]
    fn merge_default_route_candidates_keeps_ability_candidates_and_adds_scope_fallbacks() {
        let merged = merge_default_route_candidates(
            vec![CachedRouteCandidate {
                channel_id: 11,
                channel_name: "ability".into(),
                channel_type: 1,
                base_url: "https://ability.example".into(),
                model_mapping: serde_json::json!({}),
                priority: 10,
                weight: 2,
                accounts: vec![CachedRouteAccount {
                    account_id: 101,
                    account_name: "ability-account".into(),
                    weight: 1,
                    priority: 10,
                    api_key: "sk-ability".into(),
                }],
            }],
            vec![
                CachedRouteCandidate {
                    channel_id: 11,
                    channel_name: "fallback-dup".into(),
                    channel_type: 1,
                    base_url: "https://fallback-dup.example".into(),
                    model_mapping: serde_json::json!({}),
                    priority: 1,
                    weight: 1,
                    accounts: vec![CachedRouteAccount {
                        account_id: 111,
                        account_name: "fallback-dup-account".into(),
                        weight: 1,
                        priority: 1,
                        api_key: "sk-fallback-dup".into(),
                    }],
                },
                CachedRouteCandidate {
                    channel_id: 12,
                    channel_name: "fallback".into(),
                    channel_type: 1,
                    base_url: "https://fallback.example".into(),
                    model_mapping: serde_json::json!({}),
                    priority: 5,
                    weight: 1,
                    accounts: vec![CachedRouteAccount {
                        account_id: 201,
                        account_name: "fallback-account".into(),
                        weight: 1,
                        priority: 10,
                        api_key: "sk-fallback".into(),
                    }],
                },
            ],
        );

        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].channel_id, 11);
        assert_eq!(merged[0].accounts[0].account_id, 101);
        assert_eq!(merged[1].channel_id, 12);
    }

    #[test]
    fn weighted_random_select_returns_first_item_when_total_weight_is_negative() {
        let picked = weighted_random_select(&[(11_i64, -3), (12_i64, -2)]);
        assert_eq!(picked, Some(11));
    }

    #[test]
    fn select_from_route_candidates_returns_first_top_priority_candidate_when_weights_are_non_positive()
     {
        let selected = select_from_route_candidates(
            &[
                CachedRouteCandidate {
                    channel_id: 11,
                    channel_name: "primary".into(),
                    channel_type: 1,
                    base_url: "https://primary.example".into(),
                    model_mapping: serde_json::json!({}),
                    priority: 10,
                    weight: -5,
                    accounts: vec![CachedRouteAccount {
                        account_id: 101,
                        account_name: "primary-account".into(),
                        weight: 1,
                        priority: 10,
                        api_key: "sk-primary".into(),
                    }],
                },
                CachedRouteCandidate {
                    channel_id: 12,
                    channel_name: "fallback".into(),
                    channel_type: 1,
                    base_url: "https://fallback.example".into(),
                    model_mapping: serde_json::json!({}),
                    priority: 10,
                    weight: -1,
                    accounts: vec![CachedRouteAccount {
                        account_id: 201,
                        account_name: "fallback-account".into(),
                        weight: 1,
                        priority: 10,
                        api_key: "sk-fallback".into(),
                    }],
                },
            ],
            &RouteSelectionExclusions::default(),
        )
        .unwrap();

        assert_eq!(selected.channel_id, 11);
        assert_eq!(selected.account_id, 101);
    }

    #[test]
    fn compute_route_cache_ttl_seconds_defaults_to_base_ttl_without_refresh_deadline() {
        let now = chrono::Utc::now().fixed_offset();

        assert_eq!(
            compute_route_cache_ttl_seconds(now, None),
            ROUTE_CACHE_TTL_SECONDS
        );
    }

    #[test]
    fn compute_route_cache_ttl_seconds_uses_earliest_refresh_deadline() {
        let now = chrono::Utc::now().fixed_offset();

        assert_eq!(
            compute_route_cache_ttl_seconds(now, Some(now + chrono::Duration::seconds(12))),
            12
        );
        assert_eq!(
            compute_route_cache_ttl_seconds(now, Some(now + chrono::Duration::milliseconds(400))),
            1
        );
    }

    #[test]
    fn weighted_random_select_supports_large_positive_weights_without_overflow() {
        let picked = weighted_random_select(&[(11_i64, i32::MAX), (12_i64, i32::MAX)]);

        assert!(matches!(picked, Some(11 | 12)));
    }

    #[test]
    fn build_route_plan_from_candidates_keeps_request_local_fallback_order_stable() {
        let plan = build_route_plan_from_candidates(
            &sample_candidates(),
            &RouteSelectionExclusions::default(),
        );

        let ordered: Vec<(i64, i64)> = plan
            .into_iter()
            .map(|selected| (selected.channel_id, selected.account_id))
            .collect();

        assert_eq!(ordered, vec![(11, 101), (11, 102), (12, 201)]);
    }

    #[test]
    fn route_selection_plan_skips_future_entries_for_failed_channel() {
        let mut plan = RouteSelectionPlan::new(
            build_route_plan_from_candidates(
                &sample_candidates(),
                &RouteSelectionExclusions::default(),
            ),
            RouteSelectionExclusions::default(),
        );

        let first = plan.next().expect("first route");
        assert_eq!((first.channel_id, first.account_id), (11, 101));

        plan.exclude_selected_channel(&first);

        let second = plan.next().expect("second route");
        assert_eq!((second.channel_id, second.account_id), (12, 201));
        assert!(plan.next().is_none());
    }

    fn sample_candidates() -> Vec<CachedRouteCandidate> {
        vec![
            CachedRouteCandidate {
                channel_id: 11,
                channel_name: "primary".into(),
                channel_type: 1,
                base_url: "https://primary.example".into(),
                model_mapping: serde_json::json!({}),
                priority: 10,
                weight: 1,
                accounts: vec![
                    CachedRouteAccount {
                        account_id: 101,
                        account_name: "primary-a".into(),
                        weight: 1,
                        priority: 10,
                        api_key: "sk-primary-a".into(),
                    },
                    CachedRouteAccount {
                        account_id: 102,
                        account_name: "primary-b".into(),
                        weight: 1,
                        priority: 8,
                        api_key: "sk-primary-b".into(),
                    },
                ],
            },
            CachedRouteCandidate {
                channel_id: 12,
                channel_name: "secondary".into(),
                channel_type: 1,
                base_url: "https://secondary.example".into(),
                model_mapping: serde_json::json!({}),
                priority: 5,
                weight: 0,
                accounts: vec![CachedRouteAccount {
                    account_id: 201,
                    account_name: "secondary-a".into(),
                    weight: 1,
                    priority: 10,
                    api_key: "sk-secondary-a".into(),
                }],
            },
        ]
    }
}
