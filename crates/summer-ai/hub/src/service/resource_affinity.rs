use anyhow::Context;
use sea_orm::EntityTrait;
use serde::{Deserialize, Serialize};
use summer::plugin::Service;
use summer_common::error::{ApiErrors, ApiResult};
use summer_sea_orm::DbConn;

use summer_ai_model::entity::channel::{self, ChannelStatus};
use summer_ai_model::entity::channel_account::{self, AccountStatus};

use crate::relay::channel_router::SelectedChannel;
use crate::service::channel::ChannelService;
use crate::service::runtime_cache::RuntimeCacheService;
use crate::service::token::TokenInfo;

const RESOURCE_AFFINITY_TTL_SECONDS: u64 = 7 * 24 * 60 * 60;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ResourceAffinityRecord {
    token_id: i64,
    group: String,
    #[serde(default)]
    channel_type: i16,
    #[serde(default)]
    base_url: String,
    channel_id: i64,
    account_id: i64,
}

impl ResourceAffinityRecord {
    fn matches_token_scope(&self, token_info: &TokenInfo) -> bool {
        self.token_id == token_info.token_id && self.group == token_info.group
    }

    fn matches_channel_snapshot(&self, channel_type: i16, base_url: &str) -> bool {
        self.channel_type == channel_type
            && normalize_base_url(&self.base_url) == normalize_base_url(base_url)
    }
}

#[derive(Clone, Service)]
pub struct ResourceAffinityService {
    #[inject(component)]
    db: DbConn,
    #[inject(component)]
    cache: RuntimeCacheService,
}

impl ResourceAffinityService {
    pub fn new(db: DbConn, cache: RuntimeCacheService) -> Self {
        Self { db, cache }
    }

    pub async fn bind(
        &self,
        token_info: &TokenInfo,
        resource_kind: &str,
        resource_id: &str,
        channel: &SelectedChannel,
    ) -> ApiResult<()> {
        if resource_id.trim().is_empty() {
            return Ok(());
        }

        let record = ResourceAffinityRecord {
            token_id: token_info.token_id,
            group: token_info.group.clone(),
            channel_type: channel.channel_type,
            base_url: channel.base_url.clone(),
            channel_id: channel.channel_id,
            account_id: channel.account_id,
        };
        self.cache
            .set_json(
                &resource_affinity_key(token_info.token_id, resource_kind, resource_id),
                &record,
                RESOURCE_AFFINITY_TTL_SECONDS,
            )
            .await
    }

    pub async fn resolve(
        &self,
        token_info: &TokenInfo,
        resource_kind: &str,
        resource_id: &str,
    ) -> ApiResult<Option<SelectedChannel>> {
        let cache_key = resource_affinity_key(token_info.token_id, resource_kind, resource_id);
        let Some(record) = self
            .cache
            .get_json::<ResourceAffinityRecord>(&cache_key)
            .await?
        else {
            return Ok(None);
        };

        if !record.matches_token_scope(token_info) {
            return Ok(None);
        }

        let Some(channel_model) = channel::Entity::find_by_id(record.channel_id)
            .one(&self.db)
            .await
            .context("failed to query bound channel")
            .map_err(ApiErrors::Internal)?
        else {
            let _ = self.cache.delete(&cache_key).await;
            return Ok(None);
        };
        if !channel_is_available(channel_model.status, channel_model.deleted_at.is_none()) {
            let _ = self.cache.delete(&cache_key).await;
            return Ok(None);
        }
        if !record
            .matches_channel_snapshot(channel_model.channel_type as i16, &channel_model.base_url)
        {
            let _ = self.cache.delete(&cache_key).await;
            return Ok(None);
        }

        let Some(account_model) = channel_account::Entity::find_by_id(record.account_id)
            .one(&self.db)
            .await
            .context("failed to query bound channel account")
            .map_err(ApiErrors::Internal)?
        else {
            let _ = self.cache.delete(&cache_key).await;
            return Ok(None);
        };

        let now = chrono::Utc::now().fixed_offset();
        let api_key = ChannelService::extract_api_key(&account_model.credentials);
        if account_model.channel_id != channel_model.id
            || !account_is_available(
                account_model.status,
                account_model.schedulable,
                account_model.deleted_at.is_none(),
                account_model.expires_at,
                account_model.rate_limited_until,
                account_model.overload_until,
                !api_key.is_empty(),
                now,
            )
        {
            let _ = self.cache.delete(&cache_key).await;
            return Ok(None);
        }

        Ok(Some(SelectedChannel {
            channel_id: channel_model.id,
            channel_name: channel_model.name,
            channel_type: channel_model.channel_type as i16,
            base_url: channel_model.base_url,
            model_mapping: channel_model.model_mapping,
            api_key,
            account_id: account_model.id,
            account_name: account_model.name,
        }))
    }

    pub async fn delete(
        &self,
        token_info: &TokenInfo,
        resource_kind: &str,
        resource_id: &str,
    ) -> ApiResult<()> {
        self.cache
            .delete(&resource_affinity_key(
                token_info.token_id,
                resource_kind,
                resource_id,
            ))
            .await
    }
}

fn channel_is_available(status: ChannelStatus, not_deleted: bool) -> bool {
    status == ChannelStatus::Enabled && not_deleted
}

