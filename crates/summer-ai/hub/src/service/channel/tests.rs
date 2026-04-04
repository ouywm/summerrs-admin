use super::health_logic::{
    failure_cooldown_window, should_penalize_upstream_health_failure,
    should_quarantine_account_on_auth_failure,
};
use super::{
    ChannelService, build_model_endpoint_scope_pairs, compute_failure_health_update,
    compute_relay_success_health_update, compute_test_success_health_update,
    effective_channel_endpoint_scopes, pick_probe_endpoint_scope, provider_probe_failure_message,
    relay_health_update_is_stale, relay_request_started_at, resolve_probe_endpoint_scope,
    resolve_probe_model, select_probe_model, select_schedulable_account,
    validate_probe_success_body, validate_provider_endpoint_scopes,
};
use bytes::Bytes;
use std::collections::HashMap;
use summer_ai_model::entity::channel::ChannelStatus;
use summer_ai_model::entity::channel_account;
use summer_common::error::ApiErrors;
use summer_web::axum::http::HeaderMap;

#[test]
fn effective_channel_endpoint_scopes_defaults_to_chat() {
    assert_eq!(
        effective_channel_endpoint_scopes(1, Vec::new()),
        vec!["chat".to_string()]
    );
}

#[test]
fn effective_channel_endpoint_scopes_keeps_bridged_anthropic_responses() {
    assert_eq!(
        effective_channel_endpoint_scopes(
            3,
            vec!["chat".into(), "responses".into(), "embeddings".into()]
        ),
        vec!["chat".to_string(), "responses".to_string()]
    );
}

#[test]
fn effective_channel_endpoint_scopes_keeps_openai_configured_scopes() {
    assert_eq!(
        effective_channel_endpoint_scopes(1, vec!["chat".into(), "responses".into()]),
        vec!["chat".to_string(), "responses".to_string()]
    );
}

#[test]
fn effective_channel_endpoint_scopes_keeps_azure_configured_scopes() {
    assert_eq!(
        effective_channel_endpoint_scopes(14, vec!["chat".into(), "embeddings".into()]),
        vec!["chat".to_string(), "embeddings".to_string()]
    );
}

#[test]
fn effective_channel_endpoint_scopes_keeps_gemini_embeddings() {
    assert_eq!(
        effective_channel_endpoint_scopes(24, vec!["chat".into(), "embeddings".into()]),
        vec!["chat".to_string(), "embeddings".to_string()]
    );
}

#[test]
fn validate_provider_endpoint_scopes_rejects_anthropic_embeddings_scope() {
    let error =
        validate_provider_endpoint_scopes(3, &["chat".into(), "embeddings".into()]).unwrap_err();
    assert_eq!(
        error,
        "channel type 3 does not support endpoint scopes: embeddings"
    );
}

#[test]
fn validate_provider_endpoint_scopes_allows_empty_scope_list() {
    assert!(validate_provider_endpoint_scopes(3, &[]).is_ok());
}

#[test]
fn validate_provider_endpoint_scopes_allows_azure_openai_scopes() {
    assert!(
        validate_provider_endpoint_scopes(
            14,
            &["chat".into(), "responses".into(), "embeddings".into()]
        )
        .is_ok()
    );
}

#[test]
fn validate_provider_endpoint_scopes_allows_gemini_embeddings() {
    assert!(validate_provider_endpoint_scopes(24, &["chat".into(), "embeddings".into()]).is_ok());
}

#[test]
fn build_model_endpoint_scope_pairs_intersects_supported_endpoints_per_model() {
    let supported = HashMap::from([
        ("gemini-2.5-pro".to_string(), vec!["chat".to_string()]),
        (
            "text-embedding-004".to_string(),
            vec!["embeddings".to_string()],
        ),
    ]);

    let pairs = build_model_endpoint_scope_pairs(
        vec!["gemini-2.5-pro".into(), "text-embedding-004".into()],
        vec!["chat".into(), "embeddings".into()],
        &supported,
    );

    assert_eq!(
        pairs,
        vec![
            ("gemini-2.5-pro".to_string(), "chat".to_string()),
            ("text-embedding-004".to_string(), "embeddings".to_string()),
        ]
    );
}

