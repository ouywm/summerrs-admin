use std::time::Duration;

use chrono::{Duration as ChronoDuration, Utc};
use sea_orm::prelude::BigDecimal;
use summer_ai_core::{AuthData, EndpointScope, oauth::openai::OpenAiStoredCredentials};
use summer_ai_model::entity::routing::{channel, channel_account};
use summer_ai_relay::service::{
    channel_store::build_service_target_with_auth, oauth::credentials::needs_refresh,
};

fn test_openai_channel() -> channel::Model {
    channel::Model {
        id: 1,
        name: "openai".into(),
        channel_type: channel::ChannelType::OpenAi,
        vendor_code: "openai".into(),
        base_url: "https://api.openai.com/v1".into(),
        status: channel::ChannelStatus::Enabled,
        models: serde_json::json!(["gpt-4.1"]),
        model_mapping: serde_json::json!({}),
        channel_group: String::new(),
        endpoint_scopes: serde_json::json!(["chat"]),
        capabilities: serde_json::json!([]),
        weight: 1,
        priority: 1,
        config: serde_json::json!({}),
        auto_ban: false,
        test_model: String::new(),
        used_quota: 0,
        balance: BigDecimal::from(0),
        balance_updated_at: None,
        response_time: 0,
        success_rate: BigDecimal::from(0),
        failure_streak: 0,
        last_used_at: None,
        last_error_at: None,
        last_error_code: String::new(),
        last_error_message: String::new(),
        last_health_status: channel::ChannelLastHealthStatus::Unknown,
        deleted_at: None,
        remark: String::new(),
        create_by: String::new(),
        create_time: Utc::now().fixed_offset(),
        update_by: String::new(),
        update_time: Utc::now().fixed_offset(),
    }
}

fn test_oauth_account() -> channel_account::Model {
    channel_account::Model {
        id: 7,
        channel_id: 1,
        name: "oauth".into(),
        credential_type: "oauth".into(),
        credentials: serde_json::json!({
            "access_token": "at",
            "refresh_token": "rt",
            "id_token": "id",
            "expires_at": "2026-04-26T18:00:00Z",
            "client_id": "app_test"
        }),
        secret_ref: String::new(),
        status: channel_account::ChannelAccountStatus::Enabled,
        schedulable: true,
        priority: 1,
        weight: 1,
        rate_multiplier: BigDecimal::from(1),
        concurrency_limit: 0,
        quota_limit: BigDecimal::from(0),
        quota_used: BigDecimal::from(0),
        balance: BigDecimal::from(0),
        balance_updated_at: None,
        response_time: 0,
        failure_streak: 0,
        last_used_at: None,
        last_error_at: None,
        last_error_code: String::new(),
        last_error_message: String::new(),
        rate_limited_until: None,
        overload_until: None,
        expires_at: None,
        test_model: String::new(),
        test_time: None,
        extra: serde_json::json!({}),
        deleted_at: None,
        remark: String::new(),
        create_by: String::new(),
        create_time: Utc::now().fixed_offset(),
        update_by: String::new(),
        update_time: Utc::now().fixed_offset(),
        disabled_api_keys: serde_json::json!([]),
    }
}

#[test]
fn stored_openai_credentials_detect_refresh_window() {
    let creds = OpenAiStoredCredentials {
        access_token: "at".into(),
        refresh_token: "rt".into(),
        id_token: "id".into(),
        expires_at: Utc::now() + ChronoDuration::minutes(2),
        client_id: "app_test".into(),
        email: None,
        chatgpt_account_id: None,
        chatgpt_user_id: None,
        organization_id: None,
        plan_type: None,
        subscription_expires_at: None,
        token_version: Some(1),
        extra: serde_json::Map::new(),
    };

    assert!(needs_refresh(&creds, Duration::from_secs(180)));
}

#[test]
fn build_service_target_with_auth_accepts_oauth_bearer_tokens() {
    let channel = test_openai_channel();
    let account = test_oauth_account();
    let target = build_service_target_with_auth(
        &channel,
        &account,
        AuthData::from_single("at"),
        "gpt-4.1",
        EndpointScope::Chat,
    )
    .expect("target");

    assert_eq!(target.actual_model(), "gpt-4.1");
    assert_eq!(target.auth.resolve().unwrap().as_deref(), Some("at"));
}
