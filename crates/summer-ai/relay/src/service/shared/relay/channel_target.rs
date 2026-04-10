use std::collections::HashMap;

use anyhow::Context;
use summer_ai_core::provider::{ProviderKind, ProviderRegistry};
use summer_ai_model::entity::ability;
use summer_ai_model::entity::channel;
use summer_ai_model::entity::channel::ChannelType;
use summer_ai_model::entity::channel_account;
use summer_common::error::{ApiErrors, ApiResult};
use summer_sea_orm::DbConn;

#[derive(Clone)]
pub(crate) struct ResolvedRelayTarget {
    pub(crate) channel: channel::Model,
    pub(crate) account: channel_account::Model,
    pub(crate) provider_kind: ProviderKind,
    pub(crate) base_url: String,
    pub(crate) upstream_model: String,
    pub(crate) api_key: String,
}

pub(crate) async fn resolve_relay_target(
    db: &DbConn,
    channel_group: &str,
    endpoint_scope: &str,
    requested_model: &str,
) -> ApiResult<ResolvedRelayTarget> {
    let abilities = ability::Entity::find_enabled_route_candidates(
        db,
        channel_group,
        endpoint_scope,
        requested_model,
    )
    .await
    .context("查询模型能力失败")?;

    if abilities.is_empty() {
        return Err(ApiErrors::NotFound(format!(
            "model '{}' is not available",
            requested_model
        )));
    }

    let channel_ids = collect_channel_ids(&abilities);
    let channels = channel::Entity::find_enabled_undeleted_by_ids(db, &channel_ids)
        .await
        .context("查询渠道失败")?;
    let accounts = channel_account::Entity::find_schedulable_by_channel_ids(db, &channel_ids)
        .await
        .context("查询渠道账号失败")?;

    let channels_by_id = channels
        .into_iter()
        .map(|channel| (channel.id, channel))
        .collect::<HashMap<_, _>>();
    let accounts_by_channel_id = group_accounts_by_channel_id(accounts);

    if let Some(target) = select_resolved_target(
        &abilities,
        &channels_by_id,
        &accounts_by_channel_id,
        requested_model,
    )? {
        return Ok(target);
    }

    Err(ApiErrors::ServiceUnavailable(format!(
        "no available channel for model '{}'",
        requested_model
    )))
}

fn collect_channel_ids(abilities: &[ability::Model]) -> Vec<i64> {
    let mut channel_ids = Vec::new();
    for ability in abilities {
        if !channel_ids.contains(&ability.channel_id) {
            channel_ids.push(ability.channel_id);
        }
    }
    channel_ids
}

fn group_accounts_by_channel_id(
    accounts: Vec<channel_account::Model>,
) -> HashMap<i64, Vec<channel_account::Model>> {
    let mut grouped = HashMap::<i64, Vec<channel_account::Model>>::new();
    for account in accounts {
        grouped.entry(account.channel_id).or_default().push(account);
    }
    grouped
}

fn select_resolved_target(
    abilities: &[ability::Model],
    channels_by_id: &HashMap<i64, channel::Model>,
    accounts_by_channel_id: &HashMap<i64, Vec<channel_account::Model>>,
    requested_model: &str,
) -> Result<Option<ResolvedRelayTarget>, ApiErrors> {
    for ability in abilities {
        let Some(channel) = channels_by_id.get(&ability.channel_id) else {
            continue;
        };

        let Some(accounts) = accounts_by_channel_id.get(&channel.id) else {
            continue;
        };

        let Some((account, api_key)) = accounts
            .iter()
            .find_map(|account| account.api_key().map(|api_key| (account.clone(), api_key)))
        else {
            continue;
        };

        let provider_kind = provider_kind_from_channel_type(channel.channel_type)
            .ok_or_else(|| ApiErrors::BadRequest("unsupported channel type".to_string()))?;
        let upstream_model = channel.resolve_upstream_model(requested_model);
        let base_url = effective_base_url(channel, provider_kind);

        return Ok(Some(ResolvedRelayTarget {
            channel: channel.clone(),
            account,
            provider_kind,
            base_url,
            upstream_model,
            api_key,
        }));
    }

    Ok(None)
}

pub(crate) fn provider_kind_from_channel_type(channel_type: ChannelType) -> Option<ProviderKind> {
    ProviderKind::from_channel_type(channel_type as i16)
}

