use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::Utc;
use summer_ai_admin::service::openai_oauth_service::{
    build_oauth_account_payload, decode_openai_id_token_profile, ensure_openai_channel_type,
};
use summer_ai_core::oauth::openai::OpenAiStoredCredentials;
use summer_ai_model::entity::routing::channel;

#[test]
fn ensure_openai_channel_type_rejects_non_openai_channels() {
    let err = ensure_openai_channel_type(channel::ChannelType::Gemini).unwrap_err();
    assert!(err.contains("OpenAI"));
}

#[test]
fn decode_openai_id_token_profile_prefers_default_organization() {
    let token = fake_jwt(serde_json::json!({
        "email": "user@example.com",
        "https://api.openai.com/auth": {
            "chatgpt_account_id": "acc_1",
            "chatgpt_user_id": "user_1",
            "chatgpt_plan_type": "plus",
            "organizations": [
                {"id": "org_other", "is_default": false},
                {"id": "org_default", "is_default": true}
            ]
        }
    }));

    let profile = decode_openai_id_token_profile(&token).expect("profile");
    assert_eq!(profile.email.as_deref(), Some("user@example.com"));
    assert_eq!(profile.chatgpt_account_id.as_deref(), Some("acc_1"));
    assert_eq!(profile.chatgpt_user_id.as_deref(), Some("user_1"));
    assert_eq!(profile.organization_id.as_deref(), Some("org_default"));
    assert_eq!(profile.plan_type.as_deref(), Some("plus"));
}

#[test]
fn build_oauth_account_payload_sets_oauth_credential_type_and_expiry() {
    let now = Utc::now();
    let mut credential_extra = serde_json::Map::new();
    credential_extra.insert("provider_hint".into(), serde_json::json!("hosted"));
    let credentials = OpenAiStoredCredentials {
        access_token: "at".into(),
        refresh_token: "rt".into(),
        id_token: "id".into(),
        expires_at: now,
        client_id: "app_test".into(),
        email: Some("user@example.com".into()),
        chatgpt_account_id: Some("acc_1".into()),
        chatgpt_user_id: Some("user_1".into()),
        organization_id: Some("org_1".into()),
        plan_type: Some("plus".into()),
        subscription_expires_at: None,
        token_version: Some(2),
        extra: credential_extra,
    };

    let payload = build_oauth_account_payload(&credentials);
    assert_eq!(payload.credential_type, "oauth");
    assert_eq!(payload.credentials["access_token"], "at");
    assert_eq!(payload.credentials["refresh_token"], "rt");
    assert_eq!(payload.credentials["client_id"], "app_test");
    assert_eq!(payload.credentials["provider_hint"], "hosted");
    assert_eq!(payload.extra["oauth_provider"], "openai");
    assert!(payload.extra.get("provider_hint").is_none());
    assert_eq!(payload.expires_at, Some(now.fixed_offset()));
}

fn fake_jwt(payload: serde_json::Value) -> String {
    let header = URL_SAFE_NO_PAD.encode(r#"{"alg":"none","typ":"JWT"}"#);
    let payload = URL_SAFE_NO_PAD.encode(payload.to_string());
    format!("{header}.{payload}.signature")
}
