use super::*;
use reqwest::header::HeaderValue;
use summer_ai_core::types::embedding::EmbeddingResponse;

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn anthropic_responses_route_skips_rate_limited_primary_on_next_request() {
    let actual_model = "claude-sonnet-4-20250514";
    let primary = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: "/v1/messages".into(),
        expected_header_name: "x-api-key".into(),
        expected_header_value: "sk-primary".into(),
        expected_body_substring: Some(format!("\"model\":\"{actual_model}\"")),
        additional_expected_headers: vec![("anthropic-version".into(), "2023-06-01".into())],
        additional_expected_body_substrings: vec!["\"text\":\"Hello\"".into()],
        response_status: StatusCode::TOO_MANY_REQUESTS,
        response_content_type: "application/json".into(),
        response_headers: vec![],
        response_body:
            r#"{"type":"error","error":{"type":"rate_limit_error","message":"slow down"}}"#
                .to_string(),
    })
    .await;
    let fallback = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: "/v1/messages".into(),
        expected_header_name: "x-api-key".into(),
        expected_header_value: "sk-fallback".into(),
        expected_body_substring: Some(format!("\"model\":\"{actual_model}\"")),
        additional_expected_headers: vec![("anthropic-version".into(), "2023-06-01".into())],
        additional_expected_body_substrings: vec!["\"text\":\"Hello\"".into()],
        response_status: StatusCode::OK,
        response_content_type: "application/json".into(),
        response_headers: vec![],
        response_body: serde_json::json!({
            "id": "msg_resp_fallback_skip_123",
            "model": actual_model,
            "content": [{"type": "text", "text": "Hello from Claude fallback"}],
            "stop_reason": "end_turn",
            "usage": {
                "input_tokens": 12,
                "output_tokens": 7
            }
        })
        .to_string(),
    })
    .await;
    let harness = TestHarness::anthropic_responses_fallback_affinity_fixture(
        &primary.base_url,
        &fallback.base_url,
    )
    .await;
    let first_request_id = format!(
        "anthropic-responses-rate-limit-first-{}",
        harness.model_name
    );
    let second_request_id = format!(
        "anthropic-responses-rate-limit-second-{}",
        harness.model_name
    );

    let first_response = harness
        .json_request(
            Method::POST,
            "/v1/responses",
            &first_request_id,
            serde_json::json!({
                "model": harness.model_name,
                "input": "Hello",
                "stream": false
            }),
        )
        .await;
    assert_eq!(first_response.status(), StatusCode::OK);
    let first_payload = crate::router::tests::support::response_json(first_response).await;
    assert_eq!(first_payload["id"], "msg_resp_fallback_skip_123");

    let primary_account = harness.wait_for_primary_account_rate_limited().await;
    assert_eq!(primary_account.failure_streak, 1);
    assert!(primary_account.rate_limited_until.is_some());

    let second_response = harness
        .json_request(
            Method::POST,
            "/v1/responses",
            &second_request_id,
            serde_json::json!({
                "model": harness.model_name,
                "input": "Hello",
                "stream": false
            }),
        )
        .await;
    assert_eq!(second_response.status(), StatusCode::OK);
    let second_payload = crate::router::tests::support::response_json(second_response).await;
    assert_eq!(second_payload["id"], "msg_resp_fallback_skip_123");

    let token = harness.wait_for_token_used_quota(38).await;
    assert_eq!(token.used_quota, 38);

    assert_eq!(primary.hit_count("/v1/messages"), 1);
    assert_eq!(fallback.hit_count("/v1/messages"), 2);

    harness.cleanup().await;
}

