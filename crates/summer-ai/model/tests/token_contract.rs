use summer_ai_model::dto::token::{CreateTokenDto, UpdateTokenDto};
use summer_ai_model::entity::billing::token::{self, TokenStatus};
use summer_ai_model::vo::token::TokenVo;

#[test]
fn create_token_dto_hashes_raw_key_and_stores_only_prefix() {
    let dto = CreateTokenDto {
        user_id: 42,
        service_account_id: None,
        project_id: None,
        name: "production".to_string(),
        remain_quota: Some(5000),
        unlimited_quota: Some(false),
        models: Some(vec!["gpt-4o-mini".to_string(), "claude-sonnet".to_string()]),
        endpoint_scopes: Some(vec!["chat".to_string(), "responses".to_string()]),
        ip_whitelist: Some(vec![
            "127.0.0.1".to_string(),
            "".to_string(),
            "10.0.0.0/8".to_string(),
        ]),
        ip_blacklist: None,
        group_code_override: Some("vip".to_string()),
        rpm_limit: Some(60),
        tpm_limit: Some(100_000),
        concurrency_limit: Some(8),
        daily_quota_limit: Some(10_000),
        monthly_quota_limit: Some(100_000),
        expire_time: None,
        remark: Some("ops".to_string()),
        status: None,
    };

    let active = dto.into_active_model(
        "operator",
        "sk-test-clear-value",
        "64-char-lower-sha256-hash",
        "sk-test-",
    );

    assert_eq!(active.key_hash.unwrap(), "64-char-lower-sha256-hash");
    assert_eq!(active.key_prefix.unwrap(), "sk-test-");
    assert_eq!(active.status.unwrap(), TokenStatus::Enabled);
    assert_eq!(active.service_account_id.unwrap(), 0);
    assert_eq!(active.project_id.unwrap(), 0);
    assert_eq!(
        active.models.unwrap(),
        serde_json::json!(["gpt-4o-mini", "claude-sonnet"])
    );
    assert_eq!(
        active.ip_whitelist.unwrap(),
        serde_json::json!(["127.0.0.1", "10.0.0.0/8"])
    );
}

#[test]
fn update_token_dto_does_not_touch_key_material_or_usage_counters() {
    let now = chrono::Utc::now().fixed_offset();
    let model = token::Model {
        id: 7,
        user_id: 42,
        service_account_id: 0,
        project_id: 0,
        name: "old".into(),
        key_hash: "hash-before".into(),
        key_prefix: "sk-before".into(),
        status: TokenStatus::Enabled,
        remain_quota: 100,
        used_quota: 90,
        unlimited_quota: false,
        models: serde_json::json!(["old-model"]),
        endpoint_scopes: serde_json::json!([]),
        ip_whitelist: serde_json::json!([]),
        ip_blacklist: serde_json::json!([]),
        group_code_override: String::new(),
        rpm_limit: 0,
        tpm_limit: 0,
        concurrency_limit: 0,
        daily_quota_limit: 0,
        monthly_quota_limit: 0,
        daily_used_quota: 11,
        monthly_used_quota: 22,
        daily_window_start: None,
        monthly_window_start: None,
        expire_time: None,
        access_time: None,
        last_used_ip: String::new(),
        last_user_agent: String::new(),
        remark: String::new(),
        create_by: "creator".into(),
        create_time: now,
        update_by: "creator".into(),
        update_time: now,
    };
    let mut active: token::ActiveModel = model.into();

    UpdateTokenDto {
        name: Some("new".into()),
        status: Some(TokenStatus::Disabled),
        remain_quota: Some(200),
        unlimited_quota: Some(true),
        models: Some(vec!["new-model".into()]),
        endpoint_scopes: None,
        ip_whitelist: None,
        ip_blacklist: None,
        group_code_override: None,
        rpm_limit: None,
        tpm_limit: None,
        concurrency_limit: None,
        daily_quota_limit: None,
        monthly_quota_limit: None,
        expire_time: None,
        remark: None,
    }
    .apply_to(&mut active, "operator");

    assert_eq!(active.key_hash.unwrap(), "hash-before");
    assert_eq!(active.key_prefix.unwrap(), "sk-before");
    assert_eq!(active.used_quota.unwrap(), 90);
    assert_eq!(active.daily_used_quota.unwrap(), 11);
    assert_eq!(active.monthly_used_quota.unwrap(), 22);
}

#[test]
fn token_vo_masks_key_hash_and_exposes_prefix_only() {
    let now = chrono::Utc::now().fixed_offset();
    let vo = TokenVo::from_model(token::Model {
        id: 7,
        user_id: 42,
        service_account_id: 0,
        project_id: 3,
        name: "prod".into(),
        key_hash: "secret-hash".into(),
        key_prefix: "sk-abcd".into(),
        status: TokenStatus::Enabled,
        remain_quota: 100,
        used_quota: 50,
        unlimited_quota: false,
        models: serde_json::json!(["gpt-4o"]),
        endpoint_scopes: serde_json::json!(["chat"]),
        ip_whitelist: serde_json::json!(["127.0.0.1"]),
        ip_blacklist: serde_json::json!([]),
        group_code_override: "vip".into(),
        rpm_limit: 60,
        tpm_limit: 1000,
        concurrency_limit: 4,
        daily_quota_limit: 500,
        monthly_quota_limit: 5000,
        daily_used_quota: 10,
        monthly_used_quota: 20,
        daily_window_start: None,
        monthly_window_start: None,
        expire_time: None,
        access_time: None,
        last_used_ip: "127.0.0.1".into(),
        last_user_agent: "ua".into(),
        remark: "remark".into(),
        create_by: "creator".into(),
        create_time: now,
        update_by: "updater".into(),
        update_time: now,
    });

    assert_eq!(vo.key_prefix, "sk-abcd");
    assert_eq!(vo.models, vec!["gpt-4o"]);
    assert_eq!(vo.endpoint_scopes, vec!["chat"]);
    assert_eq!(vo.create_time, now);
    assert_eq!(vo.update_time, now);
}