#[test]
fn select_probe_model_prefers_model_matching_requested_scope() {
    let supported = HashMap::from([
        ("gemini-2.5-pro".to_string(), vec!["chat".to_string()]),
        (
            "text-embedding-004".to_string(),
            vec!["embeddings".to_string()],
        ),
    ]);

    let picked = select_probe_model(
        "",
        vec!["gemini-2.5-pro".into(), "text-embedding-004".into()],
        Some("embeddings"),
        &supported,
    );

    assert_eq!(picked.as_deref(), Some("text-embedding-004"));
}

#[test]
fn pick_probe_endpoint_scope_prefers_chat_then_responses_then_embeddings() {
    assert_eq!(
        pick_probe_endpoint_scope(1, &serde_json::json!(["responses", "embeddings"])),
        Some("responses")
    );
    assert_eq!(
        pick_probe_endpoint_scope(1, &serde_json::json!(["chat", "responses"])),
        Some("chat")
    );
}

#[test]
fn resolve_probe_endpoint_scope_rejects_resource_only_scopes() {
    let error = resolve_probe_endpoint_scope(1, &serde_json::json!(["files"]), None).unwrap_err();
    assert!(matches!(error, ApiErrors::BadRequest(_)));
}

#[test]
fn resolve_probe_endpoint_scope_accepts_requested_supported_scope() {
    assert_eq!(
        resolve_probe_endpoint_scope(
            1,
            &serde_json::json!(["chat", "responses"]),
            Some("responses")
        )
        .unwrap(),
        "responses"
    );
}

#[test]
fn resolve_probe_endpoint_scope_rejects_requested_disabled_scope() {
    let error = resolve_probe_endpoint_scope(1, &serde_json::json!(["chat"]), Some("responses"))
        .unwrap_err();
    assert!(matches!(error, ApiErrors::BadRequest(_)));
}

#[test]
fn failure_cooldown_window_marks_rate_limit_failures() {
    let now = chrono::Utc::now().fixed_offset();
    let cooldown = failure_cooldown_window(429, 2, now);

    assert_eq!(
        cooldown.rate_limited_until,
        Some(now + chrono::Duration::seconds(120))
    );
    assert_eq!(cooldown.overload_until, None);
}

#[test]
fn failure_cooldown_window_marks_upstream_overload_failures() {
    let now = chrono::Utc::now().fixed_offset();
    let cooldown = failure_cooldown_window(503, 3, now);

    assert_eq!(cooldown.rate_limited_until, None);
    assert_eq!(
        cooldown.overload_until,
        Some(now + chrono::Duration::seconds(45))
    );
}

#[test]
fn failure_cooldown_window_does_not_cool_down_client_errors() {
    let now = chrono::Utc::now().fixed_offset();
    let cooldown = failure_cooldown_window(400, 4, now);

    assert_eq!(cooldown.rate_limited_until, None);
    assert_eq!(cooldown.overload_until, None);
}

#[test]
fn should_penalize_upstream_health_failure_skips_invalid_requests() {
    assert!(!should_penalize_upstream_health_failure(400));
    assert!(!should_penalize_upstream_health_failure(404));
    assert!(!should_penalize_upstream_health_failure(413));
    assert!(!should_penalize_upstream_health_failure(422));
}

#[test]
fn should_penalize_upstream_health_failure_marks_auth_and_overload_failures() {
    assert!(!should_penalize_upstream_health_failure(401));
    assert!(should_penalize_upstream_health_failure(429));
    assert!(should_penalize_upstream_health_failure(503));
}

#[test]
fn should_quarantine_account_on_auth_failure_marks_auth_failures() {
    assert!(should_quarantine_account_on_auth_failure(401));
    assert!(should_quarantine_account_on_auth_failure(403));
    assert!(!should_quarantine_account_on_auth_failure(429));
}

#[test]
fn relay_request_started_at_clamps_negative_elapsed() {
    let now = chrono::Utc::now().fixed_offset();

    assert_eq!(relay_request_started_at(now, -10), now);
}

