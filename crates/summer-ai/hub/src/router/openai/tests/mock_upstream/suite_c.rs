use super::*;
use crate::service::openai_responses_stream::ResponsesStreamTracker;
use summer_ai_core::types::embedding::EmbeddingResponse;
use summer_ai_core::types::responses::ResponsesResponse;

#[tokio::test]
async fn gemini_chat_non_stream_mock_upstream_converts_file_uri_image_to_file_data() {
    let req: ChatCompletionRequest = serde_json::from_value(serde_json::json!({
        "model": "gpt-5.4 xhigh",
        "messages": [{
            "role": "user",
            "content": [{
                "type": "image_url",
                "image_url": {
                    "url": "https://generativelanguage.googleapis.com/v1beta/files/file-123",
                    "mime_type": "image/png"
                }
            }]
        }]
    }))
    .expect("gemini file uri request");
    let actual_model = "gemini-2.5-pro";
    let (_server, response) = send_mock_chat_request(
        24,
        "gem-key",
        &req,
        actual_model,
        MockUpstreamSpec {
            expected_path_and_query: format!("/v1beta/models/{actual_model}:generateContent"),
            expected_header_name: "x-goog-api-key".into(),
            expected_header_value: "gem-key".into(),
            expected_body_substring: Some("\"contents\"".into()),
            additional_expected_headers: vec![],
            additional_expected_body_substrings: vec![
                "\"fileData\"".into(),
                "\"mimeType\":\"image/png\"".into(),
                "\"fileUri\":\"https://generativelanguage.googleapis.com/v1beta/files/file-123\""
                    .into(),
            ],
            response_status: StatusCode::OK,
            response_content_type: "application/json".into(),
            response_headers: vec![],
            response_body: serde_json::json!({
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
            })
            .to_string(),
        },
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn azure_legacy_chat_non_stream_mock_upstream_success() {
    let req = sample_mock_chat_request(false);
    let actual_model = "gpt-4o-deployment";
    let (_server, response) = send_mock_chat_request(
        14,
        "azure-key",
        &req,
        actual_model,
        MockUpstreamSpec {
            expected_path_and_query: format!(
                "/openai/deployments/{actual_model}/chat/completions?api-version=2024-10-21"
            ),
            expected_header_name: "api-key".into(),
            expected_header_value: "azure-key".into(),
            expected_body_substring: None,
            additional_expected_headers: vec![],
            additional_expected_body_substrings: vec![],
            response_status: StatusCode::OK,
            response_content_type: "application/json".into(),
            response_headers: vec![],
            response_body: serde_json::json!({
                "id": "chatcmpl_azure_123",
                "object": "chat.completion",
                "created": 1_774_277_000,
                "model": actual_model,
                "choices": [{
                    "index": 0,
                    "message": {"role": "assistant", "content": "Hello from Azure"},
                    "finish_reason": "stop"
                }],
                "usage": {
                    "prompt_tokens": 12,
                    "completion_tokens": 7,
                    "total_tokens": 19
                }
            })
            .to_string(),
        },
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let parsed = get_adapter(14)
        .parse_response(response.bytes().await.expect("body"), actual_model)
        .expect("parse azure response");
    assert_eq!(parsed.model, actual_model);
    assert_eq!(
        parsed.choices[0].message.content,
        serde_json::json!("Hello from Azure")
    );
    assert_eq!(parsed.usage.total_tokens, 19);
}

#[tokio::test]
async fn azure_v1_responses_non_stream_mock_upstream_success() {
    let req = sample_mock_responses_request(false);
    let actual_model = "gpt-4.1-deployment";
    let server = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: "/openai/v1/responses".into(),
        expected_header_name: "api-key".into(),
        expected_header_value: "azure-key".into(),
        expected_body_substring: Some(format!("\"model\":\"{actual_model}\"")),
        additional_expected_headers: vec![],
        additional_expected_body_substrings: vec![],
        response_status: StatusCode::OK,
        response_content_type: "application/json".into(),
        response_headers: vec![],
        response_body: serde_json::json!({
            "id": "resp_azure_123",
            "object": "response",
            "model": actual_model,
            "status": "completed",
            "usage": {
                "input_tokens": 12,
                "output_tokens": 7,
                "total_tokens": 19
            },
            "output_text": "hello from azure responses"
        })
        .to_string(),
    })
    .await;

    let base_url = format!("{}/openai/v1", server.base_url);
    let response =
        send_mock_responses_request_to_base_url(14, &base_url, "azure-key", &req, actual_model)
            .await;

    assert_eq!(response.status(), StatusCode::OK);
    let parsed: ResponsesResponse =
        serde_json::from_slice(&response.bytes().await.expect("body")).expect("responses json");
    assert_eq!(parsed.id, "resp_azure_123");
    assert_eq!(parsed.model, actual_model);
    assert_eq!(
        parsed.usage.as_ref().map(|usage| usage.total_tokens),
        Some(19)
    );
}

#[tokio::test]
async fn azure_legacy_embeddings_non_stream_mock_upstream_success() {
    let req = sample_mock_embeddings_request();
    let actual_model = "text-embedding-3-large-deployment";
    let (_server, response) = send_mock_embeddings_request(
        14,
        "azure-key",
        &req,
        actual_model,
        MockUpstreamSpec {
            expected_path_and_query: format!(
                "/openai/deployments/{actual_model}/embeddings?api-version=2024-10-21"
            ),
            expected_header_name: "api-key".into(),
            expected_header_value: "azure-key".into(),
            expected_body_substring: Some("\"input\":\"hello\"".into()),
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
async fn responses_non_stream_mock_upstream_success() {
    let req = sample_mock_responses_request(false);
    let actual_model = "gpt-5.4-mini";
    let (_server, response) = send_mock_responses_request(
        1,
        "sk-openai-test",
        &req,
        actual_model,
        MockUpstreamSpec {
            expected_path_and_query: "/v1/responses".into(),
            expected_header_name: "authorization".into(),
            expected_header_value: "Bearer sk-openai-test".into(),
            expected_body_substring: Some(format!("\"model\":\"{actual_model}\"")),
            additional_expected_headers: vec![],
            additional_expected_body_substrings: vec![],
            response_status: StatusCode::OK,
            response_content_type: "application/json".into(),
            response_headers: vec![],
            response_body: serde_json::json!({
                "id": "resp_123",
                "object": "response",
                "model": actual_model,
                "status": "completed",
                "usage": {
                    "input_tokens": 12,
                    "output_tokens": 7,
                    "total_tokens": 19
                },
                "output_text": "hello"
            })
            .to_string(),
        },
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let parsed: ResponsesResponse =
        serde_json::from_slice(&response.bytes().await.expect("body")).expect("responses json");
    assert_eq!(parsed.id, "resp_123");
    assert_eq!(parsed.model, actual_model);
    assert_eq!(
        parsed.usage.as_ref().map(|usage| usage.total_tokens),
        Some(19)
    );
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn responses_route_persists_request_and_execution_snapshots() {
    let actual_model = "gpt-5.4-mini";
    let primary = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: "/v1/responses".into(),
        expected_header_name: "authorization".into(),
        expected_header_value: "Bearer sk-primary".into(),
        expected_body_substring: Some(format!("\"model\":\"{actual_model}\"")),
        additional_expected_headers: vec![],
        additional_expected_body_substrings: vec!["\"input\":\"Hello\"".into()],
        response_status: StatusCode::OK,
        response_content_type: "application/json".into(),
        response_headers: vec![("x-request-id".into(), "responses-upstream-123".into())],
        response_body: serde_json::json!({
            "id": "resp_123",
            "object": "response",
            "model": actual_model,
            "status": "completed",
            "usage": {
                "input_tokens": 12,
                "output_tokens": 7,
                "total_tokens": 19
            },
            "output_text": "hello"
        })
        .to_string(),
    })
    .await;
    let harness =
        TestHarness::responses_affinity_fixture(&primary.base_url, "http://127.0.0.1:9").await;
    let request_id = format!("responses-request-tracking-{}", harness.model_name);

    let response = harness
        .json_request(
            Method::POST,
            "/v1/responses",
            &request_id,
            serde_json::json!({
                "model": harness.model_name,
                "input": "Hello"
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let upstream_request_id = response
        .headers()
        .get("x-upstream-request-id")
        .and_then(|value| value.to_str().ok())
        .expect("responses upstream request id")
        .to_string();
    let payload = crate::router::test_support::response_json(response).await;
    assert_eq!(payload["id"], "resp_123");
    assert_eq!(upstream_request_id, "responses-upstream-123");

    let request = harness.wait_for_request_by_request_id(&request_id).await;
    assert_eq!(request.endpoint, "responses");
    assert_eq!(request.request_format, "openai/responses");
    assert_eq!(request.requested_model, harness.model_name);
    assert_eq!(request.upstream_model, actual_model);
    assert_eq!(request.status, RequestStatus::Success);
    assert_eq!(request.response_status_code, 200);
    assert!(!request.is_stream);
    assert_eq!(request.request_body["input"], "Hello");
    assert_eq!(request.request_headers["authorization"], "***");

    let executions = harness
        .wait_for_request_executions_by_request_id(&request_id)
        .await;
    assert_eq!(executions.len(), 1);
    let execution = &executions[0];
    assert_eq!(execution.attempt_no, 1);
    assert_eq!(execution.status, ExecutionStatus::Success);
    assert_eq!(execution.endpoint, "responses");
    assert_eq!(execution.request_format, "openai/responses");
    assert_eq!(execution.requested_model, harness.model_name);
    assert_eq!(execution.upstream_model, actual_model);
    assert_eq!(execution.response_status_code, 200);
    assert_eq!(execution.upstream_request_id, "responses-upstream-123");
    assert_eq!(execution.request_body["model"], actual_model);
    assert_eq!(execution.request_headers["authorization"], "***");

    harness.cleanup().await;
}

#[tokio::test]
async fn responses_stream_tracker_parses_completed_event_from_mock_upstream() {
    let req = sample_mock_responses_request(true);
    let actual_model = "gpt-5.4-mini";
    let (_server, response) = send_mock_responses_request(
        1,
        "sk-openai-test",
        &req,
        actual_model,
        MockUpstreamSpec {
            expected_path_and_query: "/v1/responses".into(),
            expected_header_name: "authorization".into(),
            expected_header_value: "Bearer sk-openai-test".into(),
            expected_body_substring: Some(format!("\"model\":\"{actual_model}\"")),
            additional_expected_headers: vec![],
            additional_expected_body_substrings: vec![],
            response_status: StatusCode::OK,
            response_content_type: "text/event-stream".into(),
            response_headers: vec![],
            response_body: concat!(
                "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_123\",\"model\":\"gpt-5.4-mini\"}}\n\n",
                "data: {\"type\":\"response.output_text.delta\",\"delta\":\"hel\"}\n\n",
                "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_123\",\"model\":\"gpt-5.4-mini\",\"usage\":{\"input_tokens\":12,\"output_tokens\":7,\"total_tokens\":19}}}\n\n",
                "data: [DONE]\n\n"
            )
            .into(),
        },
    )
    .await;

    let body = response.bytes().await.expect("body");
    let mut tracker = ResponsesStreamTracker::default();
    let start = std::time::Instant::now();
    let mut first_token_time = None;
    tracker.ingest(&body, &start, &mut first_token_time);

    assert_eq!(tracker.response_id, "resp_123");
    assert_eq!(tracker.upstream_model, actual_model);
    assert_eq!(
        tracker.usage.as_ref().map(|usage| usage.total_tokens),
        Some(19)
    );
    assert!(first_token_time.is_some());
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn responses_stream_route_persists_request_and_execution_snapshots() {
    let primary = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: "/v1/responses".into(),
        expected_header_name: "authorization".into(),
        expected_header_value: "Bearer sk-primary".into(),
        expected_body_substring: Some("\"stream\":true".into()),
        additional_expected_headers: vec![],
        additional_expected_body_substrings: vec!["\"input\":\"Hello stream\"".into()],
        response_status: StatusCode::OK,
        response_content_type: "text/event-stream".into(),
        response_headers: vec![("x-request-id".into(), "responses-stream-upstream-123".into())],
        response_body: concat!(
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_stream_123\",\"model\":\"gpt-5.4-mini\"}}\n\n",
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"hel\"}\n\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_stream_123\",\"model\":\"gpt-5.4-mini\",\"usage\":{\"input_tokens\":12,\"output_tokens\":7,\"total_tokens\":19}}}\n\n",
            "data: [DONE]\n\n"
        )
        .into(),
    })
    .await;
    let harness =
        TestHarness::responses_affinity_fixture(&primary.base_url, "http://127.0.0.1:9").await;
    let request_id = format!("responses-stream-request-tracking-{}", harness.model_name);

    let response = harness
        .json_request(
            Method::POST,
            "/v1/responses",
            &request_id,
            serde_json::json!({
                "model": harness.model_name,
                "input": "Hello stream",
                "stream": true
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = crate::router::test_support::response_text(response).await;
    assert!(body.contains("response.completed"));

    let request = harness.wait_for_request_by_request_id(&request_id).await;
    assert_eq!(request.endpoint, "responses");
    assert_eq!(request.status, RequestStatus::Success);
    assert_eq!(request.response_status_code, 200);
    assert!(request.is_stream);
    assert_eq!(request.upstream_model, "gpt-5.4-mini");

    let executions = harness
        .wait_for_request_executions_by_request_id(&request_id)
        .await;
    assert_eq!(executions.len(), 1);
    let execution = &executions[0];
    assert_eq!(execution.status, ExecutionStatus::Success);
    assert_eq!(
        execution.upstream_request_id,
        "responses-stream-upstream-123"
    );
    assert_eq!(execution.response_status_code, 200);

    harness.cleanup().await;
}

#[tokio::test]
async fn responses_mock_upstream_provider_failure() {
    let req = sample_mock_responses_request(false);
    let actual_model = "gpt-5.4-mini";
    let (_server, response) = send_mock_responses_request(
        1,
        "sk-openai-test",
        &req,
        actual_model,
        MockUpstreamSpec {
            expected_path_and_query: "/v1/responses".into(),
            expected_header_name: "authorization".into(),
            expected_header_value: "Bearer sk-openai-test".into(),
            expected_body_substring: Some(format!("\"model\":\"{actual_model}\"")),
            additional_expected_headers: vec![],
            additional_expected_body_substrings: vec![],
            response_status: StatusCode::TOO_MANY_REQUESTS,
            response_content_type: "application/json".into(),
            response_headers: vec![],
            response_body: r#"{"error":{"message":"slow down","type":"rate_limit_error","code":"rate_limit_error"}}"#
                .to_string(),
        },
    )
    .await;

    let status = response.status();
    let headers = response.headers().clone();
    let body = response.bytes().await.expect("body");
    let failure = classify_upstream_provider_failure(1, status, &headers, &body);
    assert_eq!(failure.scope, UpstreamFailureScope::Account);
    assert_eq!(failure.error.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(failure.error.error.error.r#type, "rate_limit_error");
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn anthropic_responses_route_bridges_non_stream_chat_response() {
    let actual_model = "claude-sonnet-4-20250514";
    let primary = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: "/v1/messages".into(),
        expected_header_name: "x-api-key".into(),
        expected_header_value: "sk-primary".into(),
        expected_body_substring: Some(format!("\"model\":\"{actual_model}\"")),
        additional_expected_headers: vec![("anthropic-version".into(), "2023-06-01".into())],
        additional_expected_body_substrings: vec!["\"text\":\"Hello\"".into()],
        response_status: StatusCode::OK,
        response_content_type: "application/json".into(),
        response_headers: vec![(
            "anthropic-request-id".into(),
            "anthropic-upstream-responses-123".into(),
        )],
        response_body: serde_json::json!({
            "id": "msg_resp_123",
            "model": actual_model,
            "content": [{"type": "text", "text": "Hello from Claude responses bridge"}],
            "stop_reason": "end_turn",
            "usage": {
                "input_tokens": 12,
                "output_tokens": 7
            }
        })
        .to_string(),
    })
    .await;
    let harness =
        TestHarness::anthropic_responses_affinity_fixture(&primary.base_url, "http://127.0.0.1:9")
            .await;
    let request_id = format!("anthropic-responses-bridge-{}", harness.model_name);

    let response = harness
        .json_request(
            Method::POST,
            "/v1/responses",
            &request_id,
            serde_json::json!({
                "model": harness.model_name,
                "input": "Hello",
                "stream": false
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let upstream_request_id = response
        .headers()
        .get("x-upstream-request-id")
        .and_then(|value| value.to_str().ok())
        .expect("anthropic upstream request id")
        .to_string();
    let payload = crate::router::test_support::response_json(response).await;

    assert_eq!(payload["object"], "response");
    assert_eq!(payload["id"], "msg_resp_123");
    assert_eq!(payload["model"], actual_model);
    assert_eq!(payload["output_text"], "Hello from Claude responses bridge");
    assert_eq!(payload["usage"]["total_tokens"], 19);
    assert_eq!(upstream_request_id, "anthropic-upstream-responses-123");

    let token = harness.wait_for_token_used_quota(19).await;
    assert_eq!(token.used_quota, 19);

    let log = harness.wait_for_log_by_request_id(&request_id).await;
    assert_eq!(log.endpoint, "responses");
    assert_eq!(log.request_format, "openai/responses");
    assert_eq!(log.requested_model, harness.model_name);
    assert_eq!(log.upstream_model, actual_model);
    assert_eq!(log.model_name, harness.model_name);
    assert_eq!(log.total_tokens, 19);
    assert_eq!(log.quota, 19);
    assert_eq!(log.status_code, 200);
    assert_eq!(log.upstream_request_id, "anthropic-upstream-responses-123");
    assert_eq!(log.status, LogStatus::Success);
    assert!(!log.is_stream);

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn gemini_responses_route_bridges_stream_to_response_events() {
    let actual_model = "gemini-2.5-pro";
    let primary = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: format!(
            "/v1beta/models/{actual_model}:streamGenerateContent?alt=sse"
        ),
        expected_header_name: "x-goog-api-key".into(),
        expected_header_value: "sk-primary".into(),
        expected_body_substring: Some("\"contents\"".into()),
        additional_expected_headers: vec![],
        additional_expected_body_substrings: vec!["\"text\":\"Hello\"".into()],
        response_status: StatusCode::OK,
        response_content_type: "text/event-stream".into(),
        response_headers: vec![("x-request-id".into(), "gemini-upstream-stream-123".into())],
        response_body: concat!(
            "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"Hello\"}]}}]}\n\n",
            "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\" world\"}]},\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":12,\"candidatesTokenCount\":7,\"totalTokenCount\":19}}\n\n"
        )
        .into(),
    })
    .await;
    let harness =
        TestHarness::gemini_responses_affinity_fixture(&primary.base_url, "http://127.0.0.1:9")
            .await;
    let request_id = format!("gemini-responses-bridge-stream-{}", harness.model_name);

    let response = harness
        .json_request(
            Method::POST,
            "/v1/responses",
            &request_id,
            serde_json::json!({
                "model": harness.model_name,
                "input": "Hello",
                "stream": true
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = crate::router::test_support::response_text(response).await;

    assert!(body.contains("\"type\":\"response.created\""));
    assert!(body.contains("\"type\":\"response.output_text.delta\""));
    assert!(body.contains("\"type\":\"response.completed\""));
    assert!(body.contains("\"total_tokens\":19"));
    assert!(body.contains("Hello world"));

    let token = harness.wait_for_token_used_quota(19).await;
    assert_eq!(token.used_quota, 19);

    let log = harness.wait_for_log_by_request_id(&request_id).await;
    assert_eq!(log.endpoint, "responses");
    assert_eq!(log.request_format, "openai/responses");
    assert_eq!(log.requested_model, harness.model_name);
    assert_eq!(log.upstream_model, actual_model);
    assert_eq!(log.model_name, harness.model_name);
    assert_eq!(log.total_tokens, 19);
    assert_eq!(log.quota, 19);
    assert_eq!(log.status_code, 200);
    assert_eq!(log.upstream_request_id, "gemini-upstream-stream-123");
    assert_eq!(log.status, LogStatus::Success);
    assert!(log.is_stream);

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn anthropic_responses_route_follow_up_reads_bridged_cache() {
    let actual_model = "claude-sonnet-4-20250514";
    let primary = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: "/v1/messages".into(),
        expected_header_name: "x-api-key".into(),
        expected_header_value: "sk-primary".into(),
        expected_body_substring: Some(format!("\"model\":\"{actual_model}\"")),
        additional_expected_headers: vec![("anthropic-version".into(), "2023-06-01".into())],
        additional_expected_body_substrings: vec!["\"text\":\"Hello\"".into()],
        response_status: StatusCode::OK,
        response_content_type: "application/json".into(),
        response_headers: vec![],
        response_body: serde_json::json!({
            "id": "msg_resp_followup_123",
            "model": actual_model,
            "content": [{"type": "text", "text": "Hello from bridged cache"}],
            "stop_reason": "end_turn",
            "usage": {
                "input_tokens": 12,
                "output_tokens": 7
            }
        })
        .to_string(),
    })
    .await;
    let harness =
        TestHarness::anthropic_responses_affinity_fixture(&primary.base_url, "http://127.0.0.1:9")
            .await;

    let create_response = harness
        .json_request(
            Method::POST,
            "/v1/responses",
            "anthropic-responses-bridge-followup-create",
            serde_json::json!({
                "model": harness.model_name,
                "input": "Hello",
                "stream": false
            }),
        )
        .await;
    assert_eq!(create_response.status(), StatusCode::OK);
    let create_payload = crate::router::test_support::response_json(create_response).await;
    let response_id = create_payload["id"]
        .as_str()
        .expect("bridged response id")
        .to_string();

    let get_response = harness
        .empty_request(
            Method::GET,
            &format!("/v1/responses/{response_id}"),
            "anthropic-responses-bridge-followup-get",
        )
        .await;
    assert_eq!(get_response.status(), StatusCode::OK);
    let get_payload = crate::router::test_support::response_json(get_response).await;
    assert_eq!(get_payload["id"], response_id);
    assert_eq!(get_payload["status"], "completed");
    assert_eq!(get_payload["output_text"], "Hello from bridged cache");

    let input_items_response = harness
        .empty_request(
            Method::GET,
            &format!("/v1/responses/{response_id}/input_items"),
            "anthropic-responses-bridge-followup-input-items",
        )
        .await;
    assert_eq!(input_items_response.status(), StatusCode::OK);
    let input_items_payload =
        crate::router::test_support::response_json(input_items_response).await;
    assert_eq!(input_items_payload["object"], "list");
    assert_eq!(input_items_payload["data"][0]["role"], "user");
    assert_eq!(input_items_payload["data"][0]["content"], "Hello");

    let cancel_response = harness
        .empty_request(
            Method::POST,
            &format!("/v1/responses/{response_id}/cancel"),
            "anthropic-responses-bridge-followup-cancel",
        )
        .await;
    assert_eq!(cancel_response.status(), StatusCode::OK);
    let cancel_payload = crate::router::test_support::response_json(cancel_response).await;
    assert_eq!(cancel_payload["id"], response_id);
    assert_eq!(cancel_payload["status"], "cancelled");

    let get_cancelled_response = harness
        .empty_request(
            Method::GET,
            &format!("/v1/responses/{response_id}"),
            "anthropic-responses-bridge-followup-get-cancelled",
        )
        .await;
    assert_eq!(get_cancelled_response.status(), StatusCode::OK);
    let cancelled_payload =
        crate::router::test_support::response_json(get_cancelled_response).await;
    assert_eq!(cancelled_payload["status"], "cancelled");

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn gemini_responses_route_stream_follow_up_reads_bridged_cache() {
    let actual_model = "gemini-2.5-pro";
    let primary = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: format!(
            "/v1beta/models/{actual_model}:streamGenerateContent?alt=sse"
        ),
        expected_header_name: "x-goog-api-key".into(),
        expected_header_value: "sk-primary".into(),
        expected_body_substring: Some("\"contents\"".into()),
        additional_expected_headers: vec![],
        additional_expected_body_substrings: vec!["\"text\":\"Hello\"".into()],
        response_status: StatusCode::OK,
        response_content_type: "text/event-stream".into(),
        response_headers: vec![],
        response_body: concat!(
            "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"Hello\"}]}}]}\n\n",
            "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\" world\"}]},\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":12,\"candidatesTokenCount\":7,\"totalTokenCount\":19}}\n\n"
        )
        .into(),
    })
    .await;
    let harness =
        TestHarness::gemini_responses_affinity_fixture(&primary.base_url, "http://127.0.0.1:9")
            .await;

    let create_response = harness
        .json_request(
            Method::POST,
            "/v1/responses",
            "gemini-responses-bridge-followup-create",
            serde_json::json!({
                "model": harness.model_name,
                "input": "Hello",
                "stream": true
            }),
        )
        .await;
    assert_eq!(create_response.status(), StatusCode::OK);
    let create_body = crate::router::test_support::response_text(create_response).await;
    let created_event = extract_responses_event(&create_body, "response.created");
    let response_id = created_event["response"]["id"]
        .as_str()
        .expect("stream bridged response id")
        .to_string();

    let cancel_response = harness
        .empty_request(
            Method::POST,
            &format!("/v1/responses/{response_id}/cancel"),
            "gemini-responses-bridge-followup-cancel",
        )
        .await;
    assert_eq!(cancel_response.status(), StatusCode::OK);
    let cancel_payload = crate::router::test_support::response_json(cancel_response).await;
    assert_eq!(cancel_payload["id"], response_id);
    assert_eq!(cancel_payload["status"], "cancelled");

    let get_response = harness
        .empty_request(
            Method::GET,
            &format!("/v1/responses/{response_id}"),
            "gemini-responses-bridge-followup-get",
        )
        .await;
    assert_eq!(get_response.status(), StatusCode::OK);
    let get_payload = crate::router::test_support::response_json(get_response).await;
    assert_eq!(get_payload["id"], response_id);
    assert_eq!(get_payload["status"], "cancelled");
    assert_eq!(get_payload["output_text"], "Hello world");

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn gemini_responses_route_stream_falls_back_after_primary_overload() {
    let actual_model = "gemini-2.5-pro";
    let primary = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: format!(
            "/v1beta/models/{actual_model}:streamGenerateContent?alt=sse"
        ),
        expected_header_name: "x-goog-api-key".into(),
        expected_header_value: "sk-primary".into(),
        expected_body_substring: Some("\"contents\"".into()),
        additional_expected_headers: vec![],
        additional_expected_body_substrings: vec!["\"text\":\"Hello\"".into()],
        response_status: StatusCode::SERVICE_UNAVAILABLE,
        response_content_type: "application/json".into(),
        response_headers: vec![],
        response_body:
            r#"{"error":{"message":"gemini upstream overloaded","type":"server_error","code":"server_error"}}"#
                .to_string(),
    })
    .await;
    let fallback = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: format!(
            "/v1beta/models/{actual_model}:streamGenerateContent?alt=sse"
        ),
        expected_header_name: "x-goog-api-key".into(),
        expected_header_value: "sk-fallback".into(),
        expected_body_substring: Some("\"contents\"".into()),
        additional_expected_headers: vec![],
        additional_expected_body_substrings: vec!["\"text\":\"Hello\"".into()],
        response_status: StatusCode::OK,
        response_content_type: "text/event-stream".into(),
        response_headers: vec![("x-request-id".into(), "gemini-fallback-stream-123".into())],
        response_body: concat!(
            "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"Hello\"}]}}]}\n\n",
            "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\" fallback\"}]},\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":12,\"candidatesTokenCount\":7,\"totalTokenCount\":19}}\n\n"
        )
        .into(),
    })
    .await;
    let harness = TestHarness::gemini_responses_fallback_affinity_fixture(
        &primary.base_url,
        &fallback.base_url,
    )
    .await;
    let request_id = format!("gemini-responses-stream-fallback-{}", harness.model_name);

    let response = harness
        .json_request(
            Method::POST,
            "/v1/responses",
            &request_id,
            serde_json::json!({
                "model": harness.model_name,
                "input": "Hello",
                "stream": true
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let upstream_request_id = response
        .headers()
        .get("x-upstream-request-id")
        .and_then(|value| value.to_str().ok())
        .expect("gemini fallback responses stream upstream request id")
        .to_string();
    let body = crate::router::test_support::response_text(response).await;

    assert!(body.contains("\"type\":\"response.created\""));
    assert!(body.contains("\"type\":\"response.completed\""));
    assert!(body.contains("\"total_tokens\":19"));
    assert!(body.contains("Hello fallback"));
    assert_eq!(upstream_request_id, "gemini-fallback-stream-123");

    let token = harness.wait_for_token_used_quota(19).await;
    assert_eq!(token.used_quota, 19);

    let log = harness.wait_for_log_by_request_id(&request_id).await;
    assert_eq!(log.endpoint, "responses");
    assert_eq!(log.request_format, "openai/responses");
    assert_eq!(log.requested_model, harness.model_name);
    assert_eq!(log.upstream_model, actual_model);
    assert_eq!(log.model_name, harness.model_name);
    assert_eq!(log.total_tokens, 19);
    assert_eq!(log.quota, 19);
    assert_eq!(log.status_code, 200);
    assert_eq!(log.upstream_request_id, "gemini-fallback-stream-123");
    assert_eq!(log.status, LogStatus::Success);
    assert!(log.is_stream);

    let primary_account = harness.wait_for_primary_account_overloaded().await;
    assert_eq!(primary_account.failure_streak, 1);
    assert!(primary_account.overload_until.is_some());
    assert!(primary_account.rate_limited_until.is_none());

    let primary_channel = harness.primary_channel_model().await;
    assert_eq!(primary_channel.failure_streak, 1);
    assert_eq!(primary_channel.last_health_status, 3);

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn gemini_responses_route_stream_skips_overloaded_primary_on_next_request() {
    let actual_model = "gemini-2.5-pro";
    let primary = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: format!(
            "/v1beta/models/{actual_model}:streamGenerateContent?alt=sse"
        ),
        expected_header_name: "x-goog-api-key".into(),
        expected_header_value: "sk-primary".into(),
        expected_body_substring: Some("\"contents\"".into()),
        additional_expected_headers: vec![],
        additional_expected_body_substrings: vec!["\"text\":\"Hello\"".into()],
        response_status: StatusCode::SERVICE_UNAVAILABLE,
        response_content_type: "application/json".into(),
        response_headers: vec![],
        response_body:
            r#"{"error":{"message":"gemini upstream overloaded","type":"server_error","code":"server_error"}}"#
                .to_string(),
    })
    .await;
    let fallback = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: format!(
            "/v1beta/models/{actual_model}:streamGenerateContent?alt=sse"
        ),
        expected_header_name: "x-goog-api-key".into(),
        expected_header_value: "sk-fallback".into(),
        expected_body_substring: Some("\"contents\"".into()),
        additional_expected_headers: vec![],
        additional_expected_body_substrings: vec!["\"text\":\"Hello\"".into()],
        response_status: StatusCode::OK,
        response_content_type: "text/event-stream".into(),
        response_headers: vec![],
        response_body: concat!(
            "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"Hello\"}]}}]}\n\n",
            "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\" fallback\"}]},\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":12,\"candidatesTokenCount\":7,\"totalTokenCount\":19}}\n\n"
        )
        .into(),
    })
    .await;
    let harness = TestHarness::gemini_responses_fallback_affinity_fixture(
        &primary.base_url,
        &fallback.base_url,
    )
    .await;
    let first_request_id = format!("gemini-responses-overload-first-{}", harness.model_name);
    let second_request_id = format!("gemini-responses-overload-second-{}", harness.model_name);

    let first_response = harness
        .json_request(
            Method::POST,
            "/v1/responses",
            &first_request_id,
            serde_json::json!({
                "model": harness.model_name,
                "input": "Hello",
                "stream": true
            }),
        )
        .await;
    assert_eq!(first_response.status(), StatusCode::OK);
    let first_body = crate::router::test_support::response_text(first_response).await;
    assert!(first_body.contains("\"type\":\"response.completed\""));
    assert!(first_body.contains("Hello fallback"));

    let primary_account = harness.wait_for_primary_account_overloaded().await;
    assert_eq!(primary_account.failure_streak, 1);
    assert!(primary_account.overload_until.is_some());

    let second_response = harness
        .json_request(
            Method::POST,
            "/v1/responses",
            &second_request_id,
            serde_json::json!({
                "model": harness.model_name,
                "input": "Hello",
                "stream": true
            }),
        )
        .await;
    assert_eq!(second_response.status(), StatusCode::OK);
    let second_body = crate::router::test_support::response_text(second_response).await;
    assert!(second_body.contains("\"type\":\"response.completed\""));
    assert!(second_body.contains("Hello fallback"));

    let token = harness.wait_for_token_used_quota(38).await;
    assert_eq!(token.used_quota, 38);

    assert_eq!(
        primary.hit_count(&format!(
            "/v1beta/models/{actual_model}:streamGenerateContent?alt=sse"
        )),
        1
    );
    assert_eq!(
        fallback.hit_count(&format!(
            "/v1beta/models/{actual_model}:streamGenerateContent?alt=sse"
        )),
        2
    );

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn anthropic_responses_route_falls_back_after_primary_rate_limit() {
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
        response_headers: vec![(
            "anthropic-request-id".into(),
            "anthropic-fallback-responses-123".into(),
        )],
        response_body: serde_json::json!({
            "id": "msg_resp_fallback_123",
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
    let request_id = format!("anthropic-responses-fallback-{}", harness.model_name);

    let response = harness
        .json_request(
            Method::POST,
            "/v1/responses",
            &request_id,
            serde_json::json!({
                "model": harness.model_name,
                "input": "Hello",
                "stream": false
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let upstream_request_id = response
        .headers()
        .get("x-upstream-request-id")
        .and_then(|value| value.to_str().ok())
        .expect("anthropic fallback responses upstream request id")
        .to_string();
    let payload = crate::router::test_support::response_json(response).await;

    assert_eq!(payload["id"], "msg_resp_fallback_123");
    assert_eq!(payload["output_text"], "Hello from Claude fallback");
    assert_eq!(payload["usage"]["total_tokens"], 19);
    assert_eq!(upstream_request_id, "anthropic-fallback-responses-123");

    let token = harness.wait_for_token_used_quota(19).await;
    assert_eq!(token.used_quota, 19);

    let log = harness.wait_for_log_by_request_id(&request_id).await;
    assert_eq!(log.endpoint, "responses");
    assert_eq!(log.request_format, "openai/responses");
    assert_eq!(log.requested_model, harness.model_name);
    assert_eq!(log.upstream_model, actual_model);
    assert_eq!(log.model_name, harness.model_name);
    assert_eq!(log.total_tokens, 19);
    assert_eq!(log.quota, 19);
    assert_eq!(log.status_code, 200);
    assert_eq!(log.upstream_request_id, "anthropic-fallback-responses-123");
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
