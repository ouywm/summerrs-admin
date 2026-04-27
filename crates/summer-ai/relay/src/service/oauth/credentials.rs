use std::time::Duration;

use chrono::Utc;
use serde_json::{Map, Value};
use summer_ai_core::oauth::openai::{OpenAiCredentialCodec, OpenAiStoredCredentials};
use summer_ai_model::entity::routing::channel_account;

use crate::error::RelayError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CredentialShape {
    Flat,
    NestedOauth,
}

#[derive(Clone, Debug)]
pub struct DecodedOpenAiCredentials {
    pub stored: OpenAiStoredCredentials,
    pub privacy_mode: Option<String>,
    shape: CredentialShape,
    outer: Map<String, Value>,
}

impl DecodedOpenAiCredentials {
    pub fn needs_refresh(&self, refresh_window: Duration) -> bool {
        needs_refresh(&self.stored, refresh_window)
    }

    pub fn encode(&self) -> Value {
        let codec = OpenAiCredentialCodec;
        let encoded = codec.encode_stored(&self.stored);
        match self.shape {
            CredentialShape::Flat => encoded,
            CredentialShape::NestedOauth => {
                let mut outer = self.outer.clone();
                outer.insert("oauth".into(), encoded);
                Value::Object(outer)
            }
        }
    }
}

pub fn decode_account_credentials(
    account: &channel_account::Model,
) -> Result<DecodedOpenAiCredentials, RelayError> {
    decode_credentials_value(&account.credentials)
}

pub fn needs_refresh(credentials: &OpenAiStoredCredentials, refresh_window: Duration) -> bool {
    let refresh_window =
        chrono::Duration::from_std(refresh_window).unwrap_or_else(|_| chrono::Duration::seconds(0));
    credentials.expires_at <= Utc::now() + refresh_window
}

pub fn next_token_version(current: Option<i64>) -> i64 {
    let now_ms = Utc::now().timestamp_millis();
    current
        .map(|current| current.saturating_add(1).max(now_ms))
        .unwrap_or(now_ms)
}

fn decode_credentials_value(value: &Value) -> Result<DecodedOpenAiCredentials, RelayError> {
    let codec = OpenAiCredentialCodec;
    let Some(object) = value.as_object() else {
        return Err(invalid_credentials(
            "credentials payload is not a JSON object",
        ));
    };

    let (shape, raw, outer) = match object.get("oauth") {
        Some(Value::Object(_)) => (
            CredentialShape::NestedOauth,
            object.get("oauth").unwrap_or(value),
            object.clone(),
        ),
        _ => (CredentialShape::Flat, value, Map::new()),
    };

    let stored = codec.decode(raw).map_err(|err| {
        tracing::warn!(error = %err, "failed to decode openai oauth credentials");
        invalid_credentials("openai oauth credentials decode failed")
    })?;

    Ok(DecodedOpenAiCredentials {
        stored,
        privacy_mode: None,
        shape,
        outer,
    })
}

fn invalid_credentials(_reason: &'static str) -> RelayError {
    RelayError::MissingConfig("invalid openai oauth credentials")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_flat_credentials_round_trips_known_fields() {
        let decoded = decode_credentials_value(&serde_json::json!({
            "access_token": "at",
            "refresh_token": "rt",
            "id_token": "id",
            "expires_at": "2026-04-26T18:00:00Z",
            "client_id": "app_test",
            "_token_version": 11,
            "provider_hint": "hosted"
        }))
        .expect("decode");

        assert_eq!(decoded.stored.access_token, "at");
        assert_eq!(decoded.stored.token_version, Some(11));
        assert_eq!(
            decoded.stored.extra.get("provider_hint"),
            Some(&Value::String("hosted".into()))
        );
        assert_eq!(decoded.encode()["provider_hint"], "hosted");
    }

    #[test]
    fn decode_nested_credentials_preserves_outer_shape() {
        let decoded = decode_credentials_value(&serde_json::json!({
            "oauth": {
                "access_token": "at",
                "refresh_token": "rt",
                "id_token": "id",
                "expires_at": "2026-04-26T18:00:00Z",
                "client_id": "app_test"
            },
            "label": "legacy"
        }))
        .expect("decode");

        let reencoded = decoded.encode();
        assert_eq!(reencoded["label"], "legacy");
        assert_eq!(reencoded["oauth"]["access_token"], "at");
    }
}