#[allow(clippy::too_many_arguments)]
fn account_is_available(
    status: AccountStatus,
    schedulable: bool,
    not_deleted: bool,
    expires_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    rate_limited_until: Option<chrono::DateTime<chrono::FixedOffset>>,
    overload_until: Option<chrono::DateTime<chrono::FixedOffset>>,
    has_api_key: bool,
    now: chrono::DateTime<chrono::FixedOffset>,
) -> bool {
    status == AccountStatus::Enabled
        && schedulable
        && not_deleted
        && has_api_key
        && expires_at.is_none_or(|expires_at| expires_at > now)
        && rate_limited_until.is_none_or(|recover_at| recover_at <= now)
        && overload_until.is_none_or(|recover_at| recover_at <= now)
}

fn resource_affinity_key(token_id: i64, resource_kind: &str, resource_id: &str) -> String {
    format!("ai:resource-affinity:{token_id}:{resource_kind}:{resource_id}")
}

fn normalize_base_url(base_url: &str) -> &str {
    base_url.trim_end_matches('/')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::token::TokenInfo;
    use summer_ai_model::entity::channel::ChannelStatus;
    use summer_ai_model::entity::channel_account::AccountStatus;

    #[test]
    fn resource_affinity_key_is_namespaced() {
        assert_eq!(
            resource_affinity_key(7, "response", "resp_123"),
            "ai:resource-affinity:7:response:resp_123"
        );
    }

    #[test]
    fn resource_affinity_key_is_scoped_by_token() {
        assert_ne!(
            resource_affinity_key(7, "response", "resp_123"),
            resource_affinity_key(8, "response", "resp_123")
        );
    }

    #[test]
    fn affinity_record_matches_same_token_scope() {
        let record = ResourceAffinityRecord {
            token_id: 7,
            group: "default".into(),
            channel_type: 1,
            base_url: "https://primary.example".into(),
            channel_id: 11,
            account_id: 22,
        };

        assert!(record.matches_token_scope(&sample_token_info(7, "default")));
    }

    #[test]
    fn affinity_record_rejects_different_token_scope() {
        let record = ResourceAffinityRecord {
            token_id: 7,
            group: "default".into(),
            channel_type: 1,
            base_url: "https://primary.example".into(),
            channel_id: 11,
            account_id: 22,
        };

        assert!(!record.matches_token_scope(&sample_token_info(8, "default")));
        assert!(!record.matches_token_scope(&sample_token_info(7, "other")));
    }

    #[test]
    fn affinity_record_matches_same_channel_snapshot() {
        let record = ResourceAffinityRecord {
            token_id: 7,
            group: "default".into(),
            channel_type: 3,
            base_url: "https://upstream.example/".into(),
            channel_id: 11,
            account_id: 22,
        };

        assert!(record.matches_channel_snapshot(3, "https://upstream.example"));
    }

    #[test]
    fn affinity_record_rejects_changed_channel_snapshot() {
        let record = ResourceAffinityRecord {
            token_id: 7,
            group: "default".into(),
            channel_type: 3,
            base_url: "https://upstream.example".into(),
            channel_id: 11,
            account_id: 22,
        };

        assert!(!record.matches_channel_snapshot(1, "https://upstream.example"));
        assert!(!record.matches_channel_snapshot(3, "https://other.example"));
    }

    #[test]
    fn legacy_affinity_record_without_snapshot_metadata_is_rejected() -> anyhow::Result<()> {
        let record: ResourceAffinityRecord = serde_json::from_value(serde_json::json!({
            "token_id": 7,
            "group": "default",
            "channel_id": 11,
            "account_id": 22
        }))?;

        assert!(!record.matches_channel_snapshot(3, "https://upstream.example"));

        Ok(())
    }

    #[test]
    fn bound_channel_becomes_invalid_when_not_enabled() {
        assert!(!channel_is_available(ChannelStatus::AutoDisabled, true));
        assert!(!channel_is_available(ChannelStatus::Enabled, false));
        assert!(channel_is_available(ChannelStatus::Enabled, true));
    }

    #[test]
    fn bound_account_becomes_invalid_when_rate_limited_or_overloaded() {
        let now = chrono::Utc::now().fixed_offset();

        assert!(account_is_available(
            AccountStatus::Enabled,
            true,
            true,
            Some(now + chrono::Duration::minutes(10)),
            None,
            None,
            true,
            now,
        ));
        assert!(!account_is_available(
            AccountStatus::Enabled,
            true,
            true,
            Some(now + chrono::Duration::minutes(10)),
            Some(now + chrono::Duration::minutes(1)),
            None,
            true,
            now,
        ));
        assert!(!account_is_available(
            AccountStatus::Enabled,
            true,
            true,
            Some(now + chrono::Duration::minutes(10)),
            None,
            Some(now + chrono::Duration::minutes(1)),
            true,
            now,
        ));
    }

    fn sample_token_info(token_id: i64, group: &str) -> TokenInfo {
        TokenInfo {
            token_id,
            user_id: 1,
            name: "demo".into(),
            group: group.into(),
            remain_quota: 100,
            unlimited_quota: false,
            rpm_limit: 60,
            tpm_limit: 1000,
            concurrency_limit: 1,
            allowed_models: Vec::new(),
            endpoint_scopes: Vec::new(),
        }
    }
}
