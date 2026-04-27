use bytes::Bytes;
use std::time::Duration;

use chrono::Utc;
use summer_ai_core::{
    AdapterError,
    oauth::openai::{
        OpenAiOAuthError, enrich_stored_credentials, refresh_token,
        should_skip_openai_privacy_ensure,
    },
};
use summer_ai_model::entity::routing::channel_account;

use crate::error::RelayError;
use crate::service::oauth::credentials::{DecodedOpenAiCredentials, decode_account_credentials};

pub struct OpenAiTokenRefresher<'a> {
    http: &'a reqwest::Client,
}

impl<'a> OpenAiTokenRefresher<'a> {
    pub fn new(http: &'a reqwest::Client) -> Self {
        Self { http }
    }

    pub fn needs_refresh(
        account: &channel_account::Model,
        refresh_window: Duration,
    ) -> Result<bool, RelayError> {
        Ok(decode_account_credentials(account)?.needs_refresh(refresh_window))
    }

    pub async fn refresh(
        &self,
        account: &channel_account::Model,
    ) -> Result<DecodedOpenAiCredentials, RelayError> {
        let mut decoded = decode_account_credentials(account)?;
        let response = refresh_token(
            self.http,
            &decoded.stored.client_id,
            &decoded.stored.refresh_token,
        )
        .await
        .map_err(map_refresh_error)?;

        decoded.stored = decoded
            .stored
            .merge_refresh_response(response, Utc::now())
            .map_err(|err| {
                tracing::warn!(error = %err, "invalid openai oauth refresh response");
                RelayError::MissingConfig("invalid openai oauth refresh response")
            })?;
        let enrichment = enrich_stored_credentials(
            self.http,
            &mut decoded.stored,
            should_skip_openai_privacy_ensure(&account.extra),
        )
        .await;
        decoded.privacy_mode = enrichment.privacy_mode;
        Ok(decoded)
    }
}

fn map_refresh_error(err: OpenAiOAuthError) -> RelayError {
    match err {
        OpenAiOAuthError::Transport(err) => RelayError::Http(err),
        OpenAiOAuthError::HttpStatus { status, body, .. } => RelayError::UpstreamStatus {
            status: status.as_u16(),
            body: Bytes::from(body),
        },
        OpenAiOAuthError::Decode { source, context } => {
            tracing::warn!(error = %source, context, "failed to decode openai oauth refresh body");
            RelayError::Adapter(AdapterError::DeserializeResponse(source))
        }
    }
}
