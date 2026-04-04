use super::*;
use chrono::TimeZone;
use sea_orm::prelude::BigDecimal;
use summer_ai_model::entity::channel::{self, ChannelStatus, ChannelType};
use summer_ai_model::entity::channel_account::{self, AccountStatus};
use summer_ai_model::entity::log;

fn sample_token_info() -> TokenInfo {
    TokenInfo {
        token_id: 11,
        user_id: 22,
        name: "demo-token".into(),
        group: "default".into(),
        remain_quota: 1000,
        unlimited_quota: false,
        rpm_limit: 0,
        tpm_limit: 0,
        concurrency_limit: 0,
        allowed_models: vec![],
        endpoint_scopes: vec!["chat".into()],
    }
}

fn sample_selected_channel() -> SelectedChannel {
    SelectedChannel {
        channel_id: 33,
        channel_name: "primary".into(),
        channel_type: 1,
        base_url: "https://api.example.com".into(),
        model_mapping: serde_json::json!({}),
        api_key: "sk-demo".into(),
        account_id: 44,
        account_name: "acct-primary".into(),
    }
}

#[test]
fn build_usage_log_dto_marks_success_records() {
    let dto = build_usage_log_dto(
        &sample_token_info(),
        &sample_selected_channel(),
        &Usage {
            prompt_tokens: 10,
            completion_tokens: 5,
            total_tokens: 15,
            cached_tokens: 0,
            reasoning_tokens: 0,
        },
        AiUsageLogRecord {
            endpoint: "chat/completions".into(),
            request_format: "openai/chat_completions".into(),
            request_id: "req_123".into(),
            upstream_request_id: "up_req_123".into(),
            requested_model: "gpt-5.4".into(),
            upstream_model: "gpt-5.4".into(),
            model_name: "gpt-5.4".into(),
            quota: 99,
            elapsed_time: 120,
            first_token_time: 12,
            is_stream: false,
            client_ip: "127.0.0.1".into(),
            user_agent: "Cherry Studio".into(),
            status_code: 200,
            content: String::new(),
            status: LogStatus::Success,
        },
    );

    assert_eq!(dto.status, LogStatus::Success);
    assert_eq!(dto.status_code, 200);
    assert_eq!(dto.total_tokens, 15);
    assert_eq!(dto.quota, 99);
}

#[test]
fn build_failure_log_dto_marks_failed_records() {
    let dto = build_failure_log_dto(
        &sample_token_info(),
        &sample_selected_channel(),
        AiFailureLogRecord {
            endpoint: "responses".into(),
            request_format: "openai/responses".into(),
            request_id: "req_fail_123".into(),
            upstream_request_id: String::new(),
            requested_model: "gpt-5.4".into(),
            upstream_model: String::new(),
            model_name: "gpt-5.4".into(),
            elapsed_time: 321,
            is_stream: false,
            client_ip: "127.0.0.1".into(),
            user_agent: "Cherry Studio".into(),
            status_code: 502,
            content: "upstream returned bad gateway".into(),
        },
    );

    assert_eq!(dto.status, LogStatus::Failed);
    assert_eq!(dto.status_code, 502);
    assert_eq!(dto.content, "upstream returned bad gateway");
    assert_eq!(dto.total_tokens, 0);
    assert_eq!(dto.quota, 0);
}

#[test]
fn summarize_dashboard_overview_includes_runtime_and_stream_metrics() {
    let now = chrono::Utc::now().fixed_offset();
    let overview = summarize_dashboard_overview(
        vec![
            sample_log(1, log::LogStatus::Success, true, 120, 40, "up_req_1", now),
            sample_log(2, log::LogStatus::Failed, false, 300, 0, "", now),
        ],
        9,
        vec![
            sample_channel(1, channel::ChannelStatus::Enabled),
            sample_channel(2, channel::ChannelStatus::AutoDisabled),
        ],
        vec![
            sample_account(
                11,
                1,
                channel_account::AccountStatus::Enabled,
                true,
                None,
                None,
            ),
            sample_account(
                12,
                1,
                channel_account::AccountStatus::Enabled,
                true,
                Some(now + chrono::Duration::seconds(30)),
                None,
            ),
            sample_account(
                21,
                2,
                channel_account::AccountStatus::Disabled,
                false,
                None,
                None,
            ),
        ],
        now,
    );

    assert_eq!(overview.today_request_count, 2);
    assert_eq!(overview.success_request_count, 1);
    assert_eq!(overview.failed_request_count, 1);
    assert_eq!(overview.stream_request_count, 1);
    assert_eq!(overview.total_channel_count, 2);
    assert_eq!(overview.enabled_channel_count, 1);
    assert_eq!(overview.available_channel_count, 1);
    assert_eq!(overview.auto_disabled_channel_count, 1);
    assert_eq!(overview.total_account_count, 3);
    assert_eq!(overview.enabled_account_count, 2);
    assert_eq!(overview.available_account_count, 1);
    assert_eq!(overview.rate_limited_account_count, 1);
    assert_eq!(overview.overloaded_account_count, 0);
    assert_eq!(overview.disabled_account_count, 1);
    assert_eq!(overview.unschedulable_account_count, 1);
    assert_eq!(overview.upstream_request_id_coverage_count, 1);
    assert!((overview.avg_elapsed_time - 210.0).abs() < f64::EPSILON);
    assert!((overview.avg_stream_first_token_time - 40.0).abs() < f64::EPSILON);
}

