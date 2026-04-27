use std::time::Duration;

use summer_ai_core::AuthData;
use summer_ai_model::entity::routing::channel_account;
use summer_redis::Redis;
use summer_sea_orm::DbConn;

use crate::error::RelayError;
use crate::service::oauth::credentials::decode_account_credentials;
use crate::service::oauth::refresh_api::refresh_if_needed;
use crate::service::oauth::token_refresher::OpenAiTokenRefresher;

const OPENAI_REFRESH_WINDOW: Duration = Duration::from_secs(180);

pub struct OpenAiTokenProvider<'a> {
    db: &'a DbConn,
    redis: &'a Redis,
    http: &'a reqwest::Client,
    refresh_window: Duration,
}

impl<'a> OpenAiTokenProvider<'a> {
    pub fn new(db: &'a DbConn, redis: &'a Redis, http: &'a reqwest::Client) -> Self {
        Self {
            db,
            redis,
            http,
            refresh_window: OPENAI_REFRESH_WINDOW,
        }
    }

    pub async fn auth_data_for_account(
        &self,
        account: &channel_account::Model,
    ) -> Result<AuthData, RelayError> {
        let account = if OpenAiTokenRefresher::needs_refresh(account, self.refresh_window)? {
            let outcome =
                refresh_if_needed(self.db, self.redis, self.http, account, self.refresh_window)
                    .await?;
            if outcome.refreshed {
                tracing::debug!(
                    account_id = outcome.account.id,
                    "openai oauth token refreshed"
                );
            }
            outcome.account
        } else {
            account.clone()
        };

        let decoded = decode_account_credentials(&account)?;
        let token = decoded.stored.access_token.trim();
        if token.is_empty() {
            return Err(RelayError::MissingConfig(
                "openai oauth access_token is empty",
            ));
        }
        Ok(AuthData::from_single(token.to_string()))
    }
}
