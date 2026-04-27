use chrono::{DateTime, Utc};
use serde_json::{Map, Value, json};

use super::types::{CodecError, OpenAiStoredCredentials, OpenAiTokenInfo};

const KNOWN_FIELDS: &[&str] = &[
    "access_token",
    "refresh_token",
    "id_token",
    "expires_at",
    "client_id",
    "email",
    "chatgpt_account_id",
    "chatgpt_user_id",
    "organization_id",
    "plan_type",
    "subscription_expires_at",
    "_token_version",
];

#[derive(Default)]
pub struct OpenAiCredentialCodec;

impl OpenAiCredentialCodec {
    pub fn encode(&self, info: &OpenAiTokenInfo) -> Value {
        self.encode_stored(&OpenAiStoredCredentials::from(info.clone()))
    }

    pub fn encode_stored(&self, info: &OpenAiStoredCredentials) -> Value {
        let mut value = info
            .extra
            .iter()
            .filter(|(key, _)| !KNOWN_FIELDS.contains(&key.as_str()))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect::<Map<String, Value>>();
        value.insert(
            "access_token".into(),
            Value::String(info.access_token.clone()),
        );
        value.insert(
            "refresh_token".into(),
            Value::String(info.refresh_token.clone()),
        );
        value.insert("id_token".into(), Value::String(info.id_token.clone()));
        value.insert(
            "expires_at".into(),
            Value::String(info.expires_at.to_rfc3339()),
        );
        value.insert("client_id".into(), Value::String(info.client_id.clone()));
        value.insert("email".into(), opt_string(&info.email));
        value.insert(
            "chatgpt_account_id".into(),
            opt_string(&info.chatgpt_account_id),
        );
        value.insert("chatgpt_user_id".into(), opt_string(&info.chatgpt_user_id));
        value.insert("organization_id".into(), opt_string(&info.organization_id));
        value.insert("plan_type".into(), opt_string(&info.plan_type));
        value.insert(
            "subscription_expires_at".into(),
            info.subscription_expires_at
                .map(|ts| json!(ts.to_rfc3339()))
                .unwrap_or(Value::Null),
        );
        value.insert(
            "_token_version".into(),
            info.token_version.map(Value::from).unwrap_or(Value::Null),
        );
        Value::Object(value)
    }

    pub fn decode(&self, value: &Value) -> Result<OpenAiStoredCredentials, CodecError> {
        let object = value.as_object().ok_or(CodecError::InvalidType)?;
        Ok(OpenAiStoredCredentials {
            access_token: required_string(object, "access_token")?,
            refresh_token: required_string(object, "refresh_token")?,
            id_token: required_string(object, "id_token")?,
            expires_at: required_timestamp(object, "expires_at")?,
            client_id: required_string(object, "client_id")?,
            email: optional_string(object, "email")?,
            chatgpt_account_id: optional_string(object, "chatgpt_account_id")?,
            chatgpt_user_id: optional_string(object, "chatgpt_user_id")?,
            organization_id: optional_string(object, "organization_id")?,
            plan_type: optional_string(object, "plan_type")?,
            subscription_expires_at: optional_timestamp(object, "subscription_expires_at")?,
            token_version: optional_i64(object, "_token_version")?,
            extra: extra_fields(object),
        })
    }
}

fn opt_string(value: &Option<String>) -> Value {
    value
        .as_ref()
        .map(|v| Value::String(v.clone()))
        .unwrap_or(Value::Null)
}

fn required_string(object: &Map<String, Value>, field: &'static str) -> Result<String, CodecError> {
    object
        .get(field)
        .ok_or(CodecError::MissingField(field))
        .and_then(|value| match value {
            Value::String(s) => Ok(s.clone()),
            _ => Err(CodecError::InvalidString { field }),
        })
}

fn optional_string(
    object: &Map<String, Value>,
    field: &'static str,
) -> Result<Option<String>, CodecError> {
    match object.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(CodecError::InvalidString { field }),
    }
}

fn required_timestamp(
    object: &Map<String, Value>,
    field: &'static str,
) -> Result<DateTime<Utc>, CodecError> {
    let raw = required_string(object, field)?;
    parse_timestamp(field, &raw)
}

fn optional_timestamp(
    object: &Map<String, Value>,
    field: &'static str,
) -> Result<Option<DateTime<Utc>>, CodecError> {
    match object.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(raw)) => parse_timestamp(field, raw).map(Some),
        Some(_) => Err(CodecError::InvalidString { field }),
    }
}

fn optional_i64(
    object: &Map<String, Value>,
    field: &'static str,
) -> Result<Option<i64>, CodecError> {
    match object.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(number)) => number
            .as_i64()
            .map(Some)
            .ok_or(CodecError::InvalidInteger { field }),
        Some(_) => Err(CodecError::InvalidInteger { field }),
    }
}

fn parse_timestamp(field: &'static str, raw: &str) -> Result<DateTime<Utc>, CodecError> {
    DateTime::parse_from_rfc3339(raw)
        .map(|ts| ts.with_timezone(&Utc))
        .map_err(|source| CodecError::InvalidTimestamp { field, source })
}

fn extra_fields(object: &Map<String, Value>) -> Map<String, Value> {
    object
        .iter()
        .filter(|(key, _)| !KNOWN_FIELDS.contains(&key.as_str()))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}
