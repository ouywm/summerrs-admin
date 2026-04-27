use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub type OpenAiForm = BTreeMap<String, String>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenAiTokenInfo {
    pub access_token: String,
    pub refresh_token: String,
    pub id_token: String,
    pub expires_at: DateTime<Utc>,
    pub client_id: String,
    pub email: Option<String>,
    pub chatgpt_account_id: Option<String>,
    pub chatgpt_user_id: Option<String>,
    pub organization_id: Option<String>,
    pub plan_type: Option<String>,
    pub subscription_expires_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenAiStoredCredentials {
    pub access_token: String,
    pub refresh_token: String,
    pub id_token: String,
    pub expires_at: DateTime<Utc>,
    pub client_id: String,
    pub email: Option<String>,
    pub chatgpt_account_id: Option<String>,
    pub chatgpt_user_id: Option<String>,
    pub organization_id: Option<String>,
    pub plan_type: Option<String>,
    pub subscription_expires_at: Option<DateTime<Utc>>,
    pub token_version: Option<i64>,
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl OpenAiStoredCredentials {
    pub fn from_exchange_response(
        response: OpenAiTokenResponse,
        client_id: impl Into<String>,
        exchanged_at: DateTime<Utc>,
    ) -> Result<Self, TokenNormalizationError> {
        Self::normalize(response, client_id.into(), None, exchanged_at, true)
    }

    pub fn merge_refresh_response(
        &self,
        response: OpenAiTokenResponse,
        refreshed_at: DateTime<Utc>,
    ) -> Result<Self, TokenNormalizationError> {
        Self::normalize(
            response,
            self.client_id.clone(),
            Some(self),
            refreshed_at,
            false,
        )
    }

    fn normalize(
        response: OpenAiTokenResponse,
        client_id: String,
        existing: Option<&Self>,
        issued_at: DateTime<Utc>,
        require_id_token: bool,
    ) -> Result<Self, TokenNormalizationError> {
        let access_token =
            required_non_empty("access_token", Some(response.access_token.as_str()))?;
        let refresh_token = response
            .refresh_token
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .map(ToOwned::to_owned)
            .or_else(|| existing.map(|credentials| credentials.refresh_token.clone()))
            .ok_or(TokenNormalizationError::MissingField("refresh_token"))?;
        let id_token = response
            .id_token
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .map(ToOwned::to_owned)
            .or_else(|| existing.map(|credentials| credentials.id_token.clone()));
        let id_token = if require_id_token {
            id_token.ok_or(TokenNormalizationError::MissingField("id_token"))?
        } else {
            id_token.unwrap_or_default()
        };
        let client_id = required_non_empty("client_id", Some(client_id.as_str()))?;
        let expires_in = response
            .expires_in
            .ok_or(TokenNormalizationError::MissingField("expires_in"))?;
        let expires_at = issued_at + chrono::Duration::seconds(expires_in.max(0));

        let mut normalized = existing.cloned().unwrap_or(Self {
            access_token: String::new(),
            refresh_token: String::new(),
            id_token: String::new(),
            expires_at,
            client_id: client_id.clone(),
            email: None,
            chatgpt_account_id: None,
            chatgpt_user_id: None,
            organization_id: None,
            plan_type: None,
            subscription_expires_at: None,
            token_version: None,
            extra: serde_json::Map::new(),
        });
        normalized.access_token = access_token;
        normalized.refresh_token = refresh_token;
        normalized.id_token = id_token;
        normalized.expires_at = expires_at;
        normalized.client_id = client_id;
        Ok(normalized)
    }
}

impl From<OpenAiTokenInfo> for OpenAiStoredCredentials {
    fn from(value: OpenAiTokenInfo) -> Self {
        Self {
            access_token: value.access_token,
            refresh_token: value.refresh_token,
            id_token: value.id_token,
            expires_at: value.expires_at,
            client_id: value.client_id,
            email: value.email,
            chatgpt_account_id: value.chatgpt_account_id,
            chatgpt_user_id: value.chatgpt_user_id,
            organization_id: value.organization_id,
            plan_type: value.plan_type,
            subscription_expires_at: value.subscription_expires_at,
            token_version: None,
            extra: serde_json::Map::new(),
        }
    }
}

impl From<OpenAiStoredCredentials> for OpenAiTokenInfo {
    fn from(value: OpenAiStoredCredentials) -> Self {
        Self {
            access_token: value.access_token,
            refresh_token: value.refresh_token,
            id_token: value.id_token,
            expires_at: value.expires_at,
            client_id: value.client_id,
            email: value.email,
            chatgpt_account_id: value.chatgpt_account_id,
            chatgpt_user_id: value.chatgpt_user_id,
            organization_id: value.organization_id,
            plan_type: value.plan_type,
            subscription_expires_at: value.subscription_expires_at,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct OpenAiTokenResponse {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub id_token: Option<String>,
    #[serde(default)]
    pub token_type: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub expires_in: Option<i64>,
}

#[derive(Debug, Error)]
pub enum CodecError {
    #[error("openai oauth credentials must be a JSON object")]
    InvalidType,
    #[error("missing field `{0}`")]
    MissingField(&'static str),
    #[error("field `{field}` must be a string")]
    InvalidString { field: &'static str },
    #[error("field `{field}` must be an integer")]
    InvalidInteger { field: &'static str },
    #[error("field `{field}` must be an RFC3339 timestamp: {source}")]
    InvalidTimestamp {
        field: &'static str,
        source: chrono::ParseError,
    },
}

#[derive(Debug, Error)]
pub enum TokenNormalizationError {
    #[error("openai oauth token response is missing required field `{0}`")]
    MissingField(&'static str),
}

#[derive(Debug, Error)]
pub enum OpenAiOAuthError {
    #[error("openai oauth transport failed: {0}")]
    Transport(#[source] reqwest::Error),
    #[error("openai oauth response decode failed: {source}; context: {context}")]
    Decode {
        #[source]
        source: serde_json::Error,
        context: String,
    },
    #[error("openai oauth returned {status}: {body}")]
    HttpStatus {
        status: reqwest::StatusCode,
        body: String,
        provider_error: Option<OpenAiOAuthErrorResponse>,
    },
}

impl OpenAiOAuthError {
    pub fn provider_error_code(&self) -> Option<&str> {
        match self {
            Self::HttpStatus { provider_error, .. } => provider_error
                .as_ref()
                .and_then(|error| error.error.as_deref()),
            _ => None,
        }
    }

    pub fn is_invalid_grant(&self) -> bool {
        self.provider_error_code() == Some("invalid_grant")
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct OpenAiOAuthErrorResponse {
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub error_description: Option<String>,
    #[serde(flatten, default)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

fn required_non_empty(
    field: &'static str,
    value: Option<&str>,
) -> Result<String, TokenNormalizationError> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or(TokenNormalizationError::MissingField(field))
}
