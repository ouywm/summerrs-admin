use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use sea_orm::{ActiveModelTrait, EntityTrait};
use summer_ai_core::oauth::openai::{OpenAiStoredCredentials, build_stored_extra_overlay};
use summer_ai_model::entity::routing::channel_account;
use summer_redis::Redis;
use summer_redis::redis;
use summer_redis::redis::AsyncCommands;
use summer_sea_orm::DbConn;

use crate::error::RelayError;
use crate::service::oauth::credentials::next_token_version;
use crate::service::oauth::token_refresher::OpenAiTokenRefresher;

pub struct RefreshOutcome {
    pub account: channel_account::Model,
    pub refreshed: bool,
}

pub async fn refresh_if_needed(
    db: &DbConn,
    redis: &Redis,
    http: &reqwest::Client,
    account: &channel_account::Model,
    refresh_window: Duration,
) -> Result<RefreshOutcome, RelayError> {
    let lock = account_lock(account.id);
    let _guard = lock.lock().await;

    let fresh_account = channel_account::Entity::find_by_id(account.id)
        .one(db)
        .await
        .map_err(RelayError::Database)?
        .unwrap_or_else(|| account.clone());

    if !OpenAiTokenRefresher::needs_refresh(&fresh_account, refresh_window)? {
        return Ok(RefreshOutcome {
            account: fresh_account,
            refreshed: false,
        });
    }

    let refresher = OpenAiTokenRefresher::new(http);
    let mut refreshed = refresher.refresh(&fresh_account).await?;
    refreshed.stored.token_version = Some(next_token_version(refreshed.stored.token_version));

    let merged_extra = merge_refreshed_account_extra(
        &fresh_account.extra,
        &refreshed.stored,
        refreshed.privacy_mode.as_deref(),
    );
    let mut active: channel_account::ActiveModel = fresh_account.into();
    active.credentials = sea_orm::ActiveValue::Set(refreshed.encode());
    active.expires_at = sea_orm::ActiveValue::Set(Some(refreshed.stored.expires_at.fixed_offset()));
    active.extra = sea_orm::ActiveValue::Set(merged_extra);
    let account = active.update(db).await.map_err(RelayError::Database)?;
    invalidate_runtime_account_cache(redis, account.id)
        .await
        .map_err(|err| RelayError::Redis(err.to_string()))?;

    Ok(RefreshOutcome {
        account,
        refreshed: true,
    })
}

type LockMap = Mutex<HashMap<i64, Arc<tokio::sync::Mutex<()>>>>;

fn account_lock(account_id: i64) -> Arc<tokio::sync::Mutex<()>> {
    let map = LOCAL_LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = map.lock().expect("oauth refresh lock map poisoned");
    guard
        .entry(account_id)
        .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
        .clone()
}

static LOCAL_LOCKS: OnceLock<LockMap> = OnceLock::new();

async fn invalidate_runtime_account_cache(
    redis: &Redis,
    account_id: i64,
) -> redis::RedisResult<()> {
    let mut conn = redis.clone();
    conn.del::<_, ()>(runtime_account_key(account_id)).await
}

fn runtime_account_key(account_id: i64) -> String {
    format!("ai:ch:a:{account_id}")
}

fn merge_refreshed_account_extra(
    existing: &serde_json::Value,
    credentials: &OpenAiStoredCredentials,
    privacy_mode: Option<&str>,
) -> serde_json::Value {
    let mut merged = existing.as_object().cloned().unwrap_or_default();
    for (key, value) in build_stored_extra_overlay(credentials, privacy_mode) {
        merged.insert(key, value);
    }
    serde_json::Value::Object(merged)
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use summer_ai_core::oauth::openai::OpenAiStoredCredentials;

    use super::*;

    #[test]
    fn merge_refreshed_account_extra_overlays_oauth_summaries_and_privacy() {
        let credentials = OpenAiStoredCredentials {
            access_token: "at".into(),
            refresh_token: "rt".into(),
            id_token: "id".into(),
            expires_at: Utc::now(),
            client_id: "app_test".into(),
            email: Some("user@example.com".into()),
            chatgpt_account_id: None,
            chatgpt_user_id: None,
            organization_id: Some("org_1".into()),
            plan_type: Some("plus".into()),
            subscription_expires_at: None,
            token_version: Some(2),
            extra: serde_json::Map::new(),
        };

        let merged = merge_refreshed_account_extra(
            &serde_json::json!({
                "legacy": "kept",
                "privacy_mode": "training_set_failed"
            }),
            &credentials,
            Some("training_off"),
        );

        assert_eq!(merged["legacy"], "kept");
        assert_eq!(merged["oauth_provider"], "openai");
        assert_eq!(merged["oauth_email"], "user@example.com");
        assert_eq!(merged["oauth_plan_type"], "plus");
        assert_eq!(merged["oauth_organization_id"], "org_1");
        assert_eq!(merged["privacy_mode"], "training_off");
    }
}
