use super::*;

pub(super) struct FixtureSeed {
    pub(super) base: i64,
    pub(super) model_name: String,
    pub(super) group: String,
    pub(super) raw_api_key: String,
    pub(super) primary_base_url: String,
    pub(super) fallback_base_url: String,
    pub(super) endpoint_scopes: Vec<&'static str>,
    pub(super) ability_scopes: Vec<&'static str>,
    pub(super) include_fallback_abilities: bool,
    pub(super) channel_type: channel::ChannelType,
    pub(super) vendor_code: String,
    pub(super) model_type: model_config::ModelType,
    pub(super) model_mapping: serde_json::Value,
}

pub(super) async fn seed_fixture(db: &summer_sea_orm::DbConn, seed: FixtureSeed) -> CleanupIds {
    let now = chrono::Utc::now().fixed_offset();
    let primary_channel_id = seed.base + 11;
    let fallback_channel_id = seed.base + 12;
    let primary_account_id = seed.base + 21;
    let fallback_account_id = seed.base + 22;
    let token_id = seed.base + 31;
    let model_config_id = seed.base + 41;
    let mut ability_ids = Vec::new();

    model_config::ActiveModel {
        id: Set(model_config_id),
        model_name: Set(seed.model_name.clone()),
        display_name: Set(seed.model_name.clone()),
        model_type: Set(seed.model_type),
        vendor_code: Set(seed.vendor_code.clone()),
        supported_endpoints: Set(serde_json::json!(seed.endpoint_scopes)),
        input_ratio: Set(BigDecimal::from(1)),
        output_ratio: Set(BigDecimal::from(1)),
        cached_input_ratio: Set(BigDecimal::from(1)),
        reasoning_ratio: Set(BigDecimal::from(1)),
        capabilities: Set(serde_json::json!([])),
        max_context: Set(128_000),
        currency: Set("USD".to_string()),
        effective_from: Set(None),
        metadata: Set(serde_json::json!({})),
        enabled: Set(true),
        remark: Set(String::new()),
        create_by: Set("test".to_string()),
        create_time: Set(now),
        update_by: Set("test".to_string()),
        update_time: Set(now),
    }
    .insert(db)
    .await
    .expect("insert model config");

    channel::ActiveModel {
        id: Set(primary_channel_id),
        name: Set(format!("primary-channel-{}", seed.base)),
        channel_type: Set(seed.channel_type),
        vendor_code: Set(seed.vendor_code.clone()),
        base_url: Set(seed.primary_base_url),
        status: Set(channel::ChannelStatus::Enabled),
        models: Set(serde_json::json!([seed.model_name])),
        model_mapping: Set(seed.model_mapping.clone()),
        channel_group: Set(seed.group.clone()),
        endpoint_scopes: Set(serde_json::json!(seed.endpoint_scopes)),
        capabilities: Set(serde_json::json!([])),
        weight: Set(1),
        priority: Set(1),
        config: Set(serde_json::json!({})),
        auto_ban: Set(false),
        test_model: Set(String::new()),
        used_quota: Set(0),
        balance: Set(BigDecimal::from(0)),
        balance_updated_at: Set(None),
        response_time: Set(0),
        success_rate: Set(BigDecimal::from(1)),
        failure_streak: Set(0),
        last_used_at: Set(None),
        last_error_at: Set(None),
        last_error_code: Set(String::new()),
        last_error_message: Set(None),
        last_health_status: Set(1),
        deleted_at: Set(None),
        remark: Set(String::new()),
        create_by: Set("test".to_string()),
        create_time: Set(now),
        update_by: Set("test".to_string()),
        update_time: Set(now),
    }
    .insert(db)
    .await
    .expect("insert primary channel");

    channel::ActiveModel {
        id: Set(fallback_channel_id),
        name: Set(format!("fallback-channel-{}", seed.base)),
        channel_type: Set(seed.channel_type),
        vendor_code: Set(seed.vendor_code.clone()),
        base_url: Set(seed.fallback_base_url),
        status: Set(channel::ChannelStatus::Enabled),
        models: Set(serde_json::json!([seed.model_name])),
        model_mapping: Set(seed.model_mapping),
        channel_group: Set(seed.group.clone()),
        endpoint_scopes: Set(serde_json::json!(seed.endpoint_scopes)),
        capabilities: Set(serde_json::json!([])),
        weight: Set(1),
        priority: Set(10),
        config: Set(serde_json::json!({})),
        auto_ban: Set(false),
        test_model: Set(String::new()),
        used_quota: Set(0),
        balance: Set(BigDecimal::from(0)),
        balance_updated_at: Set(None),
        response_time: Set(0),
        success_rate: Set(BigDecimal::from(1)),
        failure_streak: Set(0),
        last_used_at: Set(None),
        last_error_at: Set(None),
        last_error_code: Set(String::new()),
        last_error_message: Set(None),
        last_health_status: Set(1),
        deleted_at: Set(None),
        remark: Set(String::new()),
        create_by: Set("test".to_string()),
        create_time: Set(now),
        update_by: Set("test".to_string()),
        update_time: Set(now),
    }
    .insert(db)
    .await
    .expect("insert fallback channel");

    channel_account::ActiveModel {
        id: Set(primary_account_id),
        channel_id: Set(primary_channel_id),
        name: Set(format!("primary-account-{}", seed.base)),
        credential_type: Set("api_key".to_string()),
        credentials: Set(serde_json::json!({"api_key": "sk-primary"})),
        secret_ref: Set(String::new()),
        status: Set(channel_account::AccountStatus::Enabled),
        schedulable: Set(true),
        priority: Set(1),
        weight: Set(1),
        rate_multiplier: Set(BigDecimal::from(1)),
        concurrency_limit: Set(0),
        quota_limit: Set(BigDecimal::from(0)),
        quota_used: Set(BigDecimal::from(0)),
        balance: Set(BigDecimal::from(0)),
        balance_updated_at: Set(None),
        response_time: Set(0),
        failure_streak: Set(0),
        last_used_at: Set(None),
        last_error_at: Set(None),
        last_error_code: Set(String::new()),
        last_error_message: Set(None),
        rate_limited_until: Set(None),
        overload_until: Set(None),
        expires_at: Set(None),
        test_model: Set(String::new()),
        test_time: Set(None),
        extra: Set(serde_json::json!({})),
        deleted_at: Set(None),
        remark: Set(String::new()),
        create_by: Set("test".to_string()),
        create_time: Set(now),
        update_by: Set("test".to_string()),
        update_time: Set(now),
    }
    .insert(db)
    .await
    .expect("insert primary account");

    channel_account::ActiveModel {
        id: Set(fallback_account_id),
        channel_id: Set(fallback_channel_id),
        name: Set(format!("fallback-account-{}", seed.base)),
        credential_type: Set("api_key".to_string()),
        credentials: Set(serde_json::json!({"api_key": "sk-fallback"})),
        secret_ref: Set(String::new()),
        status: Set(channel_account::AccountStatus::Enabled),
        schedulable: Set(true),
        priority: Set(1),
        weight: Set(1),
        rate_multiplier: Set(BigDecimal::from(1)),
        concurrency_limit: Set(0),
        quota_limit: Set(BigDecimal::from(0)),
        quota_used: Set(BigDecimal::from(0)),
        balance: Set(BigDecimal::from(0)),
        balance_updated_at: Set(None),
        response_time: Set(0),
        failure_streak: Set(0),
        last_used_at: Set(None),
        last_error_at: Set(None),
        last_error_code: Set(String::new()),
        last_error_message: Set(None),
        rate_limited_until: Set(None),
        overload_until: Set(None),
        expires_at: Set(None),
        test_model: Set(String::new()),
        test_time: Set(None),
        extra: Set(serde_json::json!({})),
        deleted_at: Set(None),
        remark: Set(String::new()),
        create_by: Set("test".to_string()),
        create_time: Set(now),
        update_by: Set("test".to_string()),
        update_time: Set(now),
    }
    .insert(db)
    .await
    .expect("insert fallback account");

    for scope in &seed.ability_scopes {
        let primary_ability_id = ability_id_for_scope(primary_channel_id, scope);
        ability::ActiveModel {
            id: Set(primary_ability_id),
            channel_group: Set(seed.group.clone()),
            endpoint_scope: Set((*scope).to_string()),
            model: Set(seed.model_name.clone()),
            channel_id: Set(primary_channel_id),
            enabled: Set(true),
            priority: Set(10),
            weight: Set(10),
            route_config: Set(serde_json::json!({})),
            create_time: Set(now),
            update_time: Set(now),
        }
        .insert(db)
        .await
        .expect("insert primary ability");
        ability_ids.push(primary_ability_id);

        if seed.include_fallback_abilities {
            let fallback_ability_id = ability_id_for_scope(fallback_channel_id, scope);
            ability::ActiveModel {
                id: Set(fallback_ability_id),
                channel_group: Set(seed.group.clone()),
                endpoint_scope: Set((*scope).to_string()),
                model: Set(seed.model_name.clone()),
                channel_id: Set(fallback_channel_id),
                enabled: Set(true),
                priority: Set(1),
                weight: Set(1),
                route_config: Set(serde_json::json!({})),
                create_time: Set(now),
                update_time: Set(now),
            }
            .insert(db)
            .await
            .expect("insert fallback ability");
            ability_ids.push(fallback_ability_id);
        }
    }

    token::ActiveModel {
        id: Set(token_id),
        user_id: Set(seed.base),
        service_account_id: Set(0),
        project_id: Set(0),
        name: Set(format!("test-token-{}", seed.base)),
        key_hash: Set(hash_api_key(&seed.raw_api_key)),
        key_prefix: Set(seed.raw_api_key.chars().take(8).collect()),
        status: Set(token::TokenStatus::Enabled),
        remain_quota: Set(1_000_000),
        used_quota: Set(0),
        unlimited_quota: Set(true),
        models: Set(serde_json::json!([seed.model_name])),
        endpoint_scopes: Set(serde_json::json!(seed.endpoint_scopes)),
        ip_whitelist: Set(serde_json::json!([])),
        ip_blacklist: Set(serde_json::json!([])),
        group_code_override: Set(seed.group),
        rpm_limit: Set(0),
        tpm_limit: Set(0),
        concurrency_limit: Set(0),
        daily_quota_limit: Set(0),
        monthly_quota_limit: Set(0),
        daily_used_quota: Set(0),
        monthly_used_quota: Set(0),
        daily_window_start: Set(None),
        monthly_window_start: Set(None),
        expire_time: Set(None),
        access_time: Set(None),
        last_used_ip: Set(String::new()),
        last_user_agent: Set(String::new()),
        remark: Set(String::new()),
        create_by: Set("test".to_string()),
        create_time: Set(now),
        update_by: Set("test".to_string()),
        update_time: Set(now),
    }
    .insert(db)
    .await
    .expect("insert token");

    CleanupIds {
        token_id,
        primary_channel_id,
        fallback_channel_id,
        primary_account_id,
        fallback_account_id,
        model_config_id,
        ability_ids,
    }
}

pub(super) fn unique_base_id() -> i64 {
    let now = chrono::Utc::now().timestamp_millis().abs();
    now * 1_000 + i64::from(rand::random::<u16>())
}

fn hash_api_key(raw_api_key: &str) -> String {
    use sha2::{Digest, Sha256};

    hex::encode(Sha256::digest(raw_api_key.as_bytes()))
}

pub(super) fn ability_id_for_scope(channel_id: i64, scope: &str) -> i64 {
    let scope_offset = match scope {
        "responses" => 1,
        "assistants" => 2,
        "threads" => 3,
        "files" => 4,
        "vector_stores" => 5,
        other => {
            let mut hash = 0_i64;
            for byte in other.as_bytes() {
                hash += i64::from(*byte);
            }
            100 + hash
        }
    };
    channel_id * 10 + scope_offset
}
