use summer_ai_model::dto::daily_stats::DailyStatsQueryDto;
use summer_ai_model::dto::request_log::RequestLogQueryDto;
use summer_ai_model::entity::operations::daily_stats;
use summer_ai_model::entity::requests::log::{self, LogStatus, LogType};
use summer_ai_model::vo::daily_stats::DailyStatsSummaryVo;
use summer_ai_model::vo::request_log::RequestLogVo;

#[test]
fn request_log_vo_uses_log_row_without_request_body_snapshots() {
    let now = chrono::Utc::now().fixed_offset();
    let vo = RequestLogVo::from_log(log::Model {
        id: 1,
        user_id: 2,
        token_id: 3,
        token_name: "prod".into(),
        project_id: 4,
        conversation_id: 0,
        message_id: 0,
        session_id: 0,
        thread_id: 0,
        trace_id: 0,
        channel_id: 5,
        channel_name: "openai".into(),
        account_id: 6,
        account_name: "primary".into(),
        execution_id: 7,
        endpoint: "/v1/chat/completions".into(),
        request_format: "openai/chat_completions".into(),
        requested_model: "gpt-4o-mini".into(),
        upstream_model: "gpt-4o-mini".into(),
        model_name: "gpt-4o-mini".into(),
        prompt_tokens: 10,
        completion_tokens: 20,
        total_tokens: 30,
        cached_tokens: 3,
        reasoning_tokens: 4,
        quota: 30,
        cost_total: bigdecimal::BigDecimal::from(12),
        price_reference: "ref".into(),
        elapsed_time: 1000,
        first_token_time: 120,
        is_stream: true,
        request_id: "req-1".into(),
        dedupe_key: "dedupe".into(),
        upstream_request_id: "upstream".into(),
        status_code: 200,
        client_ip: "127.0.0.1".into(),
        user_agent: "ua".into(),
        content: "ok".into(),
        log_type: LogType::Consumption,
        status: LogStatus::Succeeded,
        create_time: now,
    });

    assert_eq!(vo.request_id, "req-1");
    assert_eq!(vo.total_tokens, 30);
    assert_eq!(vo.channel_name, "openai");
    assert!(vo.request_status.is_none());
    assert_eq!(vo.create_time, now);
}

#[test]
fn daily_stats_summary_accumulates_rows() {
    let date = chrono::NaiveDate::from_ymd_opt(2026, 4, 25).unwrap();
    let now = chrono::Utc::now().fixed_offset();
    let rows = vec![
        daily_stats::Model {
            id: 1,
            stats_date: date,
            user_id: 0,
            project_id: 0,
            channel_id: 0,
            account_id: 0,
            model_name: String::new(),
            request_count: 10,
            success_count: 8,
            fail_count: 2,
            prompt_tokens: 100,
            completion_tokens: 50,
            total_tokens: 150,
            cached_tokens: 10,
            reasoning_tokens: 5,
            quota: 150,
            cost_total: bigdecimal::BigDecimal::from(7),
            avg_elapsed_time: 100,
            avg_first_token_time: 20,
            create_time: now,
        },
        daily_stats::Model {
            id: 2,
            stats_date: date,
            user_id: 0,
            project_id: 0,
            channel_id: 0,
            account_id: 0,
            model_name: String::new(),
            request_count: 5,
            success_count: 5,
            fail_count: 0,
            prompt_tokens: 50,
            completion_tokens: 25,
            total_tokens: 75,
            cached_tokens: 0,
            reasoning_tokens: 1,
            quota: 75,
            cost_total: bigdecimal::BigDecimal::from(3),
            avg_elapsed_time: 200,
            avg_first_token_time: 40,
            create_time: now,
        },
    ];

    let summary = DailyStatsSummaryVo::from_rows(&rows);
    assert_eq!(summary.request_count, 15);
    assert_eq!(summary.success_count, 13);
    assert_eq!(summary.fail_count, 2);
    assert_eq!(summary.total_tokens, 225);
    assert_eq!(summary.avg_elapsed_time, 133);
    assert_eq!(summary.avg_first_token_time, 26);
}

#[test]
fn daily_stats_vo_uses_temporal_types() {
    let date = chrono::NaiveDate::from_ymd_opt(2026, 4, 25).unwrap();
    let now = chrono::Utc::now().fixed_offset();
    let vo = summer_ai_model::vo::daily_stats::DailyStatsVo::from_model(daily_stats::Model {
        id: 1,
        stats_date: date,
        user_id: 0,
        project_id: 0,
        channel_id: 0,
        account_id: 0,
        model_name: "gpt-4o-mini".into(),
        request_count: 10,
        success_count: 8,
        fail_count: 2,
        prompt_tokens: 100,
        completion_tokens: 50,
        total_tokens: 150,
        cached_tokens: 10,
        reasoning_tokens: 5,
        quota: 150,
        cost_total: bigdecimal::BigDecimal::from(7),
        avg_elapsed_time: 100,
        avg_first_token_time: 20,
        create_time: now,
    });

    assert_eq!(vo.stats_date, date);
    assert_eq!(vo.create_time, now);
}

#[test]
fn query_dtos_are_constructible_for_admin_filters() {
    let _request_query = RequestLogQueryDto {
        user_id: Some(1),
        token_id: Some(2),
        project_id: Some(3),
        channel_id: Some(4),
        account_id: Some(5),
        status: Some(LogStatus::Succeeded),
        log_type: Some(LogType::Consumption),
        endpoint: Some("/v1/chat/completions".into()),
        model_name: Some("gpt-4o-mini".into()),
        request_id: Some("req".into()),
        keyword: Some("prod".into()),
        start_time: None,
        end_time: None,
    };

    let _stats_query = DailyStatsQueryDto {
        start_date: Some(chrono::NaiveDate::from_ymd_opt(2026, 4, 1).unwrap()),
        end_date: Some(chrono::NaiveDate::from_ymd_opt(2026, 4, 25).unwrap()),
        user_id: None,
        project_id: None,
        channel_id: None,
        account_id: None,
        model_name: None,
    };
}
