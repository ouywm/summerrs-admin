use summer_common::error::ApiResult;
use summer_common::response::Json;
use summer_web::extractor::Component;
use summer_web::get_api;

use summer_ai_model::vo::runtime::{
    AiRuntimeChannelHealthVo, AiRuntimeRouteVo, AiRuntimeSummaryVo,
};

use crate::service::runtime::RuntimeService;

#[get_api("/ai/runtime/health")]
pub async fn runtime_health(
    Component(svc): Component<RuntimeService>,
) -> ApiResult<Json<Vec<AiRuntimeChannelHealthVo>>> {
    Ok(Json(svc.health().await?))
}

#[get_api("/ai/runtime/routes")]
pub async fn runtime_routes(
    Component(svc): Component<RuntimeService>,
) -> ApiResult<Json<Vec<AiRuntimeRouteVo>>> {
    Ok(Json(svc.routes().await?))
}

#[get_api("/ai/runtime/summary")]
pub async fn runtime_summary(
    Component(svc): Component<RuntimeService>,
) -> ApiResult<Json<AiRuntimeSummaryVo>> {
    Ok(Json(svc.summary().await?))
}

/// GET /ai/runtime/metrics — real-time relay metrics (in-memory atomic counters).
#[get_api("/ai/runtime/metrics")]
pub async fn runtime_metrics() -> Json<crate::service::metrics::RelayMetricsSnapshot> {
    Json(crate::service::metrics::relay_metrics().snapshot())
}

#[cfg(test)]
mod tests {
    use sea_orm::Set;
    use sea_orm::prelude::BigDecimal;
    use summer_ai_model::entity::log::{self, LogStatus, LogType};
    use summer_redis::redis::Client;
    use summer_web::axum::http::{Method, StatusCode};

    use crate::router::test_support::{TestHarness, response_json};
    use crate::service::runtime_cache::RuntimeCacheService;
    use crate::service::runtime_ops::RuntimeOpsService;

