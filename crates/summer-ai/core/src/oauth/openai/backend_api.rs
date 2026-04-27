use base64::Engine;
use base64::engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use reqwest::StatusCode;
use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::types::OpenAiStoredCredentials;

const CHATGPT_ACCOUNTS_CHECK_URL: &str =
    "https://chatgpt.com/backend-api/accounts/check/v4-2023-04-27";
const CHATGPT_SETTINGS_URL: &str = "https://chatgpt.com/backend-api/settings/account_user_setting?feature=training_allowed&value=false";
const REQUEST_TIMEOUT_SECS: u64 = 15;

pub const OPENAI_PRIVACY_MODE_TRAINING_OFF: &str = "training_off";
pub const OPENAI_PRIVACY_MODE_TRAINING_SET_FAILED: &str = "training_set_failed";
pub const OPENAI_PRIVACY_MODE_TRAINING_SET_CF_BLOCKED: &str = "training_set_cf_blocked";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OpenAiChatGptAccountInfo {
    pub plan_type: Option<String>,
    pub email: Option<String>,
    pub subscription_expires_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OpenAiBackendEnrichment {
    pub privacy_mode: Option<String>,
}

pub async fn enrich_stored_credentials(
    http: &reqwest::Client,
    credentials: &mut OpenAiStoredCredentials,
    skip_privacy_ensure: bool,
) -> OpenAiBackendEnrichment {
    let access_token = credentials.access_token.trim();
    if access_token.is_empty() {
        return OpenAiBackendEnrichment::default();
    }

    let organization_id = credentials
        .organization_id
        .as_deref()
        .and_then(non_empty_ref)
        .map(str::to_string)
        .or_else(|| extract_access_token_organization_id(access_token));

    if let Some(info) =
        fetch_chatgpt_account_info(http, access_token, organization_id.as_deref()).await
    {
        if credentials
            .email
            .as_deref()
            .and_then(non_empty_ref)
            .is_none()
        {
            credentials.email = info.email;
        }
        if let Some(plan_type) = info.plan_type {
            credentials.plan_type = Some(plan_type);
        }
        if let Some(subscription_expires_at) = info.subscription_expires_at {
            credentials.subscription_expires_at = Some(subscription_expires_at);
        }
    }

    let privacy_mode = if skip_privacy_ensure {
        None
    } else {
        disable_openai_training(http, access_token).await
    };

    OpenAiBackendEnrichment {
        privacy_mode: privacy_mode.map(str::to_string),
    }
}

pub fn build_stored_extra_overlay(
    credentials: &OpenAiStoredCredentials,
    privacy_mode: Option<&str>,
) -> Map<String, Value> {
    let mut extra = Map::new();
    extra.insert("oauth_provider".into(), json!("openai"));
    if let Some(email) = credentials.email.as_deref().and_then(non_empty_ref) {
        extra.insert("oauth_email".into(), json!(email));
    }
    if let Some(plan_type) = credentials.plan_type.as_deref().and_then(non_empty_ref) {
        extra.insert("oauth_plan_type".into(), json!(plan_type));
    }
    if let Some(org_id) = credentials
        .organization_id
        .as_deref()
        .and_then(non_empty_ref)
    {
        extra.insert("oauth_organization_id".into(), json!(org_id));
    }
    if let Some(mode) = privacy_mode.and_then(non_empty_ref) {
        extra.insert("privacy_mode".into(), json!(mode));
    }
    extra
}

pub fn should_skip_openai_privacy_ensure(extra: &Value) -> bool {
    let Some(mode) = extra
        .get("privacy_mode")
        .and_then(|value| value.as_str())
        .and_then(non_empty_ref)
    else {
        return false;
    };

    mode != OPENAI_PRIVACY_MODE_TRAINING_SET_FAILED
        && mode != OPENAI_PRIVACY_MODE_TRAINING_SET_CF_BLOCKED
}

pub fn parse_chatgpt_account_info(
    value: &Value,
    organization_id: Option<&str>,
) -> Option<OpenAiChatGptAccountInfo> {
    let accounts = value.get("accounts")?.as_object()?;

    if let Some(org_id) = organization_id.and_then(non_empty_ref) {
        if let Some(account) = accounts.get(org_id).and_then(|value| value.as_object()) {
            let info = extract_chatgpt_account_info(account);
            if info.plan_type.is_some()
                || info.subscription_expires_at.is_some()
                || info.email.is_some()
            {
                return Some(info);
            }
        }
    }

    let mut default_info = None;
    let mut paid_info = None;
    let mut any_info = None;

    for account in accounts.values().filter_map(|value| value.as_object()) {
        let info = extract_chatgpt_account_info(account);
        if info.plan_type.is_none() {
            continue;
        }

        if any_info.is_none() {
            any_info = Some(info.clone());
        }
        if is_default_chatgpt_account(account) && default_info.is_none() {
            default_info = Some(info.clone());
        }
        if !matches!(info.plan_type.as_deref(), Some("free")) && paid_info.is_none() {
            paid_info = Some(info);
        }
    }

    default_info.or(paid_info).or(any_info)
}

pub fn extract_access_token_organization_id(access_token: &str) -> Option<String> {
    let payload = access_token.split('.').nth(1)?;
    let decoded = decode_base64url(payload).ok()?;
    let claims = serde_json::from_slice::<OpenAiAccessTokenClaims>(&decoded).ok()?;
    claims
        .openai_auth
        .and_then(|auth| auth.poid)
        .and_then(non_empty_owned)
}

async fn fetch_chatgpt_account_info(
    http: &reqwest::Client,
    access_token: &str,
    organization_id: Option<&str>,
) -> Option<OpenAiChatGptAccountInfo> {
    let response = http
        .get(CHATGPT_ACCOUNTS_CHECK_URL)
        .header(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {access_token}"),
        )
        .header(reqwest::header::ORIGIN, "https://chatgpt.com")
        .header(reqwest::header::REFERER, "https://chatgpt.com/")
        .header(reqwest::header::ACCEPT, "application/json")
        .timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .send()
        .await;

    let Ok(response) = response else {
        tracing::debug!("chatgpt account check request failed");
        return None;
    };

    if !response.status().is_success() {
        tracing::debug!(status = %response.status(), "chatgpt account check failed");
        return None;
    }

    let Ok(value) = response.json::<Value>().await else {
        tracing::debug!("chatgpt account check decode failed");
        return None;
    };

    parse_chatgpt_account_info(&value, organization_id)
}

async fn disable_openai_training(
    http: &reqwest::Client,
    access_token: &str,
) -> Option<&'static str> {
    let response = http
        .patch(CHATGPT_SETTINGS_URL)
        .header(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {access_token}"),
        )
        .header(reqwest::header::ORIGIN, "https://chatgpt.com")
        .header(reqwest::header::REFERER, "https://chatgpt.com/")
        .header(reqwest::header::ACCEPT, "application/json")
        .header("sec-fetch-mode", "cors")
        .header("sec-fetch-site", "same-origin")
        .header("sec-fetch-dest", "empty")
        .timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .send()
        .await;

    let Ok(response) = response else {
        tracing::warn!("openai privacy request failed");
        return Some(OPENAI_PRIVACY_MODE_TRAINING_SET_FAILED);
    };

    let status = response.status();
    let Ok(body) = response.text().await else {
        tracing::warn!("openai privacy response body failed");
        return Some(OPENAI_PRIVACY_MODE_TRAINING_SET_FAILED);
    };

    Some(classify_training_opt_out_response(status, &body))
}