#[tokio::test]
async fn embeddings_non_stream_mock_upstream_success() {
    let req = sample_mock_embeddings_request();
    let actual_model = "text-embedding-3-small";
    let (_server, response) = send_mock_embeddings_request(
        1,
        "sk-openai-test",
        &req,
        actual_model,
        MockUpstreamSpec {
            expected_path_and_query: "/v1/embeddings".into(),
            expected_header_name: "authorization".into(),
            expected_header_value: "Bearer sk-openai-test".into(),
            expected_body_substring: Some(format!("\"model\":\"{actual_model}\"")),
            additional_expected_headers: vec![],
            additional_expected_body_substrings: vec![],
            response_status: StatusCode::OK,
            response_content_type: "application/json".into(),
            response_headers: vec![],
            response_body: serde_json::json!({
                "object": "list",
                "data": [{
                    "object": "embedding",
                    "index": 0,
                    "embedding": [0.1, 0.2]
                }],
                "usage": {
                    "prompt_tokens": 8,
                    "completion_tokens": 0,
                    "total_tokens": 8
                }
            })
            .to_string(),
        },
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let parsed: EmbeddingResponse =
        serde_json::from_slice(&response.bytes().await.expect("body")).expect("embeddings json");
    assert_eq!(parsed.data.len(), 1);
    assert_eq!(parsed.usage.total_tokens, 8);
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn embeddings_route_persists_request_and_execution_snapshots() {
    let primary = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: "/v1/embeddings".into(),
        expected_header_name: "authorization".into(),
        expected_header_value: "Bearer sk-primary".into(),
        expected_body_substring: Some("\"model\":\"text-embedding-3-small\"".into()),
        additional_expected_headers: vec![],
        additional_expected_body_substrings: vec!["\"input\":\"hello\"".into()],
        response_status: StatusCode::OK,
        response_content_type: "application/json".into(),
        response_headers: vec![("x-request-id".into(), "embeddings-upstream-123".into())],
        response_body: serde_json::json!({
            "object": "list",
            "data": [{
                "object": "embedding",
                "index": 0,
                "embedding": [0.1, 0.2]
            }],
            "usage": {
                "prompt_tokens": 8,
                "completion_tokens": 0,
                "total_tokens": 8
            }
        })
        .to_string(),
    })
    .await;
    let harness =
        TestHarness::embeddings_affinity_fixture(&primary.base_url, "http://127.0.0.1:9").await;
    let request_id = format!("embeddings-request-tracking-{}", harness.model_name);

    let response = harness
        .json_request(
            Method::POST,
            "/v1/embeddings",
            &request_id,
            serde_json::json!({
                "model": harness.model_name,
                "input": "hello"
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let upstream_request_id = response
        .headers()
        .get("x-upstream-request-id")
        .and_then(|value| value.to_str().ok())
        .expect("embeddings upstream request id")
        .to_string();
    let payload = crate::router::tests::support::response_json(response).await;
    assert_eq!(payload["data"][0]["embedding"][0], 0.1);
    assert_eq!(upstream_request_id, "embeddings-upstream-123");

    let request = harness.wait_for_request_by_request_id(&request_id).await;
    assert_eq!(request.endpoint, "embeddings");
    assert_eq!(request.request_format, "openai/embeddings");
    assert_eq!(request.requested_model, harness.model_name);
    assert_eq!(request.upstream_model, "text-embedding-3-small");
    assert_eq!(request.status, RequestStatus::Success);
    assert_eq!(request.response_status_code, 200);
    assert!(!request.is_stream);
    assert_eq!(request.request_body["input"], "hello");

    let executions = harness
        .wait_for_request_executions_by_request_id(&request_id)
        .await;
    assert_eq!(executions.len(), 1);
    let execution = &executions[0];
    assert_eq!(execution.status, ExecutionStatus::Success);
    assert_eq!(execution.endpoint, "embeddings");
    assert_eq!(execution.request_format, "openai/embeddings");
    assert_eq!(execution.requested_model, harness.model_name);
    assert_eq!(execution.upstream_model, "text-embedding-3-small");
    assert_eq!(execution.response_status_code, 200);
    assert_eq!(execution.upstream_request_id, "embeddings-upstream-123");
    assert_eq!(execution.request_body["model"], "text-embedding-3-small");

    harness.cleanup().await;
}

#[tokio::test]
async fn gemini_embeddings_non_stream_mock_upstream_success() {
    let req = sample_mock_embeddings_request();
    let actual_model = "text-embedding-004";
    let (_server, response) = send_mock_embeddings_request(
        24,
        "gem-key",
        &req,
        actual_model,
        MockUpstreamSpec {
            expected_path_and_query: format!("/v1beta/models/{actual_model}:embedContent"),
            expected_header_name: "x-goog-api-key".into(),
            expected_header_value: "gem-key".into(),
            expected_body_substring: Some("\"content\"".into()),
            additional_expected_headers: vec![],
            additional_expected_body_substrings: vec![
                "\"model\":\"models/text-embedding-004\"".into(),
                "\"text\":\"hello\"".into(),
            ],
            response_status: StatusCode::OK,
            response_content_type: "application/json".into(),
            response_headers: vec![],
            response_body: serde_json::json!({
                "embedding": {
                    "values": [1.0, 2.0]
                }
            })
            .to_string(),
        },
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let parsed = get_adapter(24)
        .parse_embeddings_response(response.bytes().await.expect("body"), actual_model, 8)
        .expect("parse gemini embeddings response");
    assert_eq!(parsed.data.len(), 1);
    assert_eq!(parsed.data[0].embedding, serde_json::json!([1.0, 2.0]));
    assert_eq!(parsed.usage.total_tokens, 8);
}

#[tokio::test]
async fn gemini_embeddings_batch_mock_upstream_success() {
    let req: EmbeddingRequest = serde_json::from_value(serde_json::json!({
        "model": "text-embedding-004",
        "input": ["hello", "world"]
    }))
    .expect("gemini batch embeddings request");
    let actual_model = "text-embedding-004";
    let (_server, response) = send_mock_embeddings_request(
        24,
        "gem-key",
        &req,
        actual_model,
        MockUpstreamSpec {
            expected_path_and_query: format!("/v1beta/models/{actual_model}:batchEmbedContents"),
            expected_header_name: "x-goog-api-key".into(),
            expected_header_value: "gem-key".into(),
            expected_body_substring: Some("\"requests\"".into()),
            additional_expected_headers: vec![],
            additional_expected_body_substrings: vec![
                "\"model\":\"models/text-embedding-004\"".into(),
                "\"text\":\"hello\"".into(),
                "\"text\":\"world\"".into(),
            ],
            response_status: StatusCode::OK,
            response_content_type: "application/json".into(),
            response_headers: vec![],
            response_body: serde_json::json!({
                "embeddings": [
                    {"values": [1.0, 2.0]},
                    {"values": [3.0, 4.0]}
                ]
            })
            .to_string(),
        },
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let parsed = get_adapter(24)
        .parse_embeddings_response(response.bytes().await.expect("body"), actual_model, 12)
        .expect("parse gemini batch embeddings response");
    assert_eq!(parsed.data.len(), 2);
    assert_eq!(parsed.data[0].embedding, serde_json::json!([1.0, 2.0]));
    assert_eq!(parsed.data[1].embedding, serde_json::json!([3.0, 4.0]));
    assert_eq!(parsed.usage.total_tokens, 12);
}

#[tokio::test]
async fn gemini_embeddings_mock_upstream_provider_failure() {
    let req = sample_mock_embeddings_request();
    let actual_model = "text-embedding-004";
    let (_server, response) = send_mock_embeddings_request(
        24,
        "gem-key",
        &req,
        actual_model,
        MockUpstreamSpec {
            expected_path_and_query: format!("/v1beta/models/{actual_model}:embedContent"),
            expected_header_name: "x-goog-api-key".into(),
            expected_header_value: "gem-key".into(),
            expected_body_substring: Some("\"content\"".into()),
            additional_expected_headers: vec![],
            additional_expected_body_substrings: vec![
                "\"model\":\"models/text-embedding-004\"".into(),
                "\"text\":\"hello\"".into(),
            ],
            response_status: StatusCode::BAD_REQUEST,
            response_content_type: "application/json".into(),
            response_headers: vec![],
            response_body:
                r#"{"error":{"status":"INVALID_ARGUMENT","message":"bad embedding input"}}"#
                    .to_string(),
        },
    )
    .await;

    let status = response.status();
    let headers = response.headers().clone();
    let body = response.bytes().await.expect("body");
    let failure = classify_upstream_provider_failure(24, status, &headers, &body);
    assert_eq!(failure.scope, UpstreamFailureScope::Channel);
    assert_eq!(failure.error.status, StatusCode::BAD_REQUEST);
    assert_eq!(failure.error.error.error.r#type, "invalid_request_error");
    assert_eq!(failure.error.error.error.message, "bad embedding input");
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn gemini_embeddings_route_normalizes_provider_payload_to_openai_shape() {
    let actual_model = "text-embedding-004";
    let primary = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: format!("/v1beta/models/{actual_model}:embedContent"),
        expected_header_name: "x-goog-api-key".into(),
        expected_header_value: "sk-primary".into(),
        expected_body_substring: Some("\"content\"".into()),
        additional_expected_headers: vec![],
        additional_expected_body_substrings: vec![
            "\"model\":\"models/text-embedding-004\"".into(),
            "\"text\":\"hello\"".into(),
        ],
        response_status: StatusCode::OK,
        response_content_type: "application/json".into(),
        response_headers: vec![(
            "x-request-id".into(),
            "gemini-upstream-embeddings-123".into(),
        )],
        response_body: serde_json::json!({
            "embedding": {
                "values": [1.0, 2.0]
            }
        })
        .to_string(),
    })
    .await;
    let harness =
        TestHarness::gemini_embeddings_affinity_fixture(&primary.base_url, "http://127.0.0.1:9")
            .await;
    let request_id = format!("gemini-embeddings-route-success-{}", harness.model_name);

    let response = harness
        .json_request(
            Method::POST,
            "/v1/embeddings",
            &request_id,
            serde_json::json!({
                "model": harness.model_name,
                "input": "hello"
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let upstream_request_id = response
        .headers()
        .get("x-upstream-request-id")
        .and_then(|value| value.to_str().ok())
        .expect("gemini embeddings upstream request id")
        .to_string();
    let payload = crate::router::tests::support::response_json(response).await;

    assert_eq!(payload["object"], "list");
    assert_eq!(
        payload["data"][0]["embedding"],
        serde_json::json!([1.0, 2.0])
    );
    assert_eq!(payload["usage"]["total_tokens"], 2);
    assert_eq!(upstream_request_id, "gemini-upstream-embeddings-123");

    let token = harness.wait_for_token_used_quota(2).await;
    assert_eq!(token.used_quota, 2);

    let log = harness.wait_for_log_by_request_id(&request_id).await;
    assert_eq!(log.endpoint, "embeddings");
    assert_eq!(log.request_format, "openai/embeddings");
    assert_eq!(log.requested_model, harness.model_name);
    assert_eq!(log.upstream_model, actual_model);
    assert_eq!(log.model_name, harness.model_name);
    assert_eq!(log.total_tokens, 2);
    assert_eq!(log.quota, 2);
    assert_eq!(log.status_code, 200);
    assert_eq!(log.upstream_request_id, "gemini-upstream-embeddings-123");
    assert_eq!(log.status, LogStatus::Success);
    assert!(!log.is_stream);

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn gemini_embeddings_route_falls_back_after_primary_invalid_request() {
    let actual_model = "text-embedding-004";
    let primary = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: format!("/v1beta/models/{actual_model}:embedContent"),
        expected_header_name: "x-goog-api-key".into(),
        expected_header_value: "sk-primary".into(),
        expected_body_substring: Some("\"content\"".into()),
        additional_expected_headers: vec![],
        additional_expected_body_substrings: vec![
            "\"model\":\"models/text-embedding-004\"".into(),
            "\"text\":\"hello\"".into(),
        ],
        response_status: StatusCode::BAD_REQUEST,
        response_content_type: "application/json".into(),
        response_headers: vec![],
        response_body: r#"{"error":{"status":"INVALID_ARGUMENT","message":"bad embedding input"}}"#
            .to_string(),
    })
    .await;
    let fallback = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: format!("/v1beta/models/{actual_model}:embedContent"),
        expected_header_name: "x-goog-api-key".into(),
        expected_header_value: "sk-fallback".into(),
        expected_body_substring: Some("\"content\"".into()),
        additional_expected_headers: vec![],
        additional_expected_body_substrings: vec![
            "\"model\":\"models/text-embedding-004\"".into(),
            "\"text\":\"hello\"".into(),
        ],
        response_status: StatusCode::OK,
        response_content_type: "application/json".into(),
        response_headers: vec![],
        response_body: serde_json::json!({
            "embedding": {
                "values": [9.0, 8.0]
            }
        })
        .to_string(),
    })
    .await;
    let harness = TestHarness::gemini_embeddings_fallback_affinity_fixture(
        &primary.base_url,
        &fallback.base_url,
    )
    .await;

    let response = harness
        .json_request(
            Method::POST,
            "/v1/embeddings",
            "gemini-embeddings-route-fallback",
            serde_json::json!({
                "model": harness.model_name,
                "input": "hello"
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let payload = crate::router::tests::support::response_json(response).await;

    assert_eq!(
        payload["data"][0]["embedding"],
        serde_json::json!([9.0, 8.0])
    );
    assert_eq!(payload["usage"]["total_tokens"], 2);

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn anthropic_responses_route_quarantines_primary_account_after_auth_failure() {
    let actual_model = "claude-sonnet-4-20250514";
    let primary = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: "/v1/messages".into(),
        expected_header_name: "x-api-key".into(),
        expected_header_value: "sk-primary".into(),
        expected_body_substring: Some(format!("\"model\":\"{actual_model}\"")),
        additional_expected_headers: vec![("anthropic-version".into(), "2023-06-01".into())],
        additional_expected_body_substrings: vec!["\"text\":\"Hello\"".into()],
        response_status: StatusCode::UNAUTHORIZED,
        response_content_type: "application/json".into(),
        response_headers: vec![],
        response_body:
            r#"{"type":"error","error":{"type":"authentication_error","message":"invalid api key"}}"#
                .to_string(),
    })
    .await;
    let fallback = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: "/v1/messages".into(),
        expected_header_name: "x-api-key".into(),
        expected_header_value: "sk-fallback".into(),
        expected_body_substring: Some(format!("\"model\":\"{actual_model}\"")),
        additional_expected_headers: vec![("anthropic-version".into(), "2023-06-01".into())],
        additional_expected_body_substrings: vec!["\"text\":\"Hello\"".into()],
        response_status: StatusCode::OK,
        response_content_type: "application/json".into(),
        response_headers: vec![],
        response_body: serde_json::json!({
            "id": "msg_resp_auth_fallback_123",
            "model": actual_model,
            "content": [{"type": "text", "text": "Hello from Claude fallback"}],
            "stop_reason": "end_turn",
            "usage": {
                "input_tokens": 12,
                "output_tokens": 7
            }
        })
        .to_string(),
    })
    .await;
    let harness = TestHarness::anthropic_responses_fallback_affinity_fixture(
        &primary.base_url,
        &fallback.base_url,
    )
    .await;
    let first_request_id = format!("anthropic-responses-auth-first-{}", harness.model_name);
    let second_request_id = format!("anthropic-responses-auth-second-{}", harness.model_name);

    let first_response = harness
        .json_request(
            Method::POST,
            "/v1/responses",
            &first_request_id,
            serde_json::json!({
                "model": harness.model_name,
                "input": "Hello",
                "stream": false
            }),
        )
        .await;
    assert_eq!(first_response.status(), StatusCode::OK);
    let first_payload = crate::router::tests::support::response_json(first_response).await;
    assert_eq!(first_payload["id"], "msg_resp_auth_fallback_123");

    let primary_account = harness.wait_for_primary_account_disabled().await;
    assert_eq!(
        primary_account.status,
        summer_ai_model::entity::channel_account::AccountStatus::Disabled
    );
    assert!(!primary_account.schedulable);
    assert_eq!(primary_account.failure_streak, 1);

    let primary_channel = harness.primary_channel_model().await;
    assert_eq!(primary_channel.failure_streak, 0);
    assert_eq!(primary_channel.last_health_status, 2);

    let second_response = harness
        .json_request(
            Method::POST,
            "/v1/responses",
            &second_request_id,
            serde_json::json!({
                "model": harness.model_name,
                "input": "Hello",
                "stream": false
            }),
        )
        .await;
    assert_eq!(second_response.status(), StatusCode::OK);
    let second_payload = crate::router::tests::support::response_json(second_response).await;
    assert_eq!(second_payload["id"], "msg_resp_auth_fallback_123");

    let token = harness.wait_for_token_used_quota(38).await;
    assert_eq!(token.used_quota, 38);

    assert_eq!(primary.hit_count("/v1/messages"), 1);
    assert_eq!(fallback.hit_count("/v1/messages"), 2);

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn gemini_embeddings_route_falls_back_after_primary_rate_limit() {
    let actual_model = "text-embedding-004";
    let primary = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: format!("/v1beta/models/{actual_model}:embedContent"),
        expected_header_name: "x-goog-api-key".into(),
        expected_header_value: "sk-primary".into(),
        expected_body_substring: Some("\"content\"".into()),
        additional_expected_headers: vec![],
        additional_expected_body_substrings: vec![
            "\"model\":\"models/text-embedding-004\"".into(),
            "\"text\":\"hello\"".into(),
        ],
        response_status: StatusCode::TOO_MANY_REQUESTS,
        response_content_type: "application/json".into(),
        response_headers: vec![],
        response_body: r#"{"error":{"status":"RESOURCE_EXHAUSTED","message":"slow down"}}"#
            .to_string(),
    })
    .await;
    let fallback = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: format!("/v1beta/models/{actual_model}:embedContent"),
        expected_header_name: "x-goog-api-key".into(),
        expected_header_value: "sk-fallback".into(),
        expected_body_substring: Some("\"content\"".into()),
        additional_expected_headers: vec![],
        additional_expected_body_substrings: vec![
            "\"model\":\"models/text-embedding-004\"".into(),
            "\"text\":\"hello\"".into(),
        ],
        response_status: StatusCode::OK,
        response_content_type: "application/json".into(),
        response_headers: vec![(
            "x-request-id".into(),
            "gemini-fallback-embeddings-123".into(),
        )],
        response_body: serde_json::json!({
            "embedding": {
                "values": [9.0, 8.0]
            }
        })
        .to_string(),
    })
    .await;
    let harness = TestHarness::gemini_embeddings_fallback_affinity_fixture(
        &primary.base_url,
        &fallback.base_url,
    )
    .await;
    let request_id = format!("gemini-embeddings-route-rate-limit-{}", harness.model_name);

    let response = harness
        .json_request(
            Method::POST,
            "/v1/embeddings",
            &request_id,
            serde_json::json!({
                "model": harness.model_name,
                "input": "hello"
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let upstream_request_id = response
        .headers()
        .get("x-upstream-request-id")
        .and_then(|value| value.to_str().ok())
        .expect("gemini embeddings fallback upstream request id")
        .to_string();
    let payload = crate::router::tests::support::response_json(response).await;

    assert_eq!(
        payload["data"][0]["embedding"],
        serde_json::json!([9.0, 8.0])
    );
    assert_eq!(payload["usage"]["total_tokens"], 2);
    assert_eq!(upstream_request_id, "gemini-fallback-embeddings-123");

    let token = harness.wait_for_token_used_quota(2).await;
    assert_eq!(token.used_quota, 2);

    let log = harness.wait_for_log_by_request_id(&request_id).await;
    assert_eq!(log.endpoint, "embeddings");
    assert_eq!(log.request_format, "openai/embeddings");
    assert_eq!(log.requested_model, harness.model_name);
    assert_eq!(log.upstream_model, actual_model);
    assert_eq!(log.model_name, harness.model_name);
    assert_eq!(log.total_tokens, 2);
    assert_eq!(log.quota, 2);
    assert_eq!(log.status_code, 200);
    assert_eq!(log.upstream_request_id, "gemini-fallback-embeddings-123");
    assert_eq!(log.status, LogStatus::Success);

    let primary_account = harness.wait_for_primary_account_rate_limited().await;
    assert_eq!(primary_account.failure_streak, 1);
    assert!(primary_account.rate_limited_until.is_some());
    assert!(primary_account.overload_until.is_none());

    let primary_channel = harness.primary_channel_model().await;
    assert_eq!(primary_channel.failure_streak, 1);
    assert_eq!(primary_channel.last_health_status, 3);

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn gemini_embeddings_route_skips_rate_limited_primary_on_next_request() {
    let actual_model = "text-embedding-004";
    let primary = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: format!("/v1beta/models/{actual_model}:embedContent"),
        expected_header_name: "x-goog-api-key".into(),
        expected_header_value: "sk-primary".into(),
        expected_body_substring: Some("\"content\"".into()),
        additional_expected_headers: vec![],
        additional_expected_body_substrings: vec![
            "\"model\":\"models/text-embedding-004\"".into(),
            "\"text\":\"hello\"".into(),
        ],
        response_status: StatusCode::TOO_MANY_REQUESTS,
        response_content_type: "application/json".into(),
        response_headers: vec![],
        response_body: r#"{"error":{"status":"RESOURCE_EXHAUSTED","message":"slow down"}}"#
            .to_string(),
    })
    .await;
    let fallback = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: format!("/v1beta/models/{actual_model}:embedContent"),
        expected_header_name: "x-goog-api-key".into(),
        expected_header_value: "sk-fallback".into(),
        expected_body_substring: Some("\"content\"".into()),
        additional_expected_headers: vec![],
        additional_expected_body_substrings: vec![
            "\"model\":\"models/text-embedding-004\"".into(),
            "\"text\":\"hello\"".into(),
        ],
        response_status: StatusCode::OK,
        response_content_type: "application/json".into(),
        response_headers: vec![],
        response_body: serde_json::json!({
            "embedding": {
                "values": [9.0, 8.0]
            }
        })
        .to_string(),
    })
    .await;
    let harness = TestHarness::gemini_embeddings_fallback_affinity_fixture(
        &primary.base_url,
        &fallback.base_url,
    )
    .await;
    let first_request_id = format!("gemini-embeddings-rate-limit-first-{}", harness.model_name);
    let second_request_id = format!("gemini-embeddings-rate-limit-second-{}", harness.model_name);

    let first_response = harness
        .json_request(
            Method::POST,
            "/v1/embeddings",
            &first_request_id,
            serde_json::json!({
                "model": harness.model_name,
                "input": "hello"
            }),
        )
        .await;
    assert_eq!(first_response.status(), StatusCode::OK);
    let first_payload = crate::router::tests::support::response_json(first_response).await;
    assert_eq!(
        first_payload["data"][0]["embedding"],
        serde_json::json!([9.0, 8.0])
    );

    let primary_account = harness.wait_for_primary_account_rate_limited().await;
    assert_eq!(primary_account.failure_streak, 1);
    assert!(primary_account.rate_limited_until.is_some());

    let second_response = harness
        .json_request(
            Method::POST,
            "/v1/embeddings",
            &second_request_id,
            serde_json::json!({
                "model": harness.model_name,
                "input": "hello"
            }),
        )
        .await;
    assert_eq!(second_response.status(), StatusCode::OK);
    let second_payload = crate::router::tests::support::response_json(second_response).await;
    assert_eq!(
        second_payload["data"][0]["embedding"],
        serde_json::json!([9.0, 8.0])
    );

    let token = harness.wait_for_token_used_quota(4).await;
    assert_eq!(token.used_quota, 4);

    assert_eq!(
        primary.hit_count(&format!("/v1beta/models/{actual_model}:embedContent")),
        1
    );
    assert_eq!(
        fallback.hit_count(&format!("/v1beta/models/{actual_model}:embedContent")),
        2
    );

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn gemini_embeddings_route_quarantines_primary_account_after_auth_failure() {
    let actual_model = "text-embedding-004";
    let primary = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: format!("/v1beta/models/{actual_model}:embedContent"),
        expected_header_name: "x-goog-api-key".into(),
        expected_header_value: "sk-primary".into(),
        expected_body_substring: Some("\"content\"".into()),
        additional_expected_headers: vec![],
        additional_expected_body_substrings: vec![
            "\"model\":\"models/text-embedding-004\"".into(),
            "\"text\":\"hello\"".into(),
        ],
        response_status: StatusCode::UNAUTHORIZED,
        response_content_type: "application/json".into(),
        response_headers: vec![],
        response_body: r#"{"error":{"status":"UNAUTHENTICATED","message":"invalid api key"}}"#
            .to_string(),
    })
    .await;
    let fallback = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: format!("/v1beta/models/{actual_model}:embedContent"),
        expected_header_name: "x-goog-api-key".into(),
        expected_header_value: "sk-fallback".into(),
        expected_body_substring: Some("\"content\"".into()),
        additional_expected_headers: vec![],
        additional_expected_body_substrings: vec![
            "\"model\":\"models/text-embedding-004\"".into(),
            "\"text\":\"hello\"".into(),
        ],
        response_status: StatusCode::OK,
        response_content_type: "application/json".into(),
        response_headers: vec![],
        response_body: serde_json::json!({
            "embedding": {
                "values": [9.0, 8.0]
            }
        })
        .to_string(),
    })
    .await;
    let harness = TestHarness::gemini_embeddings_fallback_affinity_fixture(
        &primary.base_url,
        &fallback.base_url,
    )
    .await;
    let first_request_id = format!("gemini-embeddings-auth-first-{}", harness.model_name);
    let second_request_id = format!("gemini-embeddings-auth-second-{}", harness.model_name);

    let first_response = harness
        .json_request(
            Method::POST,
            "/v1/embeddings",
            &first_request_id,
            serde_json::json!({
                "model": harness.model_name,
                "input": "hello"
            }),
        )
        .await;
    assert_eq!(first_response.status(), StatusCode::OK);
    let first_payload = crate::router::tests::support::response_json(first_response).await;
    assert_eq!(
        first_payload["data"][0]["embedding"],
        serde_json::json!([9.0, 8.0])
    );

    let primary_account = harness.wait_for_primary_account_disabled().await;
    assert_eq!(
        primary_account.status,
        summer_ai_model::entity::channel_account::AccountStatus::Disabled
    );
    assert!(!primary_account.schedulable);
    assert_eq!(primary_account.failure_streak, 1);

    let primary_channel = harness.primary_channel_model().await;
    assert_eq!(primary_channel.failure_streak, 0);
    assert_eq!(primary_channel.last_health_status, 2);

    let second_response = harness
        .json_request(
            Method::POST,
            "/v1/embeddings",
            &second_request_id,
            serde_json::json!({
                "model": harness.model_name,
                "input": "hello"
            }),
        )
        .await;
    assert_eq!(second_response.status(), StatusCode::OK);
    let second_payload = crate::router::tests::support::response_json(second_response).await;
    assert_eq!(
        second_payload["data"][0]["embedding"],
        serde_json::json!([9.0, 8.0])
    );

    let token = harness.wait_for_token_used_quota(4).await;
    assert_eq!(token.used_quota, 4);

    assert_eq!(
        primary.hit_count(&format!("/v1beta/models/{actual_model}:embedContent")),
        1
    );
    assert_eq!(
        fallback.hit_count(&format!("/v1beta/models/{actual_model}:embedContent")),
        2
    );

    harness.cleanup().await;
}

