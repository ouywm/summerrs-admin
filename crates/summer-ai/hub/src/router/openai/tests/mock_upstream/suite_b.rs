use super::*;
use futures::StreamExt;

#[tokio::test]
async fn gemini_chat_non_stream_mock_upstream_success() {
    let req = sample_mock_chat_request(false);
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
            additional_expected_body_substrings: vec![],
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
    let parsed = get_adapter(24)
        .parse_response(response.bytes().await.expect("body"), actual_model)
        .expect("parse gemini response");
    assert_eq!(parsed.model, actual_model);
    assert_eq!(
        parsed.choices[0].message.content,
        serde_json::json!("Hello from Gemini")
    );
    assert_eq!(parsed.usage.total_tokens, 10);
}

#[tokio::test]
async fn gemini_chat_stream_mock_upstream_success() {
    let req = sample_mock_chat_request(true);
    let actual_model = "gemini-2.5-pro";
    let (_server, response) = send_mock_chat_request(
        24,
        "gem-key",
        &req,
        actual_model,
        MockUpstreamSpec {
            expected_path_and_query: format!(
                "/v1beta/models/{actual_model}:streamGenerateContent?alt=sse"
            ),
            expected_header_name: "x-goog-api-key".into(),
            expected_header_value: "gem-key".into(),
            expected_body_substring: Some("\"contents\"".into()),
            additional_expected_headers: vec![],
            additional_expected_body_substrings: vec![],
            response_status: StatusCode::OK,
            response_content_type: "text/event-stream".into(),
            response_headers: vec![],
            response_body:
                "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"Hello\"}]}}]}\n\n\
                 data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\" Gemini\"}]},\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":4,\"candidatesTokenCount\":6,\"totalTokenCount\":10}}\n\n"
                    .into(),
        },
    )
    .await;

    let chunks: Vec<_> = get_adapter(24)
        .parse_stream(response, actual_model)
        .expect("parse stream")
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .filter_map(Result::ok)
        .collect();

    assert!(
        chunks
            .iter()
            .any(|chunk| { chunk.choices[0].delta.content.as_deref() == Some("Hello") })
    );
    let final_chunk = chunks
        .iter()
        .rfind(|chunk| chunk.choices[0].finish_reason.is_some())
        .expect("final chunk");
    assert!(matches!(
        final_chunk.choices[0].finish_reason,
        Some(summer_ai_core::types::common::FinishReason::Stop)
    ));
    assert_eq!(
        final_chunk.usage.as_ref().map(|usage| usage.total_tokens),
        Some(10)
    );
}

#[tokio::test]
async fn gemini_chat_stream_mock_upstream_multiple_candidates() {
    let req = sample_mock_chat_request(true);
    let actual_model = "gemini-2.5-pro";
    let (_server, response) = send_mock_chat_request(
        24,
        "gem-key",
        &req,
        actual_model,
        MockUpstreamSpec {
            expected_path_and_query: format!(
                "/v1beta/models/{actual_model}:streamGenerateContent?alt=sse"
            ),
            expected_header_name: "x-goog-api-key".into(),
            expected_header_value: "gem-key".into(),
            expected_body_substring: Some("\"contents\"".into()),
            additional_expected_headers: vec![],
            additional_expected_body_substrings: vec![],
            response_status: StatusCode::OK,
            response_content_type: "text/event-stream".into(),
            response_headers: vec![],
            response_body: concat!(
                "data: {\"candidates\":[",
                "{\"content\":{\"parts\":[{\"text\":\"Hello\"}]},\"finishReason\":\"STOP\"},",
                "{\"content\":{\"parts\":[{\"text\":\"Bonjour\"}]},\"finishReason\":\"MAX_TOKENS\"}",
                "],\"usageMetadata\":{\"promptTokenCount\":4,\"candidatesTokenCount\":6,\"totalTokenCount\":10}}\n\n"
            )
            .into(),
        },
    )
    .await;

    let chunks: Vec<_> = get_adapter(24)
        .parse_stream(response, actual_model)
        .expect("parse stream")
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .filter_map(Result::ok)
        .collect();

    assert!(chunks.iter().any(|chunk| {
        chunk.choices[0].index == 0 && chunk.choices[0].delta.content.as_deref() == Some("Hello")
    }));
    assert!(chunks.iter().any(|chunk| {
        chunk.choices[0].index == 1 && chunk.choices[0].delta.content.as_deref() == Some("Bonjour")
    }));
    assert!(chunks.iter().any(|chunk| {
        chunk.choices[0].index == 0
            && matches!(
                chunk.choices[0].finish_reason,
                Some(summer_ai_core::types::common::FinishReason::Stop)
            )
    }));
    assert!(chunks.iter().any(|chunk| {
        chunk.choices[0].index == 1
            && matches!(
                chunk.choices[0].finish_reason,
                Some(summer_ai_core::types::common::FinishReason::Length)
            )
    }));
    let usage_chunks = chunks
        .iter()
        .filter(|chunk| chunk.usage.is_some())
        .collect::<Vec<_>>();
    assert_eq!(usage_chunks.len(), 1);
    assert_eq!(
        usage_chunks[0]
            .usage
            .as_ref()
            .map(|usage| usage.total_tokens),
        Some(10)
    );
}