#[test]
fn relay_health_update_is_stale_when_newer_failure_exists() {
    let now = chrono::Utc::now().fixed_offset();
    let request_started_at = now - chrono::Duration::seconds(5);

    assert!(relay_health_update_is_stale(
        Some(now - chrono::Duration::seconds(2)),
        None,
        request_started_at,
    ));
    assert!(relay_health_update_is_stale(
        None,
        Some(now - chrono::Duration::seconds(1)),
        request_started_at,
    ));
    assert!(!relay_health_update_is_stale(
        Some(now - chrono::Duration::seconds(8)),
        None,
        request_started_at,
    ));
}

#[test]
fn resolve_probe_model_prefers_channel_mapping() {
    let mapping = serde_json::json!({
        "gpt-5.4 xhigh": "claude-sonnet-4-5"
    });

    assert_eq!(
        resolve_probe_model("gpt-5.4 xhigh", &mapping),
        "claude-sonnet-4-5"
    );
}

#[test]
fn extract_api_key_returns_empty_for_non_object_credentials() {
    assert!(ChannelService::extract_api_key(&serde_json::Value::Null).is_empty());
    assert!(ChannelService::extract_api_key(&serde_json::json!("sk-plain")).is_empty());
    assert!(ChannelService::extract_api_key(&serde_json::json!(["sk-array"])).is_empty());
}

#[test]
fn extract_api_key_accepts_supported_key_aliases_and_trims_whitespace() {
    assert_eq!(
        ChannelService::extract_api_key(&serde_json::json!({"api_key":" sk-primary " })),
        "sk-primary"
    );
    assert_eq!(
        ChannelService::extract_api_key(&serde_json::json!({"apiKey":"sk-camel"})),
        "sk-camel"
    );
    assert_eq!(
        ChannelService::extract_api_key(&serde_json::json!({"key":"sk-generic"})),
        "sk-generic"
    );
    assert!(ChannelService::extract_api_key(&serde_json::json!({"api_key":""})).is_empty());
}

#[test]
fn provider_probe_failure_message_uses_provider_specific_payload() {
    let message = provider_probe_failure_message(
        3,
        summer_web::axum::http::StatusCode::TOO_MANY_REQUESTS,
        &HeaderMap::new(),
        br#"{"type":"error","error":{"type":"rate_limit_error","message":"anthropic slow down"}}"#,
    );

    assert_eq!(message, "anthropic slow down");
}

#[test]
fn provider_probe_failure_message_uses_gemini_specific_payload() {
    let message = provider_probe_failure_message(
        24,
        summer_web::axum::http::StatusCode::BAD_REQUEST,
        &HeaderMap::new(),
        br#"{"error":{"status":"INVALID_ARGUMENT","message":"bad gemini request"}}"#,
    );

    assert_eq!(message, "bad gemini request");
}

#[test]
fn validate_probe_success_body_accepts_anthropic_chat_payload() {
    let body = Bytes::from(
        serde_json::to_vec(&serde_json::json!({
            "id": "msg_123",
            "model": "claude-3-5-sonnet-20241022",
            "content": [{"type": "text", "text": "Hello from Claude"}],
            "stop_reason": "end_turn",
            "usage": {
                "input_tokens": 12,
                "output_tokens": 7
            }
        }))
        .unwrap(),
    );

    assert!(validate_probe_success_body(3, "chat", "claude-3-5-sonnet-20241022", body).is_ok());
}