    async fn insert_runtime_log(
        harness: &TestHarness,
        request_id: &str,
        create_time: chrono::DateTime<chrono::FixedOffset>,
        status: LogStatus,
        status_code: i32,
        quota: i64,
    ) {
        harness.delete_logs_by_request_id(request_id).await;
        let token = harness.token_model().await;
        let channel = harness.primary_channel_model().await;
        let account = harness.primary_account_model().await;

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
                channel_name: Set(channel.name),
                account_id: Set(account.id),
                account_name: Set(account.name),
                execution_id: Set(0),
                endpoint: Set("chat/completions".to_string()),
                request_format: Set("openai/chat_completions".to_string()),
                requested_model: Set(harness.model_name.clone()),
                upstream_model: Set(harness.model_name.clone()),
                model_name: Set(harness.model_name.clone()),
                prompt_tokens: Set(40),
                completion_tokens: Set(20),
                total_tokens: Set(60),
                cached_tokens: Set(0),
                reasoning_tokens: Set(0),
                quota: Set(quota),
                cost_total: Set(BigDecimal::from(0)),
                price_reference: Set(String::new()),
                elapsed_time: Set(120),
                first_token_time: Set(30),
                is_stream: Set(true),
                request_id: Set(request_id.to_string()),
                upstream_request_id: Set(format!("up-{request_id}")),
                status_code: Set(status_code),
                client_ip: Set("127.0.0.1".to_string()),
                user_agent: Set("runtime-summary-test".to_string()),
                content: Set(String::new()),
                log_type: Set(LogType::Consume),
                status: Set(status),
                create_time: Set(create_time),
                ..Default::default()
            })
            .await;
    }

    #[tokio::test]
    #[ignore = "requires local postgres and redis"]
    async fn runtime_summary_route_reports_recent_operational_metrics() {
        let harness =
            TestHarness::responses_affinity_fixture("http://127.0.0.1:9", "http://127.0.0.1:10")
                .await;
        let now = chrono::Local::now().fixed_offset();
        harness
            .delete_logs_by_request_id("runtime-summary-success")
            .await;
        harness
            .delete_logs_by_request_id("runtime-summary-rate-limit")
            .await;
        let baseline_response = harness
            .empty_request(
                Method::GET,
                "/ai/runtime/summary",
                "runtime-summary-baseline",
            )
            .await;
        assert_eq!(baseline_response.status(), StatusCode::OK);
        let baseline = response_json(baseline_response).await;
        insert_runtime_log(
            &harness,
            "runtime-summary-success",
            now - chrono::Duration::minutes(20),
            LogStatus::Success,
            200,
            120,
        )
        .await;
        insert_runtime_log(
            &harness,
            "runtime-summary-rate-limit",
            now - chrono::Duration::minutes(10),
            LogStatus::Failed,
            429,
            0,
        )
        .await;

        let response = harness
            .empty_request(Method::GET, "/ai/runtime/summary", "runtime-summary")
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        let payload = response_json(response).await;

        assert_eq!(
            payload["recentRequestCount"].as_i64(),
            baseline["recentRequestCount"]
                .as_i64()
                .map(|value| value + 2)
        );
        assert_eq!(
            payload["recentSuccessRequestCount"].as_i64(),
            baseline["recentSuccessRequestCount"]
                .as_i64()
                .map(|value| value + 1)
        );
        assert_eq!(
            payload["recentFailedRequestCount"].as_i64(),
            baseline["recentFailedRequestCount"]
                .as_i64()
                .map(|value| value + 1)
        );
        assert_eq!(
            payload["recentRateLimitHitCount"].as_i64(),
            baseline["recentRateLimitHitCount"]
                .as_i64()
                .map(|value| value + 1)
        );
        assert_eq!(
            payload["recentActiveTokenCount"].as_i64(),
            baseline["recentActiveTokenCount"]
                .as_i64()
                .map(|value| value + 1)
        );
        assert_eq!(
            payload["recentRetryCount"].as_i64(),
            baseline["recentRetryCount"].as_i64()
        );
        assert_eq!(
            payload["recentFallbackCount"].as_i64(),
            baseline["recentFallbackCount"].as_i64()
        );
        assert_eq!(
            payload["recentRefundCount"].as_i64(),
            baseline["recentRefundCount"].as_i64()
        );
        assert_eq!(
            payload["recentSettlementFailureCount"].as_i64(),
            baseline["recentSettlementFailureCount"].as_i64()
        );

        let openai_baseline_recent = baseline["providerSummaries"]
            .as_array()
            .and_then(|items| {
                items.iter().find_map(|item| {
                    (item["channelType"].as_i64() == Some(1))
                        .then(|| item["recentRequestCount"].as_i64().unwrap_or_default())
                })
            })
            .unwrap_or_default();
        let openai_runtime_recent = payload["providerSummaries"]
            .as_array()
            .and_then(|items| {
                items.iter().find_map(|item| {
                    (item["channelType"].as_i64() == Some(1))
                        .then(|| item["recentRequestCount"].as_i64().unwrap_or_default())
                })
            })
            .unwrap_or_default();
        assert_eq!(openai_runtime_recent, openai_baseline_recent + 2);

        harness.cleanup().await;
    }

    #[tokio::test]
    #[ignore = "requires local postgres and redis"]
    async fn runtime_summary_route_reports_runtime_operational_counters() {
        let harness =
            TestHarness::responses_affinity_fixture("http://127.0.0.1:9", "http://127.0.0.1:10")
                .await;
        let runtime_ops = RuntimeOpsService::new(RuntimeCacheService::new(
            Client::open("redis://127.0.0.1/")
                .expect("create redis client")
                .get_connection_manager()
                .await
                .expect("connect redis"),
        ));

        let baseline_response = harness
            .empty_request(
                Method::GET,
                "/ai/runtime/summary",
                "runtime-summary-ops-baseline",
            )
            .await;
        assert_eq!(baseline_response.status(), StatusCode::OK);
        let baseline = response_json(baseline_response).await;

        runtime_ops.record_retry().await.expect("record retry");
        runtime_ops.record_retry().await.expect("record retry");
        runtime_ops
            .record_fallback()
            .await
            .expect("record fallback");
        runtime_ops.record_refund().await.expect("record refund");
        runtime_ops
            .record_settlement_failure()
            .await
            .expect("record settlement failure");

        let response = harness
            .empty_request(Method::GET, "/ai/runtime/summary", "runtime-summary-ops")
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        let payload = response_json(response).await;

        assert_eq!(
            payload["recentRetryCount"].as_i64(),
            baseline["recentRetryCount"].as_i64().map(|value| value + 2)
        );
        assert_eq!(
            payload["recentFallbackCount"].as_i64(),
            baseline["recentFallbackCount"]
                .as_i64()
                .map(|value| value + 1)
        );
        assert_eq!(
            payload["recentRefundCount"].as_i64(),
            baseline["recentRefundCount"]
                .as_i64()
                .map(|value| value + 1)
        );
        assert_eq!(
            payload["recentSettlementFailureCount"].as_i64(),
            baseline["recentSettlementFailureCount"]
                .as_i64()
                .map(|value| value + 1)
        );

        harness.cleanup().await;
    }
}