#[tokio::test]
async fn gemini_chat_stream_mock_upstream_reuses_tool_call_index_across_events() {
    let req = sample_mock_chat_request(true);
    let actual_model = "gemini-2.5-pro";
    let (_server, response) = send_mock_chat_request(
        24,
        "gem-key",
        &req,
        actual_model,
        MockUpstreamSpec {
            expected_path_and_query: format!(
                "/v1beta/models/{actual_model}:streamGenerateContent?alt=sse"
            ),
            expected_header_name: "x-goog-api-key".into(),
            expected_header_value: "gem-key".into(),
            expected_body_substring: Some("\"contents\"".into()),
            additional_expected_headers: vec![],
            additional_expected_body_substrings: vec![],
            response_status: StatusCode::OK,
            response_content_type: "text/event-stream".into(),
            response_headers: vec![],
            response_body: concat!(
                "data: {\"candidates\":[{\"content\":{\"parts\":[{\"functionCall\":{\"name\":\"get_weather\",\"args\":{\"city\":\"Par\"}}}]}}]}\n\n",
                "data: {\"candidates\":[{\"content\":{\"parts\":[{\"functionCall\":{\"name\":\"get_weather\",\"args\":{\"city\":\"Paris\"}}}]}}]}\n\n",
                "data: {\"candidates\":[{\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":4,\"candidatesTokenCount\":2,\"totalTokenCount\":6}}\n\n"
            )
            .into(),
        },
    )
    .await;

    let chunks: Vec<_> = get_adapter(24)
        .parse_stream(response, actual_model)
        .expect("parse stream")
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .filter_map(Result::ok)
        .collect();

    let tool_call_chunks = chunks
        .iter()
        .filter_map(|chunk| chunk.choices[0].delta.tool_calls.as_ref())
        .collect::<Vec<_>>();
    assert_eq!(tool_call_chunks.len(), 2);
    assert_eq!(tool_call_chunks[0][0].index, 0);
    assert_eq!(tool_call_chunks[1][0].index, 0);
    assert_eq!(tool_call_chunks[0][0].id.as_deref(), Some("call_0"));
    assert_eq!(tool_call_chunks[1][0].id.as_deref(), Some("call_0"));
}

#[tokio::test]
async fn gemini_chat_mock_upstream_provider_failure() {
    let req = sample_mock_chat_request(false);
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
            additional_expected_body_substrings: vec![],
            response_status: StatusCode::BAD_REQUEST,
            response_content_type: "application/json".into(),
            response_headers: vec![],
            response_body: r#"{"error":{"status":"INVALID_ARGUMENT","message":"bad tool schema"}}"#
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
    assert_eq!(failure.error.error.error.message, "bad tool schema");
}