#[test]
fn clamp_recent_failures_limit_stays_within_bounds() {
    assert_eq!(clamp_recent_failures_limit(None), 20);
    assert_eq!(clamp_recent_failures_limit(Some(0)), 1);
    assert_eq!(clamp_recent_failures_limit(Some(5)), 5);
    assert_eq!(clamp_recent_failures_limit(Some(999)), 100);
}

#[test]
fn summarize_failure_hotspots_groups_sorts_and_classifies_failures() {
    let now = chrono::Utc::now().fixed_offset();
    let items = summarize_failure_hotspots(
        vec![
            sample_log_with_failure_dims(
                1,
                "alpha",
                "acct-a",
                "gpt-5.4",
                "chat/completions",
                429,
                true,
                100,
                now - chrono::Duration::seconds(30),
            ),
            sample_log_with_failure_dims(
                2,
                "alpha",
                "acct-a",
                "gpt-5.4",
                "chat/completions",
                503,
                false,
                200,
                now - chrono::Duration::seconds(10),
            ),
            sample_log_with_failure_dims(
                3,
                "beta",
                "acct-b",
                "gemini-2.5-pro",
                "responses",
                401,
                false,
                50,
                now - chrono::Duration::seconds(5),
            ),
        ],
        "channel",
        10,
    );

    assert_eq!(items.len(), 2);
    assert_eq!(items[0].group_key, "alpha");
    assert_eq!(items[0].failed_request_count, 2);
    assert_eq!(items[0].stream_failure_count, 1);
    assert_eq!(items[0].rate_limit_failure_count, 1);
    assert_eq!(items[0].overload_failure_count, 1);
    assert_eq!(items[0].auth_failure_count, 0);
    assert_eq!(items[0].invalid_request_failure_count, 0);
    assert_eq!(items[0].other_failure_count, 0);
    assert!((items[0].avg_elapsed_time - 150.0).abs() < f64::EPSILON);

    assert_eq!(items[1].group_key, "beta");
    assert_eq!(items[1].failed_request_count, 1);
    assert_eq!(items[1].auth_failure_count, 1);
    assert_eq!(items[1].other_failure_count, 0);
}

#[test]
fn summarize_failure_hotspots_respects_limit_after_sorting() {
    let now = chrono::Utc::now().fixed_offset();
    let items = summarize_failure_hotspots(
        vec![
            sample_log_with_failure_dims(
                1,
                "alpha",
                "acct-a",
                "gpt-5.4",
                "chat/completions",
                429,
                false,
                100,
                now,
            ),
            sample_log_with_failure_dims(
                2,
                "beta",
                "acct-b",
                "gpt-5.4",
                "chat/completions",
                503,
                false,
                200,
                now + chrono::Duration::seconds(1),
            ),
            sample_log_with_failure_dims(
                3,
                "beta",
                "acct-b",
                "gpt-5.4",
                "responses",
                503,
                false,
                300,
                now + chrono::Duration::seconds(2),
            ),
        ],
        "channel",
        1,
    );

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].group_key, "beta");
    assert_eq!(items[0].failed_request_count, 2);
}

#[test]
fn normalize_failure_hotspot_group_by_defaults_to_channel() {
    assert_eq!(normalize_failure_hotspot_group_by(None), "channel");
    assert_eq!(
        normalize_failure_hotspot_group_by(Some("channel")),
        "channel"
    );
    assert_eq!(
        normalize_failure_hotspot_group_by(Some("account")),
        "account"
    );
    assert_eq!(normalize_failure_hotspot_group_by(Some("model")), "model");
    assert_eq!(
        normalize_failure_hotspot_group_by(Some("endpoint")),
        "endpoint"
    );
    assert_eq!(
        normalize_failure_hotspot_group_by(Some("unexpected")),
        "channel"
    );
}

