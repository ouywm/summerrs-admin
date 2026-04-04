use summer_common::error::ApiResult;
use summer_common::extractor::Query;
use summer_common::response::Json;
use summer_web::extractor::Component;
use summer_web::get_api;

use summer_ai_model::dto::dashboard::{
    DashboardBreakdownQueryDto, DashboardTrendsQueryDto, FailureHotspotsQueryDto,
    RecentFailuresQueryDto, TopRequestsQueryDto,
};
use summer_ai_model::vo::dashboard::{
    DashboardBreakdownVo, DashboardOverviewVo, DashboardTrendPointVo, FailureHotspotVo,
    RecentFailureVo, TopRequestVo,
};

use crate::service::log::LogService;

#[get_api("/ai/dashboard/overview")]
pub async fn overview(
    Component(svc): Component<LogService>,
) -> ApiResult<Json<DashboardOverviewVo>> {
    let vo = svc.dashboard_overview().await?;
    Ok(Json(vo))
}

#[get_api("/ai/dashboard/recent-failures")]
pub async fn recent_failures(
    Component(svc): Component<LogService>,
    Query(query): Query<RecentFailuresQueryDto>,
) -> ApiResult<Json<Vec<RecentFailureVo>>> {
    Ok(Json(
        svc.recent_failures(query.limit, query.start_time, query.end_time)
            .await?,
    ))
}

#[get_api("/ai/dashboard/failure-hotspots")]
pub async fn failure_hotspots(
    Component(svc): Component<LogService>,
    Query(query): Query<FailureHotspotsQueryDto>,
) -> ApiResult<Json<Vec<FailureHotspotVo>>> {
    Ok(Json(
        svc.failure_hotspots(
            query.group_by,
            query.limit,
            query.start_time,
            query.end_time,
        )
        .await?,
    ))
}

#[get_api("/ai/dashboard/trends")]
pub async fn trends(
    Component(svc): Component<LogService>,
    Query(query): Query<DashboardTrendsQueryDto>,
) -> ApiResult<Json<Vec<DashboardTrendPointVo>>> {
    Ok(Json(
        svc.dashboard_trends(query.period, query.limit, query.start_time, query.end_time)
            .await?,
    ))
}

#[get_api("/ai/dashboard/top-slow")]
pub async fn top_slow_requests(
    Component(svc): Component<LogService>,
    Query(query): Query<TopRequestsQueryDto>,
) -> ApiResult<Json<Vec<TopRequestVo>>> {
    Ok(Json(
        svc.top_slow_requests(query.limit, query.start_time, query.end_time)
            .await?,
    ))
}

#[get_api("/ai/dashboard/top-usage")]
pub async fn top_usage_requests(
    Component(svc): Component<LogService>,
    Query(query): Query<TopRequestsQueryDto>,
) -> ApiResult<Json<Vec<TopRequestVo>>> {
    Ok(Json(
        svc.top_usage_requests(query.limit, query.start_time, query.end_time)
            .await?,
    ))
}

#[get_api("/ai/dashboard/top-costs")]
pub async fn top_cost_requests(
    Component(svc): Component<LogService>,
    Query(query): Query<TopRequestsQueryDto>,
) -> ApiResult<Json<Vec<TopRequestVo>>> {
    Ok(Json(
        svc.top_cost_requests(query.limit, query.start_time, query.end_time)
            .await?,
    ))
}

#[get_api("/ai/dashboard/top-first-token")]
pub async fn top_first_token_requests(
    Component(svc): Component<LogService>,
    Query(query): Query<TopRequestsQueryDto>,
) -> ApiResult<Json<Vec<TopRequestVo>>> {
    Ok(Json(
        svc.top_first_token_requests(query.limit, query.start_time, query.end_time)
            .await?,
    ))
}

