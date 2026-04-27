use summer_ai_core::oauth::SessionStore;
use summer_ai_core::oauth::openai::{
    OpenAiCredentialCodec, OpenAiOAuthSession, OpenAiStoredCredentials, OpenAiTokenInfo,
    OpenAiTokenResponse, build_authorization_url, build_exchange_form, build_refresh_form,
    build_stored_extra_overlay, extract_access_token_organization_id, generate_code_challenge,
    generate_code_verifier, generate_session_id, generate_state, parse_chatgpt_account_info,
    should_skip_openai_privacy_ensure,
};

#[test]
fn build_authorization_url_includes_required_openai_flags() {
    let session_id = generate_session_id();
    assert!(!session_id.is_empty());

    let code_challenge = generate_code_challenge("challenge-1");
    assert!(!code_challenge.is_empty());

    let url = build_authorization_url(
        "state-1",
        &code_challenge,
        "http://localhost:1455/auth/callback",
        "app_test",
    )
    .expect("url");

    let raw = url.as_str();
    assert!(raw.contains("response_type=code"));
    assert!(raw.contains("client_id=app_test"));
    assert!(raw.contains("scope=openid+profile+email+offline_access"));
    assert!(raw.contains("code_challenge_method=S256"));
    assert!(raw.contains("code_challenge="));
    assert!(raw.contains("id_token_add_organizations=true"));
    assert!(raw.contains("codex_cli_simplified_flow=true"));
}

#[tokio::test(flavor = "current_thread")]
async fn session_store_expires_openai_sessions() {
    let store = SessionStore::new(std::time::Duration::from_millis(20));
    store
        .set(
            "session-1".into(),
            OpenAiOAuthSession {
                state: generate_state(),
                code_verifier: generate_code_verifier(),
                client_id: "app_test".into(),
                redirect_uri: "http://localhost:1455/auth/callback".into(),
                created_at: chrono::Utc::now(),
            },
        )
        .await;

    assert!(store.get("session-1").await.is_some());
    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    assert!(store.get("session-1").await.is_none());
}

#[test]
fn codec_round_trips_stored_credentials() {
    let codec = OpenAiCredentialCodec;
    let info = OpenAiTokenInfo {
        access_token: "at".into(),
        refresh_token: "rt".into(),
        id_token: "id".into(),
        expires_at: chrono::Utc::now(),
        client_id: "app_test".into(),
        email: Some("user@example.com".into()),
        chatgpt_account_id: Some("acc_1".into()),
        chatgpt_user_id: Some("user_1".into()),
        organization_id: Some("org_1".into()),
        plan_type: Some("plus".into()),
        subscription_expires_at: None,
    };

    let json = codec.encode(&info);
    let decoded = codec.decode(&json).expect("decode");
    assert_eq!(decoded.access_token, "at");
    assert_eq!(decoded.refresh_token, "rt");
    assert_eq!(decoded.client_id, "app_test");
    assert_eq!(decoded.token_version, None);
    assert!(decoded.extra.is_empty());
    assert_eq!(
        json.get("expires_at").and_then(|value| value.as_str()),
        Some(info.expires_at.to_rfc3339().as_str())
    );
}

#[test]
fn codec_encodes_subscription_expiry_as_rfc3339() {
    let codec = OpenAiCredentialCodec;
    let subscription_expires_at = chrono::DateTime::parse_from_rfc3339("2026-05-01T00:00:00Z")
        .expect("rfc3339")
        .with_timezone(&chrono::Utc);
    let info = OpenAiTokenInfo {
        access_token: "at".into(),
        refresh_token: "rt".into(),
        id_token: "id".into(),
        expires_at: chrono::DateTime::parse_from_rfc3339("2026-04-26T18:00:00Z")
            .expect("rfc3339")
            .with_timezone(&chrono::Utc),
        client_id: "app_test".into(),
        email: None,
        chatgpt_account_id: None,
        chatgpt_user_id: None,
        organization_id: None,
        plan_type: Some("plus".into()),
        subscription_expires_at: Some(subscription_expires_at),
    };

    let json = codec.encode(&info);
    assert_eq!(
        json.get("subscription_expires_at")
            .and_then(|value| value.as_str()),
        Some("2026-05-01T00:00:00+00:00")
    );
}