#[test]
fn summarize_dashboard_trends_fills_empty_hour_buckets_and_classifies_failures() {
    let start = chrono::FixedOffset::east_opt(8 * 3600)
        .expect("offset")
        .with_ymd_and_hms(2026, 3, 30, 10, 0, 0)
        .single()
        .expect("start");
    let items = summarize_dashboard_trends(
        vec![
            sample_log_with_failure_dims(
                1,
                "alpha",
                "acct-a",
                "gpt-5.4",
                "chat/completions",
                200,
                true,
                100,
                start + chrono::Duration::minutes(5),
            ),
            sample_log_with_failure_dims(
                2,
                "alpha",
                "acct-a",
                "gpt-5.4",
                "chat/completions",
                429,
                false,
                200,
                start + chrono::Duration::minutes(25),
            ),
            sample_log_with_failure_dims(
                3,
                "beta",
                "acct-b",
                "gemini-2.5-pro",
                "responses",
                503,
                false,
                80,
                start + chrono::Duration::hours(2) + chrono::Duration::minutes(10),
            ),
        ],
        "hour",
        4,
        start,
        start + chrono::Duration::hours(3),
    );

    assert_eq!(items.len(), 4);
    assert_eq!(items[0].bucket_start, start);
    assert_eq!(items[0].request_count, 2);
    assert_eq!(items[0].success_request_count, 1);
    assert_eq!(items[0].failed_request_count, 1);
    assert_eq!(items[0].stream_request_count, 1);
    assert_eq!(items[0].rate_limit_failure_count, 1);
    assert_eq!(items[0].overload_failure_count, 0);
    assert!((items[0].avg_elapsed_time - 150.0).abs() < f64::EPSILON);

    assert_eq!(items[1].bucket_start, start + chrono::Duration::hours(1));
    assert_eq!(items[1].request_count, 0);
    assert_eq!(items[1].failed_request_count, 0);

    assert_eq!(items[2].bucket_start, start + chrono::Duration::hours(2));
    assert_eq!(items[2].request_count, 1);
    assert_eq!(items[2].failed_request_count, 1);
    assert_eq!(items[2].overload_failure_count, 1);
}

#[test]
fn normalize_dashboard_trend_period_and_limit_apply_defaults() {
    assert_eq!(normalize_dashboard_trend_period(None), "hour");
    assert_eq!(normalize_dashboard_trend_period(Some("day")), "day");
    assert_eq!(normalize_dashboard_trend_period(Some("unexpected")), "hour");

    assert_eq!(clamp_dashboard_trend_limit(None), 24);
    assert_eq!(clamp_dashboard_trend_limit(Some(0)), 1);
    assert_eq!(clamp_dashboard_trend_limit(Some(12)), 12);
    assert_eq!(clamp_dashboard_trend_limit(Some(999)), 168);
}

#[test]
fn summarize_top_slow_requests_orders_by_elapsed_then_first_token_time() {
    let now = chrono::Utc::now().fixed_offset();
    let items = summarize_top_slow_requests(
        vec![
            sample_log_with_failure_dims(
                1,
                "alpha",
                "acct-a",
                "gpt-5.4",
                "chat/completions",
                200,
                true,
                150,
                now - chrono::Duration::seconds(30),
            ),
            sample_log_with_failure_dims(
                2,
                "beta",
                "acct-b",
                "gpt-5.4",
                "chat/completions",
                200,
                true,
                150,
                now - chrono::Duration::seconds(10),
            ),
            sample_log_with_failure_dims(
                3,
                "gamma",
                "acct-c",
                "gpt-5.4",
                "responses",
                200,
                false,
                300,
                now - chrono::Duration::seconds(20),
            ),
        ],
        2,
    );

    assert_eq!(items.len(), 2);
    assert_eq!(items[0].request_id, "req_3");
    assert_eq!(items[1].request_id, "req_2");
}