#[tokio::test]
async fn gemini_chat_stream_mock_upstream_provider_failure_event() {
    let req = sample_mock_chat_request(true);
    let actual_model = "gemini-2.5-pro";
    let (_server, response) = send_mock_chat_request(
        24,
        "gem-key",
        &req,
        actual_model,
        MockUpstreamSpec {
            expected_path_and_query: format!(
                "/v1beta/models/{actual_model}:streamGenerateContent?alt=sse"
            ),
            expected_header_name: "x-goog-api-key".into(),
            expected_header_value: "gem-key".into(),
            expected_body_substring: Some("\"contents\"".into()),
            additional_expected_headers: vec![],
            additional_expected_body_substrings: vec![],
            response_status: StatusCode::OK,
            response_content_type: "text/event-stream".into(),
            response_headers: vec![],
            response_body: concat!(
                "event: error\n",
                "data: {\"error\":{\"status\":\"INVALID_ARGUMENT\",\"message\":\"bad tool schema\"}}\n\n"
            )
            .into(),
        },
    )
    .await;

    let results = get_adapter(24)
        .parse_stream(response, actual_model)
        .expect("parse stream")
        .collect::<Vec<_>>()
        .await;

    let error = results
        .into_iter()
        .find_map(Result::err)
        .expect("expected gemini stream error");
    let stream_error = error
        .downcast_ref::<summer_ai_core::provider::ProviderStreamError>()
        .expect("expected provider stream error");
    assert_eq!(stream_error.info.kind, ProviderErrorKind::InvalidRequest);
    assert_eq!(stream_error.info.code, "INVALID_ARGUMENT");
    assert_eq!(stream_error.info.message, "bad tool schema");
}