#[test]
fn build_refresh_form_uses_profile_email_scopes() {
    let form = build_refresh_form("app_test", "rt");
    assert_eq!(form.get("grant_type").unwrap(), "refresh_token");
    assert_eq!(form.get("client_id").unwrap(), "app_test");
    assert_eq!(form.get("scope").unwrap(), "openid profile email");
}

#[test]
fn build_exchange_form_matches_openai_authorization_code_shape() {
    let form = build_exchange_form(
        "code-1",
        "verifier-1",
        "http://localhost:1455/callback",
        "app_test",
    );
    assert_eq!(form.get("grant_type").unwrap(), "authorization_code");
    assert_eq!(form.get("code").unwrap(), "code-1");
    assert_eq!(form.get("code_verifier").unwrap(), "verifier-1");
    assert_eq!(
        form.get("redirect_uri").unwrap(),
        "http://localhost:1455/callback"
    );
    assert_eq!(form.get("client_id").unwrap(), "app_test");
    assert!(form.get("scope").is_none());
}

#[test]
fn refresh_response_merges_missing_rotating_fields_from_existing_credentials() {
    let existing = OpenAiStoredCredentials {
        access_token: "at-old".into(),
        refresh_token: "rt-old".into(),
        id_token: "id-old".into(),
        expires_at: chrono::DateTime::parse_from_rfc3339("2026-04-26T18:00:00Z")
            .expect("rfc3339")
            .with_timezone(&chrono::Utc),
        client_id: "app_test".into(),
        email: Some("user@example.com".into()),
        chatgpt_account_id: Some("acc_1".into()),
        chatgpt_user_id: Some("user_1".into()),
        organization_id: Some("org_1".into()),
        plan_type: Some("plus".into()),
        subscription_expires_at: None,
        token_version: Some(7),
        extra: serde_json::Map::from_iter([(
            "provider_hint".into(),
            serde_json::Value::String("hosted".into()),
        )]),
    };
    let refreshed_at = chrono::DateTime::parse_from_rfc3339("2026-04-26T19:00:00Z")
        .expect("rfc3339")
        .with_timezone(&chrono::Utc);
    let response = OpenAiTokenResponse {
        access_token: "at-new".into(),
        refresh_token: None,
        id_token: None,
        token_type: Some("Bearer".into()),
        scope: Some("openid profile email".into()),
        expires_in: Some(3600),
    };

    let merged = existing
        .merge_refresh_response(response, refreshed_at)
        .expect("merged");
    assert_eq!(merged.access_token, "at-new");
    assert_eq!(merged.refresh_token, "rt-old");
    assert_eq!(merged.id_token, "id-old");
    assert_eq!(merged.client_id, "app_test");
    assert_eq!(merged.email.as_deref(), Some("user@example.com"));
    assert_eq!(merged.token_version, Some(7));
    assert_eq!(
        merged.extra.get("provider_hint"),
        Some(&serde_json::Value::String("hosted".into()))
    );
    assert_eq!(
        merged.expires_at,
        refreshed_at + chrono::Duration::seconds(3600)
    );
}

#[test]
fn codec_preserves_token_version_and_unknown_fields() {
    let codec = OpenAiCredentialCodec;
    let value = serde_json::json!({
        "access_token": "at",
        "refresh_token": "rt",
        "id_token": "id",
        "expires_at": "2026-04-26T18:00:00Z",
        "client_id": "app_test",
        "_token_version": 123,
        "provider_hint": "hosted",
    });

    let decoded = codec.decode(&value).expect("decode");
    assert_eq!(decoded.token_version, Some(123));
    assert_eq!(
        decoded.extra.get("provider_hint"),
        Some(&serde_json::Value::String("hosted".into()))
    );

    let reencoded = codec.encode_stored(&decoded);
    assert_eq!(reencoded["_token_version"], 123);
    assert_eq!(reencoded["provider_hint"], "hosted");
}