#[test]
fn summarize_top_usage_requests_orders_by_quota_then_tokens() {
    let now = chrono::Utc::now().fixed_offset();
    let items = summarize_top_usage_requests(
        vec![
            sample_log_with_usage_dims(1, 40, 400, now - chrono::Duration::seconds(30)),
            sample_log_with_usage_dims(2, 80, 200, now - chrono::Duration::seconds(20)),
            sample_log_with_usage_dims(3, 80, 500, now - chrono::Duration::seconds(10)),
        ],
        2,
    );

    assert_eq!(items.len(), 2);
    assert_eq!(items[0].request_id, "req_3");
    assert_eq!(items[1].request_id, "req_2");
}

#[test]
fn summarize_top_cost_requests_orders_by_cost_then_quota() {
    let now = chrono::Utc::now().fixed_offset();
    let items = summarize_top_cost_requests(
        vec![
            sample_log_with_cost_dims(1, 40, "12.50", now - chrono::Duration::seconds(30)),
            sample_log_with_cost_dims(2, 90, "12.50", now - chrono::Duration::seconds(20)),
            sample_log_with_cost_dims(3, 10, "18.00", now - chrono::Duration::seconds(10)),
        ],
        2,
    );

    assert_eq!(items.len(), 2);
    assert_eq!(items[0].request_id, "req_3");
    assert_eq!(items[1].request_id, "req_2");
}

#[test]
fn summarize_top_first_token_requests_orders_by_first_token_then_elapsed() {
    let now = chrono::Utc::now().fixed_offset();
    let items = summarize_top_first_token_requests(
        vec![
            sample_log(1, log::LogStatus::Success, true, 200, 80, "up", now),
            sample_log(
                2,
                log::LogStatus::Success,
                true,
                300,
                80,
                "up",
                now + chrono::Duration::seconds(1),
            ),
            sample_log(
                3,
                log::LogStatus::Success,
                true,
                100,
                120,
                "up",
                now + chrono::Duration::seconds(2),
            ),
        ],
        2,
    );

    assert_eq!(items.len(), 2);
    assert_eq!(items[0].request_id, "req_3");
    assert_eq!(items[1].request_id, "req_2");
}

#[test]
fn summarize_dashboard_breakdown_groups_by_channel_and_calculates_rates() {
    let now = chrono::Utc::now().fixed_offset();
    let items = summarize_dashboard_breakdown(
        vec![
            sample_log_with_failure_dims(
                1,
                "alpha",
                "acct-a",
                "gpt-5.4",
                "chat/completions",
                200,
                true,
                100,
                now,
            ),
            sample_log_with_failure_dims(
                2,
                "alpha",
                "acct-a",
                "gpt-5.4",
                "chat/completions",
                429,
                false,
                200,
                now + chrono::Duration::seconds(1),
            ),
            sample_log_with_failure_dims(
                3,
                "beta",
                "acct-b",
                "gemini-2.5-pro",
                "responses",
                503,
                false,
                50,
                now + chrono::Duration::seconds(2),
            ),
        ],
        "channel",
        10,
    );

    assert_eq!(items.len(), 2);
    assert_eq!(items[0].group_key, "alpha");
    assert_eq!(items[0].request_count, 2);
    assert_eq!(items[0].success_request_count, 1);
    assert_eq!(items[0].failed_request_count, 1);
    assert!((items[0].success_rate - 0.5).abs() < f64::EPSILON);
    assert!((items[0].failure_rate - 0.5).abs() < f64::EPSILON);
    assert!((items[0].avg_elapsed_time - 150.0).abs() < f64::EPSILON);
    assert_eq!(items[0].total_tokens, 30);
    assert_eq!(items[0].total_quota, 30);

    assert_eq!(items[1].group_key, "beta");
    assert_eq!(items[1].request_count, 1);
    assert_eq!(items[1].failed_request_count, 1);
}

#[test]
fn summarize_dashboard_breakdown_groups_by_account() {
    let now = chrono::Utc::now().fixed_offset();
    let items = summarize_dashboard_breakdown(
        vec![
            sample_log_with_failure_dims(
                1,
                "alpha",
                "acct-a",
                "gpt-5.4",
                "chat/completions",
                200,
                false,
                100,
                now,
            ),
            sample_log_with_failure_dims(
                2,
                "beta",
                "acct-b",
                "gpt-5.4",
                "chat/completions",
                200,
                false,
                100,
                now + chrono::Duration::seconds(1),
            ),
            sample_log_with_failure_dims(
                3,
                "gamma",
                "acct-a",
                "gpt-5.4",
                "responses",
                503,
                false,
                200,
                now + chrono::Duration::seconds(2),
            ),
        ],
        "account",
        10,
    );

    assert_eq!(items.len(), 2);
    assert_eq!(items[0].group_key, "acct-a");
    assert_eq!(items[0].request_count, 2);
    assert_eq!(items[0].failed_request_count, 1);
    assert_eq!(items[1].group_key, "acct-b");
    assert_eq!(items[1].request_count, 1);
}

