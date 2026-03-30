use std::collections::HashMap;

use anyhow::Context;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder};
use summer::plugin::Service;
use summer_common::error::{ApiErrors, ApiResult};
use summer_sea_orm::DbConn;

use summer_ai_model::entity::ability;
use summer_ai_model::entity::channel::{self, ChannelStatus};
use summer_ai_model::entity::channel_account::{self, AccountStatus};
use summer_ai_model::vo::runtime::{
    AiRuntimeAccountVo, AiRuntimeChannelHealthVo, AiRuntimeRouteCandidateVo, AiRuntimeRouteVo,
};

use crate::service::channel::ChannelService;

#[derive(Clone, Service)]
pub struct RuntimeService {
    #[inject(component)]
    db: DbConn,
}

impl RuntimeService {
    pub fn new(db: DbConn) -> Self {
        Self { db }
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

        Ok(build_runtime_health_items(
            channels,
            accounts,
            chrono::Utc::now().fixed_offset(),
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

        Ok(build_runtime_route_items(
            abilities,
            channels,
            accounts,
            chrono::Utc::now().fixed_offset(),
        ))
    }
}

fn build_runtime_health_items(
    channels: Vec<channel::Model>,
    accounts: Vec<channel_account::Model>,
    now: chrono::DateTime<chrono::FixedOffset>,
) -> Vec<AiRuntimeChannelHealthVo> {
    let accounts_by_channel = group_accounts_by_channel(accounts, now);

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
) -> Vec<AiRuntimeRouteVo> {
    let channel_map: HashMap<i64, channel::Model> =
        channels.into_iter().map(|item| (item.id, item)).collect();
    let accounts_by_channel = group_accounts_by_channel(accounts, now);
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

fn group_accounts_by_channel(
    accounts: Vec<channel_account::Model>,
    now: chrono::DateTime<chrono::FixedOffset>,
) -> HashMap<i64, Vec<AiRuntimeAccountVo>> {
    let mut grouped: HashMap<i64, Vec<AiRuntimeAccountVo>> = HashMap::new();
    for account in accounts {
        grouped
            .entry(account.channel_id)
            .or_default()
            .push(map_runtime_account(account, now));
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
) -> AiRuntimeAccountVo {
    let available = account_is_available(&account, now);
    AiRuntimeAccountVo {
        id: account.id,
        name: account.name,
        status: account.status,
        schedulable: account.schedulable,
        priority: account.priority,
        weight: account.weight,
        response_time: account.response_time,
        failure_streak: account.failure_streak,
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
    use sea_orm::prelude::BigDecimal;

    use super::{account_is_available, build_runtime_health_items, build_runtime_route_items};
    use summer_ai_model::entity::ability;
    use summer_ai_model::entity::channel::{self, ChannelStatus, ChannelType};
    use summer_ai_model::entity::channel_account::{self, AccountStatus};

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

        let health = build_runtime_health_items(
            channels,
            vec![
                make_account(10, 1, 10, 5),
                limited,
                make_account(30, 2, 5, 1),
            ],
            now,
        );

        assert_eq!(health.len(), 2);
        assert_eq!(health[0].id, 1);
        assert!(health[0].available);
        assert_eq!(health[0].available_account_count, 1);
        assert_eq!(health[0].accounts.len(), 2);
        assert_eq!(health[0].accounts[0].id, 20);
        assert!(!health[0].accounts[0].available);
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

        let routes = build_runtime_route_items(
            vec![
                make_ability(1, "default", "gpt-5.4", "chat", 1, 100, 20),
                make_ability(2, "default", "gpt-5.4", "chat", 2, 50, 10),
                make_ability(3, "default", "text-embedding-004", "embeddings", 2, 60, 5),
            ],
            vec![make_channel(1, "alpha", 100), make_channel(2, "beta", 80)],
            vec![make_account(11, 1, 9, 1), cooling],
            now,
        );

        assert_eq!(routes.len(), 2);
        assert_eq!(routes[0].model, "gpt-5.4");
        assert_eq!(routes[0].endpoint_scope, "chat");
        assert_eq!(routes[0].candidates.len(), 2);
        assert_eq!(routes[0].candidates[0].channel_id, 1);
        assert!(routes[0].candidates[0].available);
        assert_eq!(routes[0].candidates[1].channel_id, 2);
        assert!(!routes[0].candidates[1].available);
        assert_eq!(routes[0].candidates[1].available_account_count, 0);

        assert_eq!(routes[1].model, "text-embedding-004");
        assert_eq!(routes[1].endpoint_scope, "embeddings");
        assert_eq!(routes[1].candidates.len(), 1);
        assert_eq!(routes[1].candidates[0].channel_id, 2);
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
            last_error_message: String::new(),
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
            last_error_message: String::new(),
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
}