#[test]
fn validate_probe_success_body_accepts_gemini_chat_payload() {
    let body = Bytes::from(
        serde_json::to_vec(&serde_json::json!({
            "candidates": [{
                "content": {
                    "parts": [{"text": "Hello from Gemini"}]
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 4,
                "candidatesTokenCount": 6,
                "totalTokenCount": 10
            }
        }))
        .unwrap(),
    );

    assert!(validate_probe_success_body(24, "chat", "gemini-2.5-pro", body).is_ok());
}

#[test]
fn validate_probe_success_body_accepts_responses_payload() {
    let body = Bytes::from(
        serde_json::to_vec(&serde_json::json!({
            "id": "resp_123",
            "object": "response",
            "model": "gpt-5.4",
            "status": "completed",
            "output": [],
            "usage": {
                "input_tokens": 3,
                "output_tokens": 2,
                "total_tokens": 5
            }
        }))
        .unwrap(),
    );

    assert!(validate_probe_success_body(1, "responses", "gpt-5.4", body).is_ok());
}

#[test]
fn validate_probe_success_body_accepts_gemini_embeddings_payload() {
    let body = Bytes::from(
        serde_json::to_vec(&serde_json::json!({
            "embedding": {
                "values": [1.0, 2.0]
            }
        }))
        .unwrap(),
    );

    assert!(validate_probe_success_body(24, "embeddings", "text-embedding-004", body).is_ok());
}

#[test]
fn select_schedulable_account_returns_first_top_priority_account_when_weights_are_non_positive() {
    let now = chrono::Utc::now().fixed_offset();
    let selected = select_schedulable_account(vec![
        channel_account::Model {
            id: 101,
            channel_id: 11,
            name: "primary".into(),
            credential_type: "api_key".into(),
            credentials: serde_json::json!({"api_key": "sk-primary"}),
            secret_ref: String::new(),
            status: channel_account::AccountStatus::Enabled,
            schedulable: true,
            priority: 10,
            weight: -5,
            rate_multiplier: sea_orm::prelude::BigDecimal::from(1),
            concurrency_limit: 0,
            quota_limit: sea_orm::prelude::BigDecimal::from(0),
            quota_used: sea_orm::prelude::BigDecimal::from(0),
            balance: sea_orm::prelude::BigDecimal::from(0),
            balance_updated_at: None,
            response_time: 0,
            failure_streak: 0,
            last_used_at: None,
            last_error_at: None,
            last_error_code: String::new(),
            last_error_message: None,
            rate_limited_until: None,
            overload_until: None,
            expires_at: Some(now + chrono::Duration::minutes(5)),
            test_model: String::new(),
            test_time: None,
            extra: serde_json::json!({}),
            deleted_at: None,
            remark: String::new(),
            create_by: "test".into(),
            create_time: now,
            update_by: "test".into(),
            update_time: now,
        },
        channel_account::Model {
            id: 102,
            channel_id: 11,
            name: "secondary".into(),
            credential_type: "api_key".into(),
            credentials: serde_json::json!({"api_key": "sk-secondary"}),
            secret_ref: String::new(),
            status: channel_account::AccountStatus::Enabled,
            schedulable: true,
            priority: 10,
            weight: -1,
            rate_multiplier: sea_orm::prelude::BigDecimal::from(1),
            concurrency_limit: 0,
            quota_limit: sea_orm::prelude::BigDecimal::from(0),
            quota_used: sea_orm::prelude::BigDecimal::from(0),
            balance: sea_orm::prelude::BigDecimal::from(0),
            balance_updated_at: None,
            response_time: 0,
            failure_streak: 0,
            last_used_at: None,
            last_error_at: None,
            last_error_code: String::new(),
            last_error_message: None,
            rate_limited_until: None,
            overload_until: None,
            expires_at: Some(now + chrono::Duration::minutes(5)),
            test_model: String::new(),
            test_time: None,
            extra: serde_json::json!({}),
            deleted_at: None,
            remark: String::new(),
            create_by: "test".into(),
            create_time: now,
            update_by: "test".into(),
            update_time: now,
        },
    ])
    .expect("selected account");

    assert_eq!(selected.id, 101);
}

#[test]
fn select_schedulable_account_supports_large_positive_weights_without_overflow() {
    let now = chrono::Utc::now().fixed_offset();
    let selected = select_schedulable_account(vec![
        channel_account::Model {
            id: 101,
            channel_id: 11,
            name: "primary".into(),
            credential_type: "api_key".into(),
            credentials: serde_json::json!({"api_key": "sk-primary"}),
            secret_ref: String::new(),
            status: channel_account::AccountStatus::Enabled,
            schedulable: true,
            priority: 10,
            weight: i32::MAX,
            rate_multiplier: sea_orm::prelude::BigDecimal::from(1),
            concurrency_limit: 0,
            quota_limit: sea_orm::prelude::BigDecimal::from(0),
            quota_used: sea_orm::prelude::BigDecimal::from(0),
            balance: sea_orm::prelude::BigDecimal::from(0),
            balance_updated_at: None,
            response_time: 0,
            failure_streak: 0,
            last_used_at: None,
            last_error_at: None,
            last_error_code: String::new(),
            last_error_message: None,
            rate_limited_until: None,
            overload_until: None,
            expires_at: Some(now + chrono::Duration::minutes(5)),
            test_model: String::new(),
            test_time: None,
            extra: serde_json::json!({}),
            deleted_at: None,
            remark: String::new(),
            create_by: "test".into(),
            create_time: now,
            update_by: "test".into(),
            update_time: now,
        },
        channel_account::Model {
            id: 102,
            channel_id: 11,
            name: "secondary".into(),
            credential_type: "api_key".into(),
            credentials: serde_json::json!({"api_key": "sk-secondary"}),
            secret_ref: String::new(),
            status: channel_account::AccountStatus::Enabled,
            schedulable: true,
            priority: 10,
            weight: i32::MAX,
            rate_multiplier: sea_orm::prelude::BigDecimal::from(1),
            concurrency_limit: 0,
            quota_limit: sea_orm::prelude::BigDecimal::from(0),
            quota_used: sea_orm::prelude::BigDecimal::from(0),
            balance: sea_orm::prelude::BigDecimal::from(0),
            balance_updated_at: None,
            response_time: 0,
            failure_streak: 0,
            last_used_at: None,
            last_error_at: None,
            last_error_code: String::new(),
            last_error_message: None,
            rate_limited_until: None,
            overload_until: None,
            expires_at: Some(now + chrono::Duration::minutes(5)),
            test_model: String::new(),
            test_time: None,
            extra: serde_json::json!({}),
            deleted_at: None,
            remark: String::new(),
            create_by: "test".into(),
            create_time: now,
            update_by: "test".into(),
            update_time: now,
        },
    ]);

    assert!(matches!(
        selected.map(|account| account.id),
        Some(101 | 102)
    ));
}

#[test]
fn compute_failure_health_update_auto_disables_channel_on_third_penalized_failure() {
    let now = chrono::Utc::now().fixed_offset();
    let update = compute_failure_health_update(
        503,
        ChannelStatus::Enabled,
        channel_account::AccountStatus::Enabled,
        true,
        2,
        2,
        true,
        None,
        None,
        now,
    );

    assert!(update.penalize);
    assert!(!update.quarantine_account);
    assert_eq!(update.next_channel_failure_streak, 3);
    assert_eq!(update.next_account_failure_streak, 3);
    assert_eq!(update.next_channel_status, ChannelStatus::AutoDisabled);
    assert_eq!(
        update.next_account_status,
        channel_account::AccountStatus::Enabled
    );
    assert!(update.next_account_schedulable);
    assert_eq!(update.next_health_status, 3);
    assert!(update.invalidate_route_cache);
    assert_eq!(
        update.cooldown.overload_until,
        Some(now + chrono::Duration::seconds(45))
    );
}

#[test]
fn compute_failure_health_update_keeps_route_cache_stable_for_non_penalized_failure() {
    let now = chrono::Utc::now().fixed_offset();
    let existing_rate_limit = Some(now + chrono::Duration::seconds(30));
    let update = compute_failure_health_update(
        400,
        ChannelStatus::Enabled,
        channel_account::AccountStatus::Enabled,
        true,
        4,
        5,
        true,
        existing_rate_limit,
        None,
        now,
    );

    assert!(!update.penalize);
    assert_eq!(update.next_channel_failure_streak, 4);
    assert_eq!(update.next_account_failure_streak, 5);
    assert_eq!(update.next_channel_status, ChannelStatus::Enabled);
    assert_eq!(
        update.next_account_status,
        channel_account::AccountStatus::Enabled
    );
    assert!(update.next_account_schedulable);
    assert_eq!(update.next_health_status, 2);
    assert!(!update.invalidate_route_cache);
    assert_eq!(update.cooldown.rate_limited_until, existing_rate_limit);
}

#[test]
fn compute_failure_health_update_quarantines_account_for_auth_failure() {
    let now = chrono::Utc::now().fixed_offset();
    let update = compute_failure_health_update(
        401,
        ChannelStatus::Enabled,
        channel_account::AccountStatus::Enabled,
        true,
        2,
        1,
        true,
        None,
        None,
        now,
    );

    assert!(!update.penalize);
    assert!(update.quarantine_account);
    assert_eq!(update.next_channel_failure_streak, 2);
    assert_eq!(update.next_account_failure_streak, 2);
    assert_eq!(update.next_channel_status, ChannelStatus::Enabled);
    assert_eq!(
        update.next_account_status,
        channel_account::AccountStatus::Disabled
    );
    assert!(!update.next_account_schedulable);
    assert_eq!(update.next_health_status, 2);
    assert!(update.invalidate_route_cache);
    assert_eq!(update.cooldown.rate_limited_until, None);
    assert_eq!(update.cooldown.overload_until, None);
}

#[test]
fn compute_failure_health_update_invalidates_route_cache_for_penalized_request_errors() {
    let now = chrono::Utc::now().fixed_offset();
    let update = compute_failure_health_update(
        0,
        ChannelStatus::Enabled,
        channel_account::AccountStatus::Enabled,
        true,
        0,
        0,
        true,
        None,
        None,
        now,
    );

    assert!(update.penalize);
    assert!(update.invalidate_route_cache);
}

#[test]
fn compute_test_success_health_update_invalidates_route_cache_when_channel_or_account_reenters() {
    let now = chrono::Utc::now().fixed_offset();
    let update = compute_test_success_health_update(
        ChannelStatus::AutoDisabled,
        Some(now + chrono::Duration::seconds(10)),
        None,
        now,
    );

    assert_eq!(update.next_channel_status, ChannelStatus::Enabled);
    assert_eq!(update.next_rate_limited_until, None);
    assert_eq!(update.next_overload_until, None);
    assert!(update.invalidate_route_cache);
}

#[test]
fn compute_test_success_health_update_keeps_route_cache_when_nothing_reenters() {
    let now = chrono::Utc::now().fixed_offset();
    let update = compute_test_success_health_update(ChannelStatus::Enabled, None, None, now);

    assert_eq!(update.next_channel_status, ChannelStatus::Enabled);
    assert_eq!(update.next_rate_limited_until, None);
    assert_eq!(update.next_overload_until, None);
    assert!(!update.invalidate_route_cache);
}

#[test]
fn compute_test_success_health_update_keeps_route_cache_when_only_stale_cooldown_is_cleared() {
    let now = chrono::Utc::now().fixed_offset();
    let update = compute_test_success_health_update(
        ChannelStatus::Enabled,
        Some(now - chrono::Duration::seconds(10)),
        Some(now - chrono::Duration::seconds(5)),
        now,
    );

    assert_eq!(update.next_channel_status, ChannelStatus::Enabled);
    assert_eq!(update.next_rate_limited_until, None);
    assert_eq!(update.next_overload_until, None);
    assert!(!update.invalidate_route_cache);
}

#[test]
fn compute_relay_success_health_update_preserves_future_cooldowns() {
    let now = chrono::Utc::now().fixed_offset();
    let rate_limit_until = Some(now + chrono::Duration::seconds(15));
    let overload_until = Some(now + chrono::Duration::seconds(8));
    let update = compute_relay_success_health_update(
        ChannelStatus::Enabled,
        rate_limit_until,
        overload_until,
        now,
    );

    assert_eq!(update.next_channel_status, ChannelStatus::Enabled);
    assert_eq!(update.next_rate_limited_until, rate_limit_until);
    assert_eq!(update.next_overload_until, overload_until);
    assert!(!update.invalidate_route_cache);
}

#[test]
fn compute_relay_success_health_update_does_not_reenable_auto_disabled_channel() {
    let now = chrono::Utc::now().fixed_offset();
    let update = compute_relay_success_health_update(ChannelStatus::AutoDisabled, None, None, now);

    assert_eq!(update.next_channel_status, ChannelStatus::AutoDisabled);
    assert_eq!(update.next_rate_limited_until, None);
    assert_eq!(update.next_overload_until, None);
    assert!(!update.invalidate_route_cache);
}