pub(crate) fn effective_base_url(channel: &channel::Model, provider_kind: ProviderKind) -> String {
    if channel.base_url.trim().is_empty() {
        ProviderRegistry::meta(provider_kind)
            .default_base_url
            .to_string()
    } else {
        channel.base_url.clone()
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use summer_ai_model::entity::ability;
    use summer_ai_model::entity::channel::{
        self, ChannelLastHealthStatus, ChannelStatus, ChannelType,
    };
    use summer_ai_model::entity::channel_account::{self, ChannelAccountStatus};

    use super::{group_accounts_by_channel_id, select_resolved_target};

    fn sample_channel() -> channel::Model {
        let now = Utc::now().fixed_offset();
        channel::Model {
            id: 1,
            name: "openai-primary".into(),
            channel_type: ChannelType::OpenAi,
            vendor_code: "openai".into(),
            base_url: "https://api.openai.com".into(),
            status: ChannelStatus::Enabled,
            models: serde_json::json!(["gpt-4o"]),
            model_mapping: serde_json::json!({"gpt-4o": "gpt-4o-2026-01-01"}),
            channel_group: "default".into(),
            endpoint_scopes: serde_json::json!(["chat"]),
            capabilities: serde_json::json!(["streaming"]),
            weight: 1,
            priority: 10,
            config: serde_json::json!({}),
            auto_ban: true,
            test_model: "gpt-4o".into(),
            used_quota: 0,
            balance: 0.into(),
            balance_updated_at: None,
            response_time: 0,
            success_rate: 0.into(),
            failure_streak: 0,
            last_used_at: None,
            last_error_at: None,
            last_error_code: String::new(),
            last_error_message: String::new(),
            last_health_status: ChannelLastHealthStatus::Unknown,
            deleted_at: None,
            remark: String::new(),
            create_by: "system".into(),
            create_time: now,
            update_by: "system".into(),
            update_time: now,
        }
    }

    fn sample_account(
        id: i64,
        priority: i32,
        schedulable: bool,
        status: ChannelAccountStatus,
    ) -> channel_account::Model {
        let now = Utc::now().fixed_offset();
        channel_account::Model {
            id,
            channel_id: 1,
            name: format!("account-{id}"),
            credential_type: "api_key".into(),
            credentials: serde_json::json!({"api_key": format!("sk-{id}")}),
            secret_ref: String::new(),
            status,
            schedulable,
            priority,
            weight: 1,
            rate_multiplier: 1.into(),
            concurrency_limit: 0,
            quota_limit: 0.into(),
            quota_used: 0.into(),
            balance: 0.into(),
            balance_updated_at: None,
            response_time: 0,
            failure_streak: 0,
            last_used_at: None,
            last_error_at: None,
            last_error_code: String::new(),
            last_error_message: String::new(),
            rate_limited_until: None,
            overload_until: None,
            expires_at: None,
            test_model: String::new(),
            test_time: None,
            extra: serde_json::json!({}),
            deleted_at: None,
            remark: String::new(),
            create_by: "system".into(),
            create_time: now,
            update_by: "system".into(),
            update_time: now,
        }
    }

    #[test]
    fn extract_api_key_reads_api_key_from_credentials() {
        let account = sample_account(1, 1, true, ChannelAccountStatus::Enabled);
        assert_eq!(account.api_key().as_deref(), Some("sk-1"));
    }

    #[test]
    fn resolve_upstream_model_prefers_channel_mapping() {
        let channel = sample_channel();
        assert_eq!(
            channel.resolve_upstream_model("gpt-4o"),
            "gpt-4o-2026-01-01"
        );
        assert_eq!(channel.resolve_upstream_model("gpt-4.1"), "gpt-4.1");
    }

    #[test]
    fn select_schedulable_account_prefers_enabled_schedulable_high_priority_account() {
        let disabled = sample_account(1, 100, true, ChannelAccountStatus::Disabled);
        let low = sample_account(2, 10, true, ChannelAccountStatus::Enabled);
        let high = sample_account(3, 20, true, ChannelAccountStatus::Enabled);

        let selected = [disabled, low, high]
            .into_iter()
            .filter(|account| account.deleted_at.is_none())
            .filter(|account| account.schedulable)
            .filter(|account| account.status == ChannelAccountStatus::Enabled)
            .max_by_key(|account| (account.priority, account.weight, account.id))
            .expect("select account");
        assert_eq!(selected.id, 3);
    }

    #[test]
    fn select_resolved_target_uses_preloaded_channels_and_accounts() {
        let now = chrono::Utc::now().fixed_offset();
        let abilities = vec![ability::Model {
            id: 1,
            channel_group: "default".into(),
            endpoint_scope: "chat".into(),
            model: "gpt-4o".into(),
            channel_id: 1,
            enabled: true,
            priority: 10,
            weight: 1,
            route_config: serde_json::json!({}),
            create_time: now,
            update_time: now,
        }];
        let channel = sample_channel();
        let channels_by_id = std::iter::once((channel.id, channel)).collect();
        let accounts_by_channel_id = group_accounts_by_channel_id(vec![sample_account(
            3,
            20,
            true,
            ChannelAccountStatus::Enabled,
        )]);

        let target = select_resolved_target(
            &abilities,
            &channels_by_id,
            &accounts_by_channel_id,
            "gpt-4o",
        )
        .expect("select target")
        .expect("resolved target");

        assert_eq!(target.channel.id, 1);
        assert_eq!(target.account.id, 3);
        assert_eq!(target.api_key, "sk-3");
        assert_eq!(target.upstream_model, "gpt-4o-2026-01-01");
    }
}