#[test]
fn normalize_dashboard_breakdown_group_by_and_limit_apply_defaults() {
    assert_eq!(normalize_dashboard_breakdown_group_by(None), "channel");
    assert_eq!(
        normalize_dashboard_breakdown_group_by(Some("account")),
        "account"
    );
    assert_eq!(
        normalize_dashboard_breakdown_group_by(Some("endpoint")),
        "endpoint"
    );
    assert_eq!(
        normalize_dashboard_breakdown_group_by(Some("model")),
        "model"
    );
    assert_eq!(
        normalize_dashboard_breakdown_group_by(Some("unexpected")),
        "channel"
    );

    assert_eq!(clamp_dashboard_breakdown_limit(None), 20);
    assert_eq!(clamp_dashboard_breakdown_limit(Some(0)), 1);
    assert_eq!(clamp_dashboard_breakdown_limit(Some(8)), 8);
    assert_eq!(clamp_dashboard_breakdown_limit(Some(999)), 100);
}

#[test]
fn resolve_dashboard_window_defaults_to_recent_span() {
    let now = chrono::FixedOffset::east_opt(8 * 3600)
        .expect("offset")
        .with_ymd_and_hms(2026, 3, 31, 12, 0, 0)
        .single()
        .expect("now");
    let (start, end) = resolve_dashboard_window(now, chrono::Duration::days(1), None, None);

    assert_eq!(end, now);
    assert_eq!(start, now - chrono::Duration::days(1));
}

#[test]
fn resolve_dashboard_window_swaps_inverted_bounds() {
    let now = chrono::FixedOffset::east_opt(8 * 3600)
        .expect("offset")
        .with_ymd_and_hms(2026, 3, 31, 12, 0, 0)
        .single()
        .expect("now");
    let start = now + chrono::Duration::hours(3);
    let end = now - chrono::Duration::hours(2);
    let (resolved_start, resolved_end) =
        resolve_dashboard_window(now, chrono::Duration::days(1), Some(start), Some(end));

    assert_eq!(resolved_start, end);
    assert_eq!(resolved_end, start);
}

#[test]
fn clamp_top_requests_limit_stays_within_bounds() {
    assert_eq!(clamp_top_requests_limit(None), 20);
    assert_eq!(clamp_top_requests_limit(Some(0)), 1);
    assert_eq!(clamp_top_requests_limit(Some(5)), 5);
    assert_eq!(clamp_top_requests_limit(Some(999)), 100);
}

fn sample_channel(id: i64, status: ChannelStatus) -> channel::Model {
    let now = chrono::Utc::now().fixed_offset();
    channel::Model {
        id,
        name: format!("channel-{id}"),
        channel_type: ChannelType::OpenAi,
        vendor_code: "openai".into(),
        base_url: "https://example.com".into(),
        status,
        models: serde_json::json!(["gpt-5.4"]),
        model_mapping: serde_json::json!({}),
        channel_group: "default".into(),
        endpoint_scopes: serde_json::json!(["chat"]),
        capabilities: serde_json::json!({}),
        weight: 10,
        priority: 10,
        config: serde_json::json!({}),
        auto_ban: true,
        test_model: "gpt-5.4".into(),
        used_quota: 0,
        balance: BigDecimal::from(0),
        balance_updated_at: None,
        response_time: 100,
        success_rate: BigDecimal::from(1),
        failure_streak: 0,
        last_used_at: None,
        last_error_at: None,
        last_error_code: String::new(),
        last_error_message: None,
        last_health_status: 1,
        deleted_at: None,
        remark: String::new(),
        create_by: "test".into(),
        create_time: now,
        update_by: "test".into(),
        update_time: now,
    }
}