#[test]
fn exchange_response_requires_id_token() {
    let exchanged_at = chrono::DateTime::parse_from_rfc3339("2026-04-26T19:00:00Z")
        .expect("rfc3339")
        .with_timezone(&chrono::Utc);
    let response = OpenAiTokenResponse {
        access_token: "at-new".into(),
        refresh_token: Some("rt-new".into()),
        id_token: None,
        token_type: Some("Bearer".into()),
        scope: Some("openid profile email offline_access".into()),
        expires_in: Some(3600),
    };

    let err = OpenAiStoredCredentials::from_exchange_response(response, "app_test", exchanged_at)
        .expect_err("missing id token should fail");
    assert_eq!(
        err.to_string(),
        "openai oauth token response is missing required field `id_token`"
    );
}

#[test]
fn parse_chatgpt_account_info_prefers_matching_org_id() {
    let payload = serde_json::json!({
        "accounts": {
            "org_default": {
                "account": {
                    "plan_type": "free",
                    "is_default": true,
                    "email": "default@example.com"
                },
                "entitlement": {
                    "expires_at": "2026-05-01T00:00:00+00:00"
                }
            },
            "org_target": {
                "account": {
                    "plan_type": "plus",
                    "is_default": false,
                    "email": "target@example.com"
                },
                "entitlement": {
                    "expires_at": "2026-05-02T20:32:12+00:00"
                }
            }
        }
    });

    let info = parse_chatgpt_account_info(&payload, Some("org_target")).expect("account info");
    assert_eq!(info.plan_type.as_deref(), Some("plus"));
    assert_eq!(info.email.as_deref(), Some("target@example.com"));
    assert_eq!(
        info.subscription_expires_at,
        Some(
            chrono::DateTime::parse_from_rfc3339("2026-05-02T20:32:12+00:00")
                .expect("rfc3339")
                .with_timezone(&chrono::Utc)
        )
    );
}

#[test]
fn extract_access_token_organization_id_reads_poid() {
    let access_token = fake_jwt(serde_json::json!({
        "https://api.openai.com/auth": {
            "poid": "org_from_access_token"
        }
    }));

    assert_eq!(
        extract_access_token_organization_id(&access_token).as_deref(),
        Some("org_from_access_token")
    );
}

#[test]
fn build_stored_extra_overlay_includes_privacy_mode_and_summaries() {
    let credentials = OpenAiStoredCredentials {
        access_token: "at".into(),
        refresh_token: "rt".into(),
        id_token: "id".into(),
        expires_at: chrono::DateTime::parse_from_rfc3339("2026-04-26T18:00:00Z")
            .expect("rfc3339")
            .with_timezone(&chrono::Utc),
        client_id: "app_test".into(),
        email: Some("user@example.com".into()),
        chatgpt_account_id: Some("acc_1".into()),
        chatgpt_user_id: Some("user_1".into()),
        organization_id: Some("org_1".into()),
        plan_type: Some("plus".into()),
        subscription_expires_at: None,
        token_version: Some(1),
        extra: serde_json::Map::new(),
    };

    let extra = build_stored_extra_overlay(&credentials, Some("training_off"));
    assert_eq!(
        extra.get("oauth_provider"),
        Some(&serde_json::json!("openai"))
    );
    assert_eq!(
        extra.get("oauth_email"),
        Some(&serde_json::json!("user@example.com"))
    );
    assert_eq!(
        extra.get("oauth_plan_type"),
        Some(&serde_json::json!("plus"))
    );
    assert_eq!(
        extra.get("oauth_organization_id"),
        Some(&serde_json::json!("org_1"))
    );
    assert_eq!(
        extra.get("privacy_mode"),
        Some(&serde_json::json!("training_off"))
    );
}

#[test]
fn should_skip_openai_privacy_ensure_retries_only_failed_states() {
    assert!(!should_skip_openai_privacy_ensure(&serde_json::json!({})));
    assert!(!should_skip_openai_privacy_ensure(&serde_json::json!({
        "privacy_mode": "training_set_failed"
    })));
    assert!(!should_skip_openai_privacy_ensure(&serde_json::json!({
        "privacy_mode": "training_set_cf_blocked"
    })));
    assert!(should_skip_openai_privacy_ensure(&serde_json::json!({
        "privacy_mode": "training_off"
    })));
}

fn fake_jwt(payload: serde_json::Value) -> String {
    use base64::Engine;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;

    let header = URL_SAFE_NO_PAD.encode(r#"{"alg":"none","typ":"JWT"}"#);
    let payload = URL_SAFE_NO_PAD.encode(payload.to_string());
    format!("{header}.{payload}.signature")
}