fn classify_training_opt_out_response(status: StatusCode, body: &str) -> &'static str {
    if status.is_success() {
        return OPENAI_PRIVACY_MODE_TRAINING_OFF;
    }

    if matches!(
        status,
        StatusCode::FORBIDDEN | StatusCode::SERVICE_UNAVAILABLE
    ) {
        let body_lower = body.to_ascii_lowercase();
        if body_lower.contains("cloudflare")
            || body_lower.contains("cf-")
            || body_lower.contains("just a moment")
        {
            return OPENAI_PRIVACY_MODE_TRAINING_SET_CF_BLOCKED;
        }
    }

    OPENAI_PRIVACY_MODE_TRAINING_SET_FAILED
}

fn extract_chatgpt_account_info(account: &Map<String, Value>) -> OpenAiChatGptAccountInfo {
    OpenAiChatGptAccountInfo {
        plan_type: extract_plan_type(account),
        email: extract_email(account),
        subscription_expires_at: extract_entitlement_expires_at(account),
    }
}

fn is_default_chatgpt_account(account: &Map<String, Value>) -> bool {
    account
        .get("account")
        .and_then(|value| value.as_object())
        .and_then(|value| value.get("is_default"))
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
}

fn extract_plan_type(account: &Map<String, Value>) -> Option<String> {
    account
        .get("account")
        .and_then(|value| value.as_object())
        .and_then(|value| value.get("plan_type"))
        .and_then(|value| value.as_str())
        .and_then(non_empty_ref)
        .map(str::to_string)
        .or_else(|| {
            account
                .get("entitlement")
                .and_then(|value| value.as_object())
                .and_then(|value| value.get("subscription_plan"))
                .and_then(|value| value.as_str())
                .and_then(non_empty_ref)
                .map(str::to_string)
        })
}

fn extract_email(account: &Map<String, Value>) -> Option<String> {
    account
        .get("account")
        .and_then(|value| value.as_object())
        .and_then(|value| value.get("email"))
        .and_then(|value| value.as_str())
        .and_then(non_empty_ref)
        .map(str::to_string)
        .or_else(|| {
            account
                .get("account")
                .and_then(|value| value.as_object())
                .and_then(|value| value.get("user"))
                .and_then(|value| value.as_object())
                .and_then(|value| value.get("email"))
                .and_then(|value| value.as_str())
                .and_then(non_empty_ref)
                .map(str::to_string)
        })
}

fn extract_entitlement_expires_at(account: &Map<String, Value>) -> Option<DateTime<Utc>> {
    let raw = account
        .get("entitlement")
        .and_then(|value| value.as_object())
        .and_then(|value| value.get("expires_at"))
        .and_then(|value| value.as_str())
        .and_then(non_empty_ref)?;
    DateTime::parse_from_rfc3339(raw)
        .map(|value| value.with_timezone(&Utc))
        .ok()
}

fn decode_base64url(input: &str) -> Result<Vec<u8>, base64::DecodeError> {
    URL_SAFE_NO_PAD
        .decode(input)
        .or_else(|_| URL_SAFE.decode(input))
}

fn non_empty_ref(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn non_empty_owned(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[derive(Debug, Deserialize)]
struct OpenAiAccessTokenClaims {
    #[serde(default, rename = "https://api.openai.com/auth")]
    openai_auth: Option<OpenAiAccessTokenAuthClaims>,
}

#[derive(Debug, Deserialize)]
struct OpenAiAccessTokenAuthClaims {
    #[serde(default)]
    poid: Option<String>,
}