#[tokio::test]
async fn embeddings_mock_upstream_provider_failure() {
    let req = sample_mock_embeddings_request();
    let actual_model = "text-embedding-3-small";
    let (_server, response) = send_mock_embeddings_request(
        1,
        "sk-openai-test",
        &req,
        actual_model,
        MockUpstreamSpec {
            expected_path_and_query: "/v1/embeddings".into(),
            expected_header_name: "authorization".into(),
            expected_header_value: "Bearer sk-openai-test".into(),
            expected_body_substring: Some(format!("\"model\":\"{actual_model}\"")),
            additional_expected_headers: vec![],
            additional_expected_body_substrings: vec![],
            response_status: StatusCode::BAD_REQUEST,
            response_content_type: "application/json".into(),
            response_headers: vec![],
            response_body: r#"{"error":{"message":"bad embedding input","type":"invalid_request_error","code":"invalid_request_error"}}"#
                .to_string(),
        },
    )
    .await;

    let status = response.status();
    let headers = response.headers().clone();
    let body = response.bytes().await.expect("body");
    let failure = classify_upstream_provider_failure(1, status, &headers, &body);
    assert_eq!(failure.scope, UpstreamFailureScope::Channel);
    assert_eq!(failure.error.status, StatusCode::BAD_REQUEST);
    assert_eq!(failure.error.error.error.r#type, "invalid_request_error");
    assert_eq!(failure.error.error.error.message, "bad embedding input");
}

#[test]
fn classify_anthropic_rate_limit_as_account_failure() {
    let failure = classify_upstream_provider_failure(
        3,
        StatusCode::TOO_MANY_REQUESTS,
        &HeaderMap::new(),
        br#"{"type":"error","error":{"type":"rate_limit_error","message":"slow down"}}"#,
    );

    assert_eq!(failure.scope, UpstreamFailureScope::Account);
    assert_eq!(failure.error.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(failure.error.error.error.r#type, "rate_limit_error");
    assert_eq!(
        failure.error.error.error.code.as_deref(),
        Some("rate_limit_error")
    );
    assert_eq!(failure.error.error.error.message, "slow down");
}

#[test]
fn classify_anthropic_invalid_request_as_channel_failure() {
    let failure = classify_upstream_provider_failure(
        3,
        StatusCode::BAD_REQUEST,
        &HeaderMap::new(),
        br#"{"type":"error","error":{"type":"invalid_request_error","message":"bad claude payload"}}"#,
    );

    assert_eq!(failure.scope, UpstreamFailureScope::Channel);
    assert_eq!(failure.error.status, StatusCode::BAD_REQUEST);
    assert_eq!(failure.error.error.error.r#type, "invalid_request_error");
    assert_eq!(
        failure.error.error.error.code.as_deref(),
        Some("invalid_request_error")
    );
    assert_eq!(failure.error.error.error.message, "bad claude payload");
}

#[test]
fn classify_anthropic_new_api_error_as_account_failure() {
    let failure = classify_upstream_provider_failure(
        3,
        StatusCode::INTERNAL_SERVER_ERROR,
        &HeaderMap::new(),
        br#"{"error":{"type":"new_api_error","message":"invalid claude code request"},"type":"error"}"#,
    );

    assert_eq!(failure.scope, UpstreamFailureScope::Account);
    assert_eq!(failure.error.status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(failure.error.error.error.r#type, "server_error");
    assert_eq!(
        failure.error.error.error.code.as_deref(),
        Some("new_api_error")
    );
    assert_eq!(
        failure.error.error.error.message,
        "invalid claude code request"
    );
}

#[test]
fn classify_gemini_invalid_argument_as_channel_failure() {
    let failure = classify_upstream_provider_failure(
        24,
        StatusCode::BAD_REQUEST,
        &HeaderMap::new(),
        br#"{"error":{"status":"INVALID_ARGUMENT","message":"bad tool schema"}}"#,
    );

    assert_eq!(failure.scope, UpstreamFailureScope::Channel);
    assert_eq!(failure.error.status, StatusCode::BAD_REQUEST);
    assert_eq!(failure.error.error.error.r#type, "invalid_request_error");
    assert_eq!(
        failure.error.error.error.code.as_deref(),
        Some("invalid_argument")
    );
    assert_eq!(failure.error.error.error.message, "bad tool schema");
}

#[test]
fn classify_azure_rate_limit_as_account_failure() {
    let failure = classify_upstream_provider_failure(
        14,
        StatusCode::TOO_MANY_REQUESTS,
        &HeaderMap::new(),
        br#"{"error":{"message":"slow down","type":"rate_limit_error","code":"rate_limit_error"}}"#,
    );

    assert_eq!(failure.scope, UpstreamFailureScope::Account);
    assert_eq!(failure.error.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(failure.error.error.error.r#type, "rate_limit_error");
    assert_eq!(
        failure.error.error.error.code.as_deref(),
        Some("rate_limit_error")
    );
    assert_eq!(failure.error.error.error.message, "slow down");
}

#[test]
fn map_adapter_build_error_uses_unsupported_endpoint_contract() {
    let error = map_adapter_build_error(
        "failed to build upstream responses request",
        anyhow::anyhow!("responses endpoint is not supported"),
    );

    assert_eq!(error.status, StatusCode::BAD_GATEWAY);
    assert_eq!(error.error.error.r#type, "upstream_error");
    assert_eq!(
        error.error.error.code.as_deref(),
        Some("unsupported_endpoint")
    );
    assert_eq!(
        error.error.error.message,
        "responses endpoint is not supported"
    );
}

#[test]
fn map_adapter_build_error_keeps_internal_errors_internal() {
    let error = map_adapter_build_error(
        "failed to build upstream embeddings request",
        anyhow::anyhow!("failed to sign request"),
    );

    assert_eq!(error.status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(error.error.error.r#type, "server_error");
    assert!(
        error
            .error
            .error
            .message
            .contains("failed to build upstream embeddings request")
    );
}

#[test]
fn extract_upstream_request_id_supports_oneapi_header() {
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-oneapi-request-id",
        HeaderValue::from_static("2026032622051868099140Z3FLl6h8"),
    );

    assert_eq!(
        extract_upstream_request_id(&headers),
        "2026032622051868099140Z3FLl6h8"
    );
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn list_models_returns_fixture_models_for_token_group() {
    let harness =
        TestHarness::responses_affinity_fixture("http://127.0.0.1:9", "http://127.0.0.1:10").await;

    let response = harness
        .empty_request(Method::GET, "/v1/models", "list-models")
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let payload = crate::router::tests::support::response_json(response).await;

    assert_eq!(payload["object"], "list");
    assert_eq!(payload["data"].as_array().map(Vec::len), Some(1));
    assert_eq!(payload["data"][0]["id"], harness.model_name);

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn retrieve_model_returns_not_found_for_unknown_fixture_model() {
    let harness =
        TestHarness::responses_affinity_fixture("http://127.0.0.1:9", "http://127.0.0.1:10").await;

    let response = harness
        .empty_request(
            Method::GET,
            "/v1/models/missing-test-model",
            "retrieve-model-missing",
        )
        .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let payload = crate::router::tests::support::response_json(response).await;
    assert_eq!(payload["error"]["code"], "not_found");

    harness.cleanup().await;
}