#[get_api("/ai/dashboard/breakdown")]
pub async fn breakdown(
    Component(svc): Component<LogService>,
    Query(query): Query<DashboardBreakdownQueryDto>,
) -> ApiResult<Json<Vec<DashboardBreakdownVo>>> {
    Ok(Json(
        svc.dashboard_breakdown(
            query.group_by,
            query.limit,
            query.start_time,
            query.end_time,
        )
        .await?,
    ))
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use chrono::{DateTime, FixedOffset, Local, SecondsFormat, TimeZone};
    use sea_orm::Set;
    use sea_orm::prelude::BigDecimal;
    use summer_ai_model::entity::log::{self, LogStatus, LogType};
    use summer_web::axum::http::{Method, StatusCode};

    use crate::router::tests::support::{TestHarness, response_json};

    struct DashboardLogSeed {
        request_id: &'static str,
        create_time: DateTime<FixedOffset>,
        status: LogStatus,
        status_code: i32,
        endpoint: &'static str,
        requested_model: &'static str,
        upstream_model: &'static str,
        model_name: &'static str,
        channel_name: &'static str,
        account_name: &'static str,
        prompt_tokens: i32,
        completion_tokens: i32,
        total_tokens: i32,
        quota: i64,
        cost_total: &'static str,
        elapsed_time: i32,
        first_token_time: i32,
        is_stream: bool,
        upstream_request_id: &'static str,
        content: &'static str,
    }

    impl DashboardLogSeed {
        fn new(request_id: &'static str, create_time: DateTime<FixedOffset>) -> Self {
            Self {
                request_id,
                create_time,
                status: LogStatus::Success,
                status_code: 200,
                endpoint: "chat/completions",
                requested_model: "",
                upstream_model: "",
                model_name: "",
                channel_name: "",
                account_name: "",
                prompt_tokens: 40,
                completion_tokens: 20,
                total_tokens: 60,
                quota: 60,
                cost_total: "0.6000000000",
                elapsed_time: 120,
                first_token_time: 0,
                is_stream: false,
                upstream_request_id: "",
                content: "",
            }
        }
    }

    const DASHBOARD_REQUEST_IDS: &[&str] = &[
        "dash-overview-success",
        "dash-overview-failure",
        "recent-fail-old",
        "recent-fail-a",
        "recent-fail-b",
        "recent-success",
        "hotspot-a-1",
        "hotspot-a-2",
        "hotspot-b-1",
        "trend-success",
        "trend-rate-limit",
        "trend-overload",
        "top-slow",
        "top-first-token",
        "top-cost-usage",
        "top-failure",
        "breakdown-1",
        "breakdown-2",
        "breakdown-3",
    ];

    fn fixed_time(hour: u32, minute: u32) -> DateTime<FixedOffset> {
        fixed_time_on(28, hour, minute)
    }

    fn fixed_time_on(day: u32, hour: u32, minute: u32) -> DateTime<FixedOffset> {
        FixedOffset::east_opt(0)
            .expect("utc offset")
            .with_ymd_and_hms(2026, 3, day, hour, minute, 0)
            .single()
            .expect("fixed time")
    }

    fn query_time(value: DateTime<FixedOffset>) -> String {
        value
            .to_rfc3339_opts(SecondsFormat::Secs, false)
            .replace('+', "%2B")
    }

    fn response_time(value: DateTime<FixedOffset>) -> String {
        value.to_rfc3339_opts(SecondsFormat::Secs, true)
    }

    fn json_i64(payload: &serde_json::Value, key: &str) -> i64 {
        payload[key].as_i64().unwrap_or_default()
    }

    fn json_f64(payload: &serde_json::Value, key: &str) -> f64 {
        payload[key].as_f64().unwrap_or_default()
    }

    async fn insert_dashboard_log(harness: &TestHarness, seed: DashboardLogSeed) {
        harness.delete_logs_by_request_id(seed.request_id).await;
        let token = harness.token_model().await;
        let channel = harness.primary_channel_model().await;
        let account = harness.primary_account_model().await;
        let model_name = if seed.model_name.is_empty() {
            harness.model_name.clone()
        } else {
            seed.model_name.to_string()
        };
        let requested_model = if seed.requested_model.is_empty() {
            model_name.clone()
        } else {
            seed.requested_model.to_string()
        };
        let upstream_model = if seed.upstream_model.is_empty() {
            model_name.clone()
        } else {
            seed.upstream_model.to_string()
        };
        let channel_name = if seed.channel_name.is_empty() {
            channel.name.clone()
        } else {
            seed.channel_name.to_string()
        };
        let account_name = if seed.account_name.is_empty() {
            account.name.clone()
        } else {
            seed.account_name.to_string()
        };

        harness
            .insert_log(log::ActiveModel {
                user_id: Set(token.user_id),
                token_id: Set(token.id),
                token_name: Set(token.name),
                project_id: Set(0),
                conversation_id: Set(0),
                message_id: Set(0),
                session_id: Set(0),
                thread_id: Set(0),
                trace_id: Set(0),
                channel_id: Set(channel.id),
                channel_name: Set(channel_name),
                account_id: Set(account.id),
                account_name: Set(account_name),
                execution_id: Set(0),
                endpoint: Set(seed.endpoint.to_string()),
                request_format: Set("openai/chat_completions".to_string()),
                requested_model: Set(requested_model),
                upstream_model: Set(upstream_model),
                model_name: Set(model_name),
                prompt_tokens: Set(seed.prompt_tokens),
                completion_tokens: Set(seed.completion_tokens),
                total_tokens: Set(seed.total_tokens),
                cached_tokens: Set(0),
                reasoning_tokens: Set(0),
                quota: Set(seed.quota),
                cost_total: Set(BigDecimal::from_str(seed.cost_total).expect("cost total")),
                price_reference: Set(String::new()),
                elapsed_time: Set(seed.elapsed_time),
                first_token_time: Set(seed.first_token_time),
                is_stream: Set(seed.is_stream),
                request_id: Set(seed.request_id.to_string()),
                upstream_request_id: Set(seed.upstream_request_id.to_string()),
                status_code: Set(seed.status_code),
                client_ip: Set("127.0.0.1".to_string()),
                user_agent: Set("dashboard-route-test".to_string()),
                content: Set(seed.content.to_string()),
                log_type: Set(LogType::Consume),
                status: Set(seed.status),
                create_time: Set(seed.create_time),
                ..Default::default()
            })
            .await;
    }

    async fn clear_dashboard_logs(harness: &TestHarness) {
        for request_id in DASHBOARD_REQUEST_IDS {
            harness.delete_logs_by_request_id(request_id).await;
        }
    }

    #[tokio::test]
    #[ignore = "requires local postgres and redis"]
    async fn overview_route_returns_expanded_runtime_metrics() {
        let harness =
            TestHarness::responses_affinity_fixture("http://127.0.0.1:9", "http://127.0.0.1:10")
                .await;
        let now = Local::now().fixed_offset();
        clear_dashboard_logs(&harness).await;
        let baseline_response = harness
            .empty_request(
                Method::GET,
                "/ai/dashboard/overview",
                "dash-overview-baseline",
            )
            .await;
        assert_eq!(baseline_response.status(), StatusCode::OK);
        let baseline = response_json(baseline_response).await;

        insert_dashboard_log(
            &harness,
            DashboardLogSeed {
                request_id: "dash-overview-success",
                create_time: now - chrono::Duration::minutes(5),
                upstream_request_id: "up-overview-1",
                total_tokens: 120,
                quota: 180,
                elapsed_time: 200,
                first_token_time: 40,
                is_stream: true,
                ..DashboardLogSeed::new("dash-overview-success", now)
            },
        )
        .await;
        insert_dashboard_log(
            &harness,
            DashboardLogSeed {
                request_id: "dash-overview-failure",
                create_time: now - chrono::Duration::minutes(2),
                status: LogStatus::Failed,
                status_code: 503,
                total_tokens: 30,
                quota: 0,
                elapsed_time: 100,
                content: "upstream overloaded",
                ..DashboardLogSeed::new("dash-overview-failure", now)
            },
        )
        .await;

        let response = harness
            .empty_request(Method::GET, "/ai/dashboard/overview", "dash-overview")
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        let payload = response_json(response).await;

        let baseline_request_count = json_i64(&baseline, "todayRequestCount");
        let baseline_stream_count = json_i64(&baseline, "streamRequestCount");
        let baseline_avg_elapsed = json_f64(&baseline, "avgElapsedTime");
        let baseline_avg_stream_first_token = json_f64(&baseline, "avgStreamFirstTokenTime");

        assert_eq!(
            json_i64(&payload, "todayRequestCount"),
            baseline_request_count + 2
        );
        assert_eq!(
            json_i64(&payload, "todayTotalQuota"),
            json_i64(&baseline, "todayTotalQuota") + 180
        );
        assert_eq!(
            json_i64(&payload, "todayTotalTokens"),
            json_i64(&baseline, "todayTotalTokens") + 150
        );
        assert_eq!(
            json_i64(&payload, "successRequestCount"),
            json_i64(&baseline, "successRequestCount") + 1
        );
        assert_eq!(
            json_i64(&payload, "failedRequestCount"),
            json_i64(&baseline, "failedRequestCount") + 1
        );
        assert_eq!(
            json_i64(&payload, "streamRequestCount"),
            baseline_stream_count + 1
        );
        assert_eq!(
            json_i64(&payload, "upstreamRequestIdCoverageCount"),
            json_i64(&baseline, "upstreamRequestIdCoverageCount") + 1
        );
        assert_eq!(
            json_i64(&payload, "totalChannelCount"),
            json_i64(&baseline, "totalChannelCount")
        );
        assert_eq!(
            json_i64(&payload, "enabledChannelCount"),
            json_i64(&baseline, "enabledChannelCount")
        );
        assert_eq!(
            json_i64(&payload, "availableChannelCount"),
            json_i64(&baseline, "availableChannelCount")
        );
        assert_eq!(
            json_i64(&payload, "totalAccountCount"),
            json_i64(&baseline, "totalAccountCount")
        );
        assert_eq!(
            json_i64(&payload, "enabledAccountCount"),
            json_i64(&baseline, "enabledAccountCount")
        );
        assert_eq!(
            json_i64(&payload, "availableAccountCount"),
            json_i64(&baseline, "availableAccountCount")
        );
        assert_eq!(
            json_i64(&payload, "disabledAccountCount"),
            json_i64(&baseline, "disabledAccountCount")
        );
        assert_eq!(
            json_i64(&payload, "unschedulableAccountCount"),
            json_i64(&baseline, "unschedulableAccountCount")
        );

        let expected_avg_elapsed = if baseline_request_count == 0 {
            150.0
        } else {
            (baseline_avg_elapsed * baseline_request_count as f64 + 300.0)
                / (baseline_request_count + 2) as f64
        };
        assert!((json_f64(&payload, "avgElapsedTime") - expected_avg_elapsed).abs() < 1e-9);

        let expected_avg_stream_first_token = if baseline_stream_count == 0 {
            40.0
        } else {
            (baseline_avg_stream_first_token * baseline_stream_count as f64 + 40.0)
                / (baseline_stream_count + 1) as f64
        };
        assert!(
            (json_f64(&payload, "avgStreamFirstTokenTime") - expected_avg_stream_first_token).abs()
                < 1e-9
        );

        harness.cleanup().await;
    }

    #[test]
    fn recent_failures_query_dto_deserializes_camel_case_rfc3339_window() {
        let query = format!(
            "limit=2&startTime={}&endTime={}",
            query_time(fixed_time(10, 0)),
            query_time(fixed_time(10, 59))
        );
        let uri = format!("/ai/dashboard/recent-failures?{query}")
            .parse()
            .expect("dashboard query uri");
        let summer_web::axum::extract::Query(dto): summer_web::axum::extract::Query<
            summer_ai_model::dto::dashboard::RecentFailuresQueryDto,
        > = summer_web::axum::extract::Query::try_from_uri(&uri)
            .expect("deserialize dashboard query");

        assert_eq!(dto.limit, Some(2));
        assert_eq!(dto.start_time, Some(fixed_time(10, 0)));
        assert_eq!(dto.end_time, Some(fixed_time(10, 59)));
    }

    #[tokio::test]
    #[ignore = "requires local postgres and redis"]
    async fn recent_failures_route_honors_limit_and_time_window() {
        let harness =
            TestHarness::responses_affinity_fixture("http://127.0.0.1:9", "http://127.0.0.1:10")
                .await;
        clear_dashboard_logs(&harness).await;
        let start = fixed_time(10, 0);
        let end = fixed_time(10, 59);

        insert_dashboard_log(
            &harness,
            DashboardLogSeed {
                request_id: "recent-fail-old",
                create_time: fixed_time(9, 50),
                status: LogStatus::Failed,
                status_code: 401,
                content: "outside window",
                ..DashboardLogSeed::new("recent-fail-old", fixed_time(9, 50))
            },
        )
        .await;
        insert_dashboard_log(
            &harness,
            DashboardLogSeed {
                request_id: "recent-fail-a",
                create_time: fixed_time(10, 5),
                status: LogStatus::Failed,
                status_code: 429,
                content: "rate limited",
                ..DashboardLogSeed::new("recent-fail-a", fixed_time(10, 5))
            },
        )
        .await;
        insert_dashboard_log(
            &harness,
            DashboardLogSeed {
                request_id: "recent-fail-b",
                create_time: fixed_time(10, 25),
                status: LogStatus::Failed,
                status_code: 503,
                content: "overloaded",
                ..DashboardLogSeed::new("recent-fail-b", fixed_time(10, 25))
            },
        )
        .await;
        insert_dashboard_log(
            &harness,
            DashboardLogSeed {
                request_id: "recent-success",
                create_time: fixed_time(10, 30),
                status: LogStatus::Success,
                ..DashboardLogSeed::new("recent-success", fixed_time(10, 30))
            },
        )
        .await;
        let recent_fail_a = harness.wait_for_log_by_request_id("recent-fail-a").await;
        assert_eq!(recent_fail_a.status, LogStatus::Failed);
        assert_eq!(
            recent_fail_a.create_time,
            fixed_time(10, 5).with_timezone(recent_fail_a.create_time.offset())
        );
        let recent_fail_b = harness.wait_for_log_by_request_id("recent-fail-b").await;
        assert_eq!(recent_fail_b.status, LogStatus::Failed);
        assert_eq!(
            recent_fail_b.create_time,
            fixed_time(10, 25).with_timezone(recent_fail_b.create_time.offset())
        );
        assert_eq!(harness.count_failed_logs_in_window(start, end).await, 2);

        let uri = format!(
            "/ai/dashboard/recent-failures?limit=2&startTime={}&endTime={}",
            query_time(start),
            query_time(end)
        );
        let response = harness
            .empty_request(Method::GET, &uri, "recent-failures")
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        let payload = response_json(response).await;

        assert_eq!(payload.as_array().map(Vec::len), Some(2));
        assert_eq!(payload[0]["requestId"], "recent-fail-b");
        assert_eq!(payload[0]["statusCode"], 503);
        assert_eq!(payload[1]["requestId"], "recent-fail-a");
        assert_eq!(payload[1]["statusCode"], 429);

        harness.cleanup().await;
    }

    #[tokio::test]
    #[ignore = "requires local postgres and redis"]
    async fn failure_hotspots_route_groups_and_classifies_failures() {
        let harness =
            TestHarness::responses_affinity_fixture("http://127.0.0.1:9", "http://127.0.0.1:10")
                .await;
        clear_dashboard_logs(&harness).await;
        let start = fixed_time_on(27, 10, 0);
        let end = fixed_time_on(27, 11, 0);

        insert_dashboard_log(
            &harness,
            DashboardLogSeed {
                request_id: "hotspot-a-1",
                create_time: fixed_time_on(27, 10, 5),
                status: LogStatus::Failed,
                status_code: 429,
                account_name: "account-alpha",
                channel_name: "channel-alpha",
                is_stream: true,
                elapsed_time: 120,
                content: "rate limited",
                ..DashboardLogSeed::new("hotspot-a-1", fixed_time_on(27, 10, 5))
            },
        )
        .await;
        insert_dashboard_log(
            &harness,
            DashboardLogSeed {
                request_id: "hotspot-a-2",
                create_time: fixed_time_on(27, 10, 15),
                status: LogStatus::Failed,
                status_code: 503,
                account_name: "account-alpha",
                channel_name: "channel-alpha",
                elapsed_time: 240,
                content: "overloaded",
                ..DashboardLogSeed::new("hotspot-a-2", fixed_time_on(27, 10, 15))
            },
        )
        .await;
        insert_dashboard_log(
            &harness,
            DashboardLogSeed {
                request_id: "hotspot-b-1",
                create_time: fixed_time_on(27, 10, 25),
                status: LogStatus::Failed,
                status_code: 401,
                account_name: "account-beta",
                channel_name: "channel-beta",
                elapsed_time: 60,
                content: "unauthorized",
                ..DashboardLogSeed::new("hotspot-b-1", fixed_time_on(27, 10, 25))
            },
        )
        .await;

        let uri = format!(
            "/ai/dashboard/failure-hotspots?groupBy=account&limit=1&startTime={}&endTime={}",
            query_time(start),
            query_time(end)
        );
        let response = harness
            .empty_request(Method::GET, &uri, "failure-hotspots")
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        let payload = response_json(response).await;

        assert_eq!(payload.as_array().map(Vec::len), Some(1));
        assert_eq!(payload[0]["groupKey"], "account-alpha");
        assert_eq!(payload[0]["failedRequestCount"], 2);
        assert_eq!(payload[0]["streamFailureCount"], 1);
        assert_eq!(payload[0]["rateLimitFailureCount"], 1);
        assert_eq!(payload[0]["overloadFailureCount"], 1);
        assert_eq!(payload[0]["authFailureCount"], 0);
        assert_eq!(payload[0]["avgElapsedTime"].as_f64(), Some(180.0));

        harness.cleanup().await;
    }

    #[tokio::test]
    #[ignore = "requires local postgres and redis"]
    async fn trends_route_returns_bucketed_points_for_custom_window() {
        let harness =
            TestHarness::responses_affinity_fixture("http://127.0.0.1:9", "http://127.0.0.1:10")
                .await;
        clear_dashboard_logs(&harness).await;
        let start = fixed_time_on(26, 10, 0);
        let end = fixed_time_on(26, 12, 59);

        insert_dashboard_log(
            &harness,
            DashboardLogSeed {
                request_id: "trend-success",
                create_time: fixed_time_on(26, 10, 5),
                status: LogStatus::Success,
                is_stream: true,
                elapsed_time: 100,
                first_token_time: 25,
                ..DashboardLogSeed::new("trend-success", fixed_time_on(26, 10, 5))
            },
        )
        .await;
        insert_dashboard_log(
            &harness,
            DashboardLogSeed {
                request_id: "trend-rate-limit",
                create_time: fixed_time_on(26, 10, 35),
                status: LogStatus::Failed,
                status_code: 429,
                elapsed_time: 200,
                content: "rate limited",
                ..DashboardLogSeed::new("trend-rate-limit", fixed_time_on(26, 10, 35))
            },
        )
        .await;
        insert_dashboard_log(
            &harness,
            DashboardLogSeed {
                request_id: "trend-overload",
                create_time: fixed_time_on(26, 12, 10),
                status: LogStatus::Failed,
                status_code: 503,
                elapsed_time: 80,
                content: "overloaded",
                ..DashboardLogSeed::new("trend-overload", fixed_time_on(26, 12, 10))
            },
        )
        .await;
        let trend_overload = harness.wait_for_log_by_request_id("trend-overload").await;
        assert_eq!(
            trend_overload.create_time,
            fixed_time_on(26, 12, 10).with_timezone(trend_overload.create_time.offset())
        );
        assert_eq!(
            harness
                .count_failed_logs_in_window(fixed_time_on(26, 12, 0), fixed_time_on(26, 12, 59))
                .await,
            1
        );

        let uri = format!(
            "/ai/dashboard/trends?period=hour&limit=4&startTime={}&endTime={}",
            query_time(start),
            query_time(end)
        );
        let response = harness
            .empty_request(Method::GET, &uri, "dashboard-trends")
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        let payload = response_json(response).await;

        assert_eq!(payload.as_array().map(Vec::len), Some(3));
        assert_eq!(
            payload[0]["bucketStart"],
            response_time(fixed_time_on(26, 10, 0))
        );
        assert_eq!(payload[0]["requestCount"], 2);
        assert_eq!(payload[0]["successRequestCount"], 1);
        assert_eq!(payload[0]["failedRequestCount"], 1);
        assert_eq!(payload[0]["streamRequestCount"], 1);
        assert_eq!(payload[0]["rateLimitFailureCount"], 1);
        assert_eq!(payload[0]["avgElapsedTime"].as_f64(), Some(150.0));
        assert_eq!(payload[0]["avgFirstTokenTime"].as_f64(), Some(25.0));

        assert_eq!(
            payload[1]["bucketStart"],
            response_time(fixed_time_on(26, 11, 0))
        );
        assert_eq!(payload[1]["requestCount"], 0);
        assert_eq!(payload[1]["failedRequestCount"], 0);

        assert_eq!(
            payload[2]["bucketStart"],
            response_time(fixed_time_on(26, 12, 0))
        );
        assert_eq!(payload[2]["requestCount"], 1);
        assert_eq!(payload[2]["overloadFailureCount"], 1);

        harness.cleanup().await;
    }

    #[tokio::test]
    #[ignore = "requires local postgres and redis"]
    async fn top_request_routes_honor_limit_and_ordering() {
        let harness =
            TestHarness::responses_affinity_fixture("http://127.0.0.1:9", "http://127.0.0.1:10")
                .await;
        clear_dashboard_logs(&harness).await;
        let start = fixed_time_on(25, 10, 0);
        let end = fixed_time_on(25, 10, 59);

        insert_dashboard_log(
            &harness,
            DashboardLogSeed {
                request_id: "top-slow",
                create_time: fixed_time_on(25, 10, 5),
                status: LogStatus::Success,
                quota: 100,
                total_tokens: 200,
                cost_total: "1.2000000000",
                elapsed_time: 500,
                first_token_time: 80,
                is_stream: true,
                ..DashboardLogSeed::new("top-slow", fixed_time_on(25, 10, 5))
            },
        )
        .await;
        insert_dashboard_log(
            &harness,
            DashboardLogSeed {
                request_id: "top-first-token",
                create_time: fixed_time_on(25, 10, 15),
                status: LogStatus::Success,
                quota: 120,
                total_tokens: 300,
                cost_total: "2.4000000000",
                elapsed_time: 400,
                first_token_time: 220,
                is_stream: true,
                ..DashboardLogSeed::new("top-first-token", fixed_time_on(25, 10, 15))
            },
        )
        .await;
        insert_dashboard_log(
            &harness,
            DashboardLogSeed {
                request_id: "top-cost-usage",
                create_time: fixed_time_on(25, 10, 25),
                status: LogStatus::Success,
                quota: 160,
                total_tokens: 500,
                cost_total: "3.5000000000",
                elapsed_time: 300,
                first_token_time: 150,
                is_stream: true,
                ..DashboardLogSeed::new("top-cost-usage", fixed_time_on(25, 10, 25))
            },
        )
        .await;
        insert_dashboard_log(
            &harness,
            DashboardLogSeed {
                request_id: "top-failure",
                create_time: fixed_time_on(25, 10, 35),
                status: LogStatus::Failed,
                status_code: 502,
                quota: 0,
                total_tokens: 0,
                cost_total: "0.0000000000",
                elapsed_time: 450,
                first_token_time: 0,
                is_stream: false,
                content: "bad gateway",
                ..DashboardLogSeed::new("top-failure", fixed_time_on(25, 10, 35))
            },
        )
        .await;

        for (path, expected_request_id) in [
            ("top-slow", "top-slow"),
            ("top-usage", "top-cost-usage"),
            ("top-costs", "top-cost-usage"),
            ("top-first-token", "top-first-token"),
        ] {
            let uri = format!(
                "/ai/dashboard/{path}?limit=1&startTime={}&endTime={}",
                query_time(start),
                query_time(end)
            );
            let response = harness.empty_request(Method::GET, &uri, path).await;
            assert_eq!(response.status(), StatusCode::OK);
            let payload = response_json(response).await;
            assert_eq!(payload.as_array().map(Vec::len), Some(1), "path={path}");
            assert_eq!(payload[0]["requestId"], expected_request_id, "path={path}");
        }

        harness.cleanup().await;
    }

    #[tokio::test]
    #[ignore = "requires local postgres and redis"]
    async fn breakdown_route_supports_all_grouping_modes() {
        let harness =
            TestHarness::responses_affinity_fixture("http://127.0.0.1:9", "http://127.0.0.1:10")
                .await;
        clear_dashboard_logs(&harness).await;
        let start = fixed_time_on(24, 10, 0);
        let end = fixed_time_on(24, 11, 0);

        insert_dashboard_log(
            &harness,
            DashboardLogSeed {
                request_id: "breakdown-1",
                create_time: fixed_time_on(24, 10, 5),
                status: LogStatus::Success,
                channel_name: "channel-alpha",
                account_name: "account-alpha",
                model_name: "model-alpha",
                requested_model: "model-alpha",
                upstream_model: "model-alpha-upstream",
                endpoint: "chat/completions",
                total_tokens: 100,
                quota: 100,
                elapsed_time: 100,
                first_token_time: 20,
                is_stream: true,
                ..DashboardLogSeed::new("breakdown-1", fixed_time_on(24, 10, 5))
            },
        )
        .await;
        insert_dashboard_log(
            &harness,
            DashboardLogSeed {
                request_id: "breakdown-2",
                create_time: fixed_time_on(24, 10, 15),
                status: LogStatus::Failed,
                status_code: 503,
                channel_name: "channel-alpha",
                account_name: "account-beta",
                model_name: "model-alpha",
                requested_model: "model-alpha",
                upstream_model: "model-alpha-upstream",
                endpoint: "responses",
                total_tokens: 20,
                quota: 0,
                elapsed_time: 180,
                first_token_time: 0,
                is_stream: false,
                content: "overloaded",
                ..DashboardLogSeed::new("breakdown-2", fixed_time_on(24, 10, 15))
            },
        )
        .await;
        insert_dashboard_log(
            &harness,
            DashboardLogSeed {
                request_id: "breakdown-3",
                create_time: fixed_time_on(24, 10, 25),
                status: LogStatus::Success,
                channel_name: "channel-beta",
                account_name: "account-beta",
                model_name: "model-beta",
                requested_model: "model-beta",
                upstream_model: "model-beta-upstream",
                endpoint: "chat/completions",
                total_tokens: 150,
                quota: 150,
                elapsed_time: 220,
                first_token_time: 40,
                is_stream: true,
                ..DashboardLogSeed::new("breakdown-3", fixed_time_on(24, 10, 25))
            },
        )
        .await;

        for (group_by, expected_group_key, expected_request_count) in [
            ("channel", "channel-alpha", 2),
            ("account", "account-beta", 2),
            ("model", "model-alpha", 2),
            ("endpoint", "chat/completions", 2),
        ] {
            let uri = format!(
                "/ai/dashboard/breakdown?groupBy={group_by}&limit=5&startTime={}&endTime={}",
                query_time(start),
                query_time(end)
            );
            let response = harness.empty_request(Method::GET, &uri, group_by).await;
            assert_eq!(response.status(), StatusCode::OK);
            let payload = response_json(response).await;
            assert_eq!(
                payload[0]["groupKey"], expected_group_key,
                "groupBy={group_by}"
            );
            assert_eq!(
                payload[0]["requestCount"], expected_request_count,
                "groupBy={group_by}"
            );
        }

        harness.cleanup().await;
    }
}