#[tokio::test]
async fn gemini_chat_non_stream_mock_upstream_preserves_safety_settings_extra_body() {
    let req: ChatCompletionRequest = serde_json::from_value(serde_json::json!({
        "model": "gpt-5.4 xhigh",
        "messages": [{"role": "user", "content": "Hello"}],
        "safetySettings": [{
            "category": "HARM_CATEGORY_HATE_SPEECH",
            "threshold": "BLOCK_ONLY_HIGH"
        }]
    }))
    .expect("gemini safety settings request");
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
                "\"safetySettings\"".into(),
                "\"BLOCK_ONLY_HIGH\"".into(),
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
async fn gemini_chat_non_stream_mock_upstream_preserves_response_json_schema() {
    let req: ChatCompletionRequest = serde_json::from_value(serde_json::json!({
        "model": "gpt-5.4 xhigh",
        "messages": [{"role": "user", "content": "Return a person object"}],
        "response_format": {
            "type": "json_schema",
            "json_schema": {
                "name": "person",
                "strict": true,
                "schema": {
                    "type": "object",
                    "properties": {
                        "name": {"type": "string"},
                        "age": {"type": "integer"}
                    },
                    "required": ["name", "age"]
                }
            }
        }
    }))
    .expect("gemini response json schema request");
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
                "\"responseMimeType\":\"application/json\"".into(),
                "\"responseJsonSchema\"".into(),
                "\"required\":[\"name\",\"age\"]".into(),
            ],
            response_status: StatusCode::OK,
            response_content_type: "application/json".into(),
            response_headers: vec![],
            response_body: serde_json::json!({
                "candidates": [{
                    "content": {
                        "parts": [{"text": "{\"name\":\"Ada\",\"age\":36}"}]
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
#[ignore = "requires local postgres and redis"]
async fn anthropic_chat_route_falls_back_after_primary_rate_limit() {
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
            "anthropic-fallback-chat-123".into(),
        )],
        response_body: serde_json::json!({
            "id": "msg_chat_fallback_123",
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
    let harness = TestHarness::anthropic_chat_fallback_affinity_fixture(
        &primary.base_url,
        &fallback.base_url,
    )
    .await;
    let request_id = format!("anthropic-chat-fallback-{}", harness.model_name);
    let baseline_response = harness
        .empty_request(
            Method::GET,
            "/ai/runtime/summary",
            &format!("{request_id}-runtime-baseline"),
        )
        .await;
    assert_eq!(baseline_response.status(), StatusCode::OK);
    let baseline = crate::router::test_support::response_json(baseline_response).await;

    let response = harness
        .json_request(
            Method::POST,
            "/v1/chat/completions",
            &request_id,
            serde_json::json!({
                "model": harness.model_name,
                "messages": [{"role": "user", "content": "Hello"}],
                "stream": false
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let upstream_request_id = response
        .headers()
        .get("x-upstream-request-id")
        .and_then(|value| value.to_str().ok())
        .expect("anthropic fallback chat upstream request id")
        .to_string();
    let payload = crate::router::test_support::response_json(response).await;

    assert_eq!(payload["id"], "msg_chat_fallback_123");
    assert_eq!(
        payload["choices"][0]["message"]["content"],
        "Hello from Claude fallback"
    );
    assert_eq!(payload["usage"]["total_tokens"], 19);
    assert_eq!(upstream_request_id, "anthropic-fallback-chat-123");

    let token = harness.wait_for_token_used_quota(19).await;
    assert_eq!(token.used_quota, 19);

    let log = harness.wait_for_log_by_request_id(&request_id).await;
    assert_eq!(log.endpoint, "chat/completions");
    assert_eq!(log.request_format, "openai/chat_completions");
    assert_eq!(log.requested_model, harness.model_name);
    assert_eq!(log.upstream_model, actual_model);
    assert_eq!(log.model_name, harness.model_name);
    assert_eq!(log.total_tokens, 19);
    assert_eq!(log.quota, 19);
    assert_eq!(log.status_code, 200);
    assert_eq!(log.upstream_request_id, "anthropic-fallback-chat-123");
    assert_eq!(log.status, LogStatus::Success);
    assert!(!log.is_stream);

    let primary_account = harness.wait_for_primary_account_rate_limited().await;
    assert_eq!(primary_account.failure_streak, 1);
    assert!(primary_account.rate_limited_until.is_some());
    assert!(primary_account.overload_until.is_none());

    let primary_channel = harness.primary_channel_model().await;
    assert_eq!(primary_channel.failure_streak, 1);
    assert_eq!(primary_channel.last_health_status, 3);

    let expected_fallback_count = baseline["recentFallbackCount"].as_i64().unwrap_or_default() + 1;
    let mut observed_fallback_count = None;
    for attempt in 0..20 {
        let runtime_response = harness
            .empty_request(
                Method::GET,
                "/ai/runtime/summary",
                &format!("{request_id}-runtime-summary-{attempt}"),
            )
            .await;
        assert_eq!(runtime_response.status(), StatusCode::OK);
        let runtime_payload = crate::router::test_support::response_json(runtime_response).await;
        let fallback_count = runtime_payload["recentFallbackCount"].as_i64();
        if fallback_count == Some(expected_fallback_count) {
            observed_fallback_count = fallback_count;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert_eq!(observed_fallback_count, Some(expected_fallback_count));

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn anthropic_chat_route_skips_rate_limited_primary_on_next_request() {
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
            "id": "msg_chat_fallback_skip_123",
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
    let harness = TestHarness::anthropic_chat_fallback_affinity_fixture(
        &primary.base_url,
        &fallback.base_url,
    )
    .await;
    let first_request_id = format!("anthropic-chat-rate-limit-first-{}", harness.model_name);
    let second_request_id = format!("anthropic-chat-rate-limit-second-{}", harness.model_name);

    let first_response = harness
        .json_request(
            Method::POST,
            "/v1/chat/completions",
            &first_request_id,
            serde_json::json!({
                "model": harness.model_name,
                "messages": [{"role": "user", "content": "Hello"}],
                "stream": false
            }),
        )
        .await;
    assert_eq!(first_response.status(), StatusCode::OK);
    let first_payload = crate::router::test_support::response_json(first_response).await;
    assert_eq!(first_payload["id"], "msg_chat_fallback_skip_123");

    let primary_account = harness.wait_for_primary_account_rate_limited().await;
    assert_eq!(primary_account.failure_streak, 1);
    assert!(primary_account.rate_limited_until.is_some());

    let second_response = harness
        .json_request(
            Method::POST,
            "/v1/chat/completions",
            &second_request_id,
            serde_json::json!({
                "model": harness.model_name,
                "messages": [{"role": "user", "content": "Hello"}],
                "stream": false
            }),
        )
        .await;
    assert_eq!(second_response.status(), StatusCode::OK);
    let second_payload = crate::router::test_support::response_json(second_response).await;
    assert_eq!(second_payload["id"], "msg_chat_fallback_skip_123");

    let token = harness.wait_for_token_used_quota(38).await;
    assert_eq!(token.used_quota, 38);

    assert_eq!(primary.hit_count("/v1/messages"), 1);
    assert_eq!(fallback.hit_count("/v1/messages"), 2);

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn anthropic_chat_route_keeps_short_term_penalty_after_manual_primary_recovery() {
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
            "id": "msg_chat_recovered_penalty_123",
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
    let harness = TestHarness::anthropic_chat_fallback_affinity_fixture(
        &primary.base_url,
        &fallback.base_url,
    )
    .await;
    let first_request_id = format!("anthropic-chat-penalty-first-{}", harness.model_name);
    let second_request_id = format!("anthropic-chat-penalty-second-{}", harness.model_name);

    let first_response = harness
        .json_request(
            Method::POST,
            "/v1/chat/completions",
            &first_request_id,
            serde_json::json!({
                "model": harness.model_name,
                "messages": [{"role": "user", "content": "Hello"}],
                "stream": false
            }),
        )
        .await;
    assert_eq!(first_response.status(), StatusCode::OK);
    let first_payload = crate::router::test_support::response_json(first_response).await;
    assert_eq!(first_payload["id"], "msg_chat_recovered_penalty_123");

    let primary_account = harness.wait_for_primary_account_rate_limited().await;
    assert_eq!(primary_account.failure_streak, 1);
    assert!(primary_account.rate_limited_until.is_some());

    harness.reset_primary_persistent_route_state().await;

    let primary_channel_id = harness.primary_channel_model().await.id;
    let primary_account_id = harness.primary_account_model().await.id;
    let mut observed_recent_penalty = None;
    for attempt in 0..20 {
        let runtime_response = harness
            .empty_request(
                Method::GET,
                "/ai/runtime/health",
                &format!("{second_request_id}-runtime-health-{attempt}"),
            )
            .await;
        assert_eq!(runtime_response.status(), StatusCode::OK);
        let runtime_payload = crate::router::test_support::response_json(runtime_response).await;
        let Some(item) = runtime_payload.as_array().and_then(|items| {
            items
                .iter()
                .find(|item| item["id"].as_i64() == Some(primary_channel_id))
        }) else {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            continue;
        };

        let channel_recent_penalty = item["recentPenaltyCount"].as_i64();
        let account_recent_penalty = item["accounts"]
            .as_array()
            .and_then(|items| {
                items
                    .iter()
                    .find(|account| account["id"].as_i64() == Some(primary_account_id))
            })
            .and_then(|account| account["recentPenaltyCount"].as_i64());

        if channel_recent_penalty == Some(1) && account_recent_penalty == Some(1) {
            observed_recent_penalty = channel_recent_penalty;
            break;
        }

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert_eq!(observed_recent_penalty, Some(1));

    let second_response = harness
        .json_request(
            Method::POST,
            "/v1/chat/completions",
            &second_request_id,
            serde_json::json!({
                "model": harness.model_name,
                "messages": [{"role": "user", "content": "Hello"}],
                "stream": false
            }),
        )
        .await;
    assert_eq!(second_response.status(), StatusCode::OK);
    let second_payload = crate::router::test_support::response_json(second_response).await;
    assert_eq!(second_payload["id"], "msg_chat_recovered_penalty_123");

    assert_eq!(primary.hit_count("/v1/messages"), 1);
    assert_eq!(fallback.hit_count("/v1/messages"), 2);

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn gemini_chat_route_falls_back_after_primary_invalid_request_without_quarantining_account() {
    let actual_model = "gemini-2.5-pro";
    let primary = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: format!("/v1beta/models/{actual_model}:generateContent"),
        expected_header_name: "x-goog-api-key".into(),
        expected_header_value: "sk-primary".into(),
        expected_body_substring: Some("\"contents\"".into()),
        additional_expected_headers: vec![],
        additional_expected_body_substrings: vec!["\"text\":\"Hello\"".into()],
        response_status: StatusCode::BAD_REQUEST,
        response_content_type: "application/json".into(),
        response_headers: vec![],
        response_body: r#"{"error":{"status":"INVALID_ARGUMENT","message":"bad tool schema"}}"#
            .to_string(),
    })
    .await;
    let fallback = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: format!("/v1beta/models/{actual_model}:generateContent"),
        expected_header_name: "x-goog-api-key".into(),
        expected_header_value: "sk-fallback".into(),
        expected_body_substring: Some("\"contents\"".into()),
        additional_expected_headers: vec![],
        additional_expected_body_substrings: vec!["\"text\":\"Hello\"".into()],
        response_status: StatusCode::OK,
        response_content_type: "application/json".into(),
        response_headers: vec![],
        response_body: serde_json::json!({
            "candidates": [{
                "content": {
                    "parts": [{"text": "Hello from Gemini fallback"}]
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
    })
    .await;
    let harness =
        TestHarness::gemini_chat_fallback_affinity_fixture(&primary.base_url, &fallback.base_url)
            .await;

    let response = harness
        .json_request(
            Method::POST,
            "/v1/chat/completions",
            "gemini-chat-invalid-request-fallback",
            serde_json::json!({
                "model": harness.model_name,
                "messages": [{"role": "user", "content": "Hello"}],
                "stream": false
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let payload = crate::router::test_support::response_json(response).await;

    assert_eq!(
        payload["choices"][0]["message"]["content"],
        "Hello from Gemini fallback"
    );
    assert_eq!(payload["usage"]["total_tokens"], 10);

    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    let primary_account = harness.primary_account_model().await;
    assert_eq!(
        primary_account.status,
        summer_ai_model::entity::channel_account::AccountStatus::Enabled
    );
    assert!(primary_account.schedulable);
    assert_eq!(primary_account.failure_streak, 0);
    assert!(primary_account.rate_limited_until.is_none());
    assert!(primary_account.overload_until.is_none());

    let primary_channel = harness.primary_channel_model().await;
    assert_eq!(primary_channel.failure_streak, 0);
    assert_eq!(primary_channel.last_health_status, 2);

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn gemini_chat_route_quarantines_primary_account_after_auth_failure() {
    let actual_model = "gemini-2.5-pro";
    let primary = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: format!("/v1beta/models/{actual_model}:generateContent"),
        expected_header_name: "x-goog-api-key".into(),
        expected_header_value: "sk-primary".into(),
        expected_body_substring: Some("\"contents\"".into()),
        additional_expected_headers: vec![],
        additional_expected_body_substrings: vec!["\"text\":\"Hello\"".into()],
        response_status: StatusCode::UNAUTHORIZED,
        response_content_type: "application/json".into(),
        response_headers: vec![],
        response_body: r#"{"error":{"status":"UNAUTHENTICATED","message":"invalid api key"}}"#
            .to_string(),
    })
    .await;
    let fallback = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: format!("/v1beta/models/{actual_model}:generateContent"),
        expected_header_name: "x-goog-api-key".into(),
        expected_header_value: "sk-fallback".into(),
        expected_body_substring: Some("\"contents\"".into()),
        additional_expected_headers: vec![],
        additional_expected_body_substrings: vec!["\"text\":\"Hello\"".into()],
        response_status: StatusCode::OK,
        response_content_type: "application/json".into(),
        response_headers: vec![],
        response_body: serde_json::json!({
            "candidates": [{
                "content": {
                    "parts": [{"text": "Hello from Gemini fallback"}]
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
    })
    .await;
    let harness =
        TestHarness::gemini_chat_fallback_affinity_fixture(&primary.base_url, &fallback.base_url)
            .await;
    let first_request_id = format!("gemini-chat-auth-first-{}", harness.model_name);
    let second_request_id = format!("gemini-chat-auth-second-{}", harness.model_name);

    let first_response = harness
        .json_request(
            Method::POST,
            "/v1/chat/completions",
            &first_request_id,
            serde_json::json!({
                "model": harness.model_name,
                "messages": [{"role": "user", "content": "Hello"}],
                "stream": false
            }),
        )
        .await;
    assert_eq!(first_response.status(), StatusCode::OK);
    let first_payload = crate::router::test_support::response_json(first_response).await;
    assert_eq!(
        first_payload["choices"][0]["message"]["content"],
        "Hello from Gemini fallback"
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
            "/v1/chat/completions",
            &second_request_id,
            serde_json::json!({
                "model": harness.model_name,
                "messages": [{"role": "user", "content": "Hello"}],
                "stream": false
            }),
        )
        .await;
    assert_eq!(second_response.status(), StatusCode::OK);
    let second_payload = crate::router::test_support::response_json(second_response).await;
    assert_eq!(
        second_payload["choices"][0]["message"]["content"],
        "Hello from Gemini fallback"
    );

    let token = harness.wait_for_token_used_quota(20).await;
    assert_eq!(token.used_quota, 20);

    assert_eq!(
        primary.hit_count(&format!("/v1beta/models/{actual_model}:generateContent")),
        1
    );
    assert_eq!(
        fallback.hit_count(&format!("/v1beta/models/{actual_model}:generateContent")),
        2
    );

    harness.cleanup().await;
}