fn sample_account(
    id: i64,
    channel_id: i64,
    status: AccountStatus,
    schedulable: bool,
    rate_limited_until: Option<chrono::DateTime<chrono::FixedOffset>>,
    overload_until: Option<chrono::DateTime<chrono::FixedOffset>>,
) -> channel_account::Model {
    let now = chrono::Utc::now().fixed_offset();
    channel_account::Model {
        id,
        channel_id,
        name: format!("account-{id}"),
        credential_type: "api_key".into(),
        credentials: serde_json::json!({ "api_key": format!("sk-{id}") }),
        secret_ref: String::new(),
        status,
        schedulable,
        priority: 10,
        weight: 10,
        rate_multiplier: BigDecimal::from(1),
        concurrency_limit: 0,
        quota_limit: BigDecimal::from(0),
        quota_used: BigDecimal::from(0),
        balance: BigDecimal::from(0),
        balance_updated_at: None,
        response_time: 80,
        failure_streak: 0,
        last_used_at: None,
        last_error_at: None,
        last_error_code: String::new(),
        last_error_message: None,
        rate_limited_until,
        overload_until,
        expires_at: Some(now + chrono::Duration::hours(1)),
        test_model: String::new(),
        test_time: None,
        extra: serde_json::json!({}),
        deleted_at: None,
        remark: String::new(),
        create_by: "test".into(),
        create_time: now,
        update_by: "test".into(),
        update_time: now,
    }
}

fn sample_log(
    id: i64,
    status: log::LogStatus,
    is_stream: bool,
    elapsed_time: i32,
    first_token_time: i32,
    upstream_request_id: &str,
    create_time: chrono::DateTime<chrono::FixedOffset>,
) -> log::Model {
    log::Model {
        id,
        user_id: 1,
        token_id: 2,
        token_name: "token".into(),
        project_id: 0,
        conversation_id: 0,
        message_id: 0,
        session_id: 0,
        thread_id: 0,
        trace_id: 0,
        channel_id: 3,
        channel_name: "channel".into(),
        account_id: 4,
        account_name: "account".into(),
        execution_id: 0,
        endpoint: "chat/completions".into(),
        request_format: "openai/chat_completions".into(),
        requested_model: "gpt-5.4".into(),
        upstream_model: "gpt-5.4".into(),
        model_name: "gpt-5.4".into(),
        prompt_tokens: 10,
        completion_tokens: 5,
        total_tokens: 15,
        cached_tokens: 0,
        reasoning_tokens: 0,
        quota: 15,
        cost_total: BigDecimal::from(0),
        price_reference: String::new(),
        elapsed_time,
        first_token_time,
        is_stream,
        request_id: format!("req_{id}"),
        upstream_request_id: upstream_request_id.into(),
        status_code: if status == log::LogStatus::Success {
            200
        } else {
            502
        },
        client_ip: "127.0.0.1".into(),
        user_agent: "test".into(),
        content: String::new(),
        log_type: log::LogType::Consume,
        status,
        create_time,
    }
}

#[allow(clippy::too_many_arguments)]
fn sample_log_with_failure_dims(
    id: i64,
    channel_name: &str,
    account_name: &str,
    model_name: &str,
    endpoint: &str,
    status_code: i32,
    is_stream: bool,
    elapsed_time: i32,
    create_time: chrono::DateTime<chrono::FixedOffset>,
) -> log::Model {
    log::Model {
        channel_name: channel_name.into(),
        account_name: account_name.into(),
        model_name: model_name.into(),
        endpoint: endpoint.into(),
        upstream_model: model_name.into(),
        requested_model: model_name.into(),
        status_code,
        status: if status_code == 200 {
            log::LogStatus::Success
        } else {
            log::LogStatus::Failed
        },
        is_stream,
        elapsed_time,
        first_token_time: if is_stream { 25 } else { 0 },
        ..sample_log(
            id,
            log::LogStatus::Failed,
            is_stream,
            elapsed_time,
            0,
            "",
            create_time,
        )
    }
}

fn sample_log_with_usage_dims(
    id: i64,
    quota: i64,
    total_tokens: i32,
    create_time: chrono::DateTime<chrono::FixedOffset>,
) -> log::Model {
    log::Model {
        quota,
        total_tokens,
        status: log::LogStatus::Success,
        ..sample_log(
            id,
            log::LogStatus::Success,
            false,
            100,
            0,
            "up",
            create_time,
        )
    }
}

fn sample_log_with_cost_dims(
    id: i64,
    quota: i64,
    cost_total: &str,
    create_time: chrono::DateTime<chrono::FixedOffset>,
) -> log::Model {
    log::Model {
        quota,
        cost_total: cost_total.parse().expect("cost total"),
        status: log::LogStatus::Success,
        ..sample_log(
            id,
            log::LogStatus::Success,
            false,
            100,
            0,
            "up",
            create_time,
        )
    }
}
