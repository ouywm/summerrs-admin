use super::*;
use futures::StreamExt;

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn openai_moderations_route_native_success() {
    let primary = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: "/v1/moderations".into(),
        expected_header_name: "authorization".into(),
        expected_header_value: "Bearer sk-primary".into(),
        expected_body_substring: Some("\"model\":\"".into()),
        additional_expected_headers: vec![],
        additional_expected_body_substrings: vec!["\"input\":\"moderate this text\"".into()],
        response_status: StatusCode::OK,
        response_content_type: "application/json".into(),
        response_headers: vec![(
            "x-request-id".into(),
            "openai-upstream-moderations-123".into(),
        )],
        response_body: serde_json::json!({
            "id": "modr_123",
            "model": "upstream-moderation-model",
            "results": [{
                "flagged": false,
                "categories": {
                    "violence": false
                }
            }]
        })
        .to_string(),
    })
    .await;
    let harness =
        TestHarness::moderations_affinity_fixture(&primary.base_url, "http://127.0.0.1:9").await;
    let request_id = format!("openai-moderations-native-{}", harness.model_name);

    let response = harness
        .json_request(
            Method::POST,
            "/v1/moderations",
            &request_id,
            serde_json::json!({
                "model": harness.model_name,
                "input": "moderate this text"
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let upstream_request_id = response
        .headers()
        .get("x-upstream-request-id")
        .and_then(|value| value.to_str().ok())
        .expect("moderations upstream request id")
        .to_string();
    let payload = crate::router::tests::support::response_json(response).await;

    assert_eq!(payload["id"], "modr_123");
    assert_eq!(payload["results"][0]["flagged"], false);
    assert_eq!(upstream_request_id, "openai-upstream-moderations-123");

    let token = harness.wait_for_token_used_quota(3).await;
    assert_eq!(token.used_quota, 3);

    let log = harness.wait_for_log_by_request_id(&request_id).await;
    assert_eq!(log.endpoint, "moderations");
    assert_eq!(log.request_format, "openai/moderations");
    assert_eq!(log.requested_model, harness.model_name);
    assert_eq!(log.upstream_model, "upstream-moderation-model");
    assert_eq!(log.model_name, harness.model_name);
    assert_eq!(log.total_tokens, 3);
    assert_eq!(log.quota, 3);
    assert_eq!(log.status_code, 200);
    assert_eq!(log.upstream_request_id, "openai-upstream-moderations-123");
    assert_eq!(log.status, LogStatus::Success);
    assert!(!log.is_stream);

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn anthropic_moderations_route_returns_unsupported_endpoint() {
    let harness = TestHarness::anthropic_moderations_affinity_fixture(
        "http://127.0.0.1:9",
        "http://127.0.0.1:10",
    )
    .await;
    let request_id = format!("anthropic-moderations-unsupported-{}", harness.model_name);

    let response = harness
        .json_request(
            Method::POST,
            "/v1/moderations",
            &request_id,
            serde_json::json!({
                "model": harness.model_name,
                "input": "moderate this text"
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    let payload = crate::router::tests::support::response_json(response).await;

    assert_eq!(payload["error"]["type"], "upstream_error");
    assert_eq!(payload["error"]["code"], "unsupported_endpoint");

    let log = harness.wait_for_log_by_request_id(&request_id).await;
    assert_eq!(log.endpoint, "moderations");
    assert_eq!(log.request_format, "openai/moderations");
    assert_eq!(log.requested_model, harness.model_name);
    assert_eq!(log.status, LogStatus::Failed);
    assert_eq!(log.status_code, 0);

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn anthropic_rerank_route_returns_unsupported_endpoint() {
    let harness =
        TestHarness::anthropic_rerank_affinity_fixture("http://127.0.0.1:9", "http://127.0.0.1:10")
            .await;
    let request_id = format!("anthropic-rerank-unsupported-{}", harness.model_name);

    let response = harness
        .json_request(
            Method::POST,
            "/v1/rerank",
            &request_id,
            serde_json::json!({
                "model": harness.model_name,
                "query": "rerank this query",
                "documents": ["doc 1", "doc 2"]
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    let payload = crate::router::tests::support::response_json(response).await;

    assert_eq!(payload["error"]["type"], "upstream_error");
    assert_eq!(payload["error"]["code"], "unsupported_endpoint");

    let log = harness.wait_for_log_by_request_id(&request_id).await;
    assert_eq!(log.endpoint, "rerank");
    assert_eq!(log.request_format, "openai/rerank");
    assert_eq!(log.requested_model, harness.model_name);
    assert_eq!(log.status, LogStatus::Failed);
    assert_eq!(log.status_code, 0);

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn openai_rerank_route_native_success() {
    let primary = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: "/v1/rerank".into(),
        expected_header_name: "authorization".into(),
        expected_header_value: "Bearer sk-primary".into(),
        expected_body_substring: Some("\"model\":\"".into()),
        additional_expected_headers: vec![],
        additional_expected_body_substrings: vec![
            "\"query\":\"alpha beta\"".into(),
            "\"documents\":[\"gamma\",\"delta\"]".into(),
        ],
        response_status: StatusCode::OK,
        response_content_type: "application/json".into(),
        response_headers: vec![("x-request-id".into(), "openai-upstream-rerank-123".into())],
        response_body: serde_json::json!({
            "id": "rerank_123",
            "model": "upstream-rerank-model",
            "results": [{
                "index": 1,
                "relevance_score": 0.98
            }, {
                "index": 0,
                "relevance_score": 0.72
            }]
        })
        .to_string(),
    })
    .await;
    let harness =
        TestHarness::rerank_affinity_fixture(&primary.base_url, "http://127.0.0.1:9").await;
    let request_id = format!("openai-rerank-native-{}", harness.model_name);

    let response = harness
        .json_request(
            Method::POST,
            "/v1/rerank",
            &request_id,
            serde_json::json!({
                "model": harness.model_name,
                "query": "alpha beta",
                "documents": ["gamma", "delta"]
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let upstream_request_id = response
        .headers()
        .get("x-upstream-request-id")
        .and_then(|value| value.to_str().ok())
        .expect("rerank upstream request id")
        .to_string();
    let payload = crate::router::tests::support::response_json(response).await;

    assert_eq!(payload["id"], "rerank_123");
    assert_eq!(payload["results"][0]["index"], 1);
    assert_eq!(payload["results"][0]["relevance_score"], 0.98);
    assert_eq!(upstream_request_id, "openai-upstream-rerank-123");

    let token = harness.wait_for_token_used_quota(4).await;
    assert_eq!(token.used_quota, 4);

    let log = harness.wait_for_log_by_request_id(&request_id).await;
    assert_eq!(log.endpoint, "rerank");
    assert_eq!(log.request_format, "openai/rerank");
    assert_eq!(log.requested_model, harness.model_name);
    assert_eq!(log.upstream_model, "upstream-rerank-model");
    assert_eq!(log.model_name, harness.model_name);
    assert_eq!(log.total_tokens, 4);
    assert_eq!(log.quota, 4);
    assert_eq!(log.status_code, 200);
    assert_eq!(log.upstream_request_id, "openai-upstream-rerank-123");
    assert_eq!(log.status, LogStatus::Success);
    assert!(!log.is_stream);

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn anthropic_image_generations_route_returns_unsupported_endpoint() {
    let harness =
        TestHarness::anthropic_images_affinity_fixture("http://127.0.0.1:9", "http://127.0.0.1:10")
            .await;
    let request_id = format!("anthropic-images-unsupported-{}", harness.model_name);

    let response = harness
        .json_request(
            Method::POST,
            "/v1/images/generations",
            &request_id,
            serde_json::json!({
                "model": harness.model_name,
                "prompt": "draw a sunset"
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    let payload = crate::router::tests::support::response_json(response).await;

    assert_eq!(payload["error"]["type"], "upstream_error");
    assert_eq!(payload["error"]["code"], "unsupported_endpoint");

    let log = harness.wait_for_log_by_request_id(&request_id).await;
    assert_eq!(log.endpoint, "images/generations");
    assert_eq!(log.request_format, "openai/images_generations");
    assert_eq!(log.requested_model, harness.model_name);
    assert_eq!(log.status, LogStatus::Failed);
    assert_eq!(log.status_code, 0);

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn anthropic_image_edits_route_returns_unsupported_endpoint() {
    let harness =
        TestHarness::anthropic_images_affinity_fixture("http://127.0.0.1:9", "http://127.0.0.1:10")
            .await;
    let request_id = format!("anthropic-image-edits-unsupported-{}", harness.model_name);

    let response = harness
        .multipart_request(MultipartRequestSpec {
            uri: "/v1/images/edits",
            request_id: &request_id,
            text_fields: &[
                ("model", &harness.model_name),
                ("prompt", "edit this image"),
            ],
            file_field_name: "image",
            file_name: "edit-primary.png",
            file_content_type: "image/png",
            file_bytes: b"png bytes",
        })
        .await;
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    let payload = crate::router::tests::support::response_json(response).await;

    assert_eq!(payload["error"]["type"], "upstream_error");
    assert_eq!(payload["error"]["code"], "unsupported_endpoint");

    let log = harness.wait_for_log_by_request_id(&request_id).await;
    assert_eq!(log.endpoint, "images/edits");
    assert_eq!(log.request_format, "openai/images_edits");
    assert_eq!(log.requested_model, harness.model_name);
    assert_eq!(log.status, LogStatus::Failed);
    assert_eq!(log.status_code, 0);

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn openai_image_edits_route_native_success() {
    let primary = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: "/v1/images/edits".into(),
        expected_header_name: "authorization".into(),
        expected_header_value: "Bearer sk-primary".into(),
        expected_body_substring: Some("edit-primary.png".into()),
        additional_expected_headers: vec![],
        additional_expected_body_substrings: vec![
            "name=\"model\"\r\n\r\n".into(),
            "name=\"prompt\"\r\n\r\nedit this image".into(),
            "name=\"image\"; filename=\"edit-primary.png\"".into(),
        ],
        response_status: StatusCode::OK,
        response_content_type: "application/json".into(),
        response_headers: vec![(
            "x-request-id".into(),
            "openai-upstream-image-edits-123".into(),
        )],
        response_body: serde_json::json!({
            "created": 1,
            "data": [{
                "b64_json": "edit-result"
            }],
        })
        .to_string(),
    })
    .await;
    let harness =
        TestHarness::model_passthrough_affinity_fixture(&primary.base_url, "http://127.0.0.1:9")
            .await;
    let request_id = format!("openai-image-edits-native-{}", harness.model_name);

    let response = harness
        .multipart_request(MultipartRequestSpec {
            uri: "/v1/images/edits",
            request_id: &request_id,
            text_fields: &[
                ("model", &harness.model_name),
                ("prompt", "edit this image"),
            ],
            file_field_name: "image",
            file_name: "edit-primary.png",
            file_content_type: "image/png",
            file_bytes: b"png bytes",
        })
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let upstream_request_id = response
        .headers()
        .get("x-upstream-request-id")
        .and_then(|value| value.to_str().ok())
        .expect("image edit upstream request id")
        .to_string();
    let payload = crate::router::tests::support::response_json(response).await;

    assert_eq!(payload["data"][0]["b64_json"], "edit-result");
    assert_eq!(upstream_request_id, "openai-upstream-image-edits-123");

    let expected_tokens = 4;
    let token = harness.wait_for_token_used_quota(expected_tokens).await;
    assert_eq!(token.used_quota, expected_tokens);

    let log = harness.wait_for_log_by_request_id(&request_id).await;
    assert_eq!(log.endpoint, "images/edits");
    assert_eq!(log.request_format, "openai/images_edits");
    assert_eq!(log.requested_model, harness.model_name);
    assert_eq!(log.upstream_model, harness.model_name);
    assert_eq!(log.model_name, harness.model_name);
    assert_eq!(log.total_tokens, expected_tokens as i32);
    assert_eq!(log.quota, expected_tokens);
    assert_eq!(log.status_code, 200);
    assert_eq!(log.upstream_request_id, "openai-upstream-image-edits-123");
    assert_eq!(log.status, LogStatus::Success);
    assert!(!log.is_stream);

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn anthropic_image_variations_route_returns_unsupported_endpoint() {
    let harness =
        TestHarness::anthropic_images_affinity_fixture("http://127.0.0.1:9", "http://127.0.0.1:10")
            .await;
    let request_id = format!(
        "anthropic-image-variations-unsupported-{}",
        harness.model_name
    );

    let response = harness
        .multipart_request(MultipartRequestSpec {
            uri: "/v1/images/variations",
            request_id: &request_id,
            text_fields: &[("model", &harness.model_name), ("n", "2")],
            file_field_name: "image",
            file_name: "variation-primary.png",
            file_content_type: "image/png",
            file_bytes: b"png bytes",
        })
        .await;
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    let payload = crate::router::tests::support::response_json(response).await;

    assert_eq!(payload["error"]["type"], "upstream_error");
    assert_eq!(payload["error"]["code"], "unsupported_endpoint");

    let log = harness.wait_for_log_by_request_id(&request_id).await;
    assert_eq!(log.endpoint, "images/variations");
    assert_eq!(log.request_format, "openai/images_variations");
    assert_eq!(log.requested_model, harness.model_name);
    assert_eq!(log.status, LogStatus::Failed);
    assert_eq!(log.status_code, 0);

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn openai_image_variations_route_native_success() {
    let primary = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: "/v1/images/variations".into(),
        expected_header_name: "authorization".into(),
        expected_header_value: "Bearer sk-primary".into(),
        expected_body_substring: Some("variation-primary.png".into()),
        additional_expected_headers: vec![],
        additional_expected_body_substrings: vec![
            "name=\"model\"\r\n\r\n".into(),
            "name=\"n\"\r\n\r\n2".into(),
            "name=\"image\"; filename=\"variation-primary.png\"".into(),
        ],
        response_status: StatusCode::OK,
        response_content_type: "application/json".into(),
        response_headers: vec![(
            "x-request-id".into(),
            "openai-upstream-image-variations-123".into(),
        )],
        response_body: serde_json::json!({
            "created": 1,
            "data": [{
                "b64_json": "variation-result"
            }],
        })
        .to_string(),
    })
    .await;
    let harness =
        TestHarness::model_passthrough_affinity_fixture(&primary.base_url, "http://127.0.0.1:9")
            .await;
    let request_id = format!("openai-image-variations-native-{}", harness.model_name);

    let response = harness
        .multipart_request(MultipartRequestSpec {
            uri: "/v1/images/variations",
            request_id: &request_id,
            text_fields: &[("model", &harness.model_name), ("n", "2")],
            file_field_name: "image",
            file_name: "variation-primary.png",
            file_content_type: "image/png",
            file_bytes: b"png bytes",
        })
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let upstream_request_id = response
        .headers()
        .get("x-upstream-request-id")
        .and_then(|value| value.to_str().ok())
        .expect("image variation upstream request id")
        .to_string();
    let payload = crate::router::tests::support::response_json(response).await;

    assert_eq!(payload["data"][0]["b64_json"], "variation-result");
    assert_eq!(upstream_request_id, "openai-upstream-image-variations-123");

    let expected_tokens = 2;
    let token = harness.wait_for_token_used_quota(expected_tokens).await;
    assert_eq!(token.used_quota, expected_tokens);

    let log = harness.wait_for_log_by_request_id(&request_id).await;
    assert_eq!(log.endpoint, "images/variations");
    assert_eq!(log.request_format, "openai/images_variations");
    assert_eq!(log.requested_model, harness.model_name);
    assert_eq!(log.upstream_model, harness.model_name);
    assert_eq!(log.model_name, harness.model_name);
    assert_eq!(log.total_tokens, expected_tokens as i32);
    assert_eq!(log.quota, expected_tokens);
    assert_eq!(log.status_code, 200);
    assert_eq!(
        log.upstream_request_id,
        "openai-upstream-image-variations-123"
    );
    assert_eq!(log.status, LogStatus::Success);
    assert!(!log.is_stream);

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn anthropic_files_list_route_returns_unsupported_endpoint() {
    let harness =
        TestHarness::anthropic_files_affinity_fixture("http://127.0.0.1:9", "http://127.0.0.1:10")
            .await;
    let request_id = "anthropic-files-list-unsupported";

    let response = harness
        .empty_request(Method::GET, "/v1/files", request_id)
        .await;
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    let payload = crate::router::tests::support::response_json(response).await;

    assert_eq!(payload["error"]["type"], "upstream_error");
    assert_eq!(payload["error"]["code"], "unsupported_endpoint");

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn openai_files_upload_route_native_success() {
    let primary = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: "/v1/files".into(),
        expected_header_name: "authorization".into(),
        expected_header_value: "Bearer sk-primary".into(),
        expected_body_substring: Some("fixture.txt".into()),
        additional_expected_headers: vec![],
        additional_expected_body_substrings: vec![
            "name=\"purpose\"\r\n\r\nassistants".into(),
            "name=\"file\"; filename=\"fixture.txt\"".into(),
        ],
        response_status: StatusCode::OK,
        response_content_type: "application/json".into(),
        response_headers: vec![("x-request-id".into(), "openai-files-upload-123".into())],
        response_body: serde_json::json!({
            "id": "file_primary",
            "object": "file",
            "bytes": 5,
            "created_at": 1,
            "filename": "fixture.txt",
            "purpose": "assistants",
            "status": "processed",
        })
        .to_string(),
    })
    .await;
    let harness =
        TestHarness::files_affinity_fixture(&primary.base_url, "http://127.0.0.1:9").await;
    let request_id = "openai-files-upload-native";

    let upload_response = harness
        .multipart_request(MultipartRequestSpec {
            uri: "/v1/files",
            request_id,
            text_fields: &[("purpose", "assistants")],
            file_field_name: "file",
            file_name: "fixture.txt",
            file_content_type: "text/plain",
            file_bytes: b"hello",
        })
        .await;
    assert_eq!(upload_response.status(), StatusCode::OK);
    let upload_upstream_request_id = upload_response
        .headers()
        .get("x-upstream-request-id")
        .and_then(|value| value.to_str().ok())
        .expect("file upload upstream request id")
        .to_string();
    let upload_payload = crate::router::tests::support::response_json(upload_response).await;

    assert_eq!(upload_payload["id"], "file_primary");
    assert_eq!(upload_upstream_request_id, "openai-files-upload-123");
    assert_eq!(primary.hit_count("/v1/files"), 1);

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn files_upload_route_rejects_payload_over_limit() {
    let harness =
        TestHarness::files_affinity_fixture("http://127.0.0.1:9", "http://127.0.0.1:10").await;
    let request_id = "files-upload-over-limit";
    let oversized = vec![b'x'; MAX_MULTIPART_FILE_SIZE_BYTES + 1];

    let response = harness
        .multipart_request(MultipartRequestSpec {
            uri: "/v1/files",
            request_id,
            text_fields: &[("purpose", "assistants")],
            file_field_name: "file",
            file_name: "oversized.txt",
            file_content_type: "text/plain",
            file_bytes: &oversized,
        })
        .await;
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    let payload = crate::router::tests::support::response_json(response).await;

    assert_eq!(payload["error"]["type"], "invalid_request_error");
    assert_eq!(payload["error"]["code"], "payload_too_large");

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn anthropic_audio_speech_route_returns_unsupported_endpoint() {
    let harness =
        TestHarness::anthropic_audio_affinity_fixture("http://127.0.0.1:9", "http://127.0.0.1:10")
            .await;
    let request_id = format!("anthropic-audio-speech-unsupported-{}", harness.model_name);

    let response = harness
        .json_request(
            Method::POST,
            "/v1/audio/speech",
            &request_id,
            serde_json::json!({
                "model": harness.model_name,
                "input": "say hello from native",
                "voice": "alloy"
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    let payload = crate::router::tests::support::response_json(response).await;

    assert_eq!(payload["error"]["type"], "upstream_error");
    assert_eq!(payload["error"]["code"], "unsupported_endpoint");

    let log = harness.wait_for_log_by_request_id(&request_id).await;
    assert_eq!(log.endpoint, "audio/speech");
    assert_eq!(log.request_format, "openai/audio_speech");
    assert_eq!(log.requested_model, harness.model_name);
    assert_eq!(log.status, LogStatus::Failed);
    assert_eq!(log.status_code, 0);

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn openai_audio_speech_route_native_success() {
    let primary = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: "/v1/audio/speech".into(),
        expected_header_name: "authorization".into(),
        expected_header_value: "Bearer sk-primary".into(),
        expected_body_substring: Some("\"model\":\"".into()),
        additional_expected_headers: vec![],
        additional_expected_body_substrings: vec![
            "\"input\":\"say hello from native\"".into(),
            "\"voice\":\"alloy\"".into(),
        ],
        response_status: StatusCode::OK,
        response_content_type: "audio/mpeg".into(),
        response_headers: vec![(
            "x-request-id".into(),
            "openai-upstream-audio-speech-123".into(),
        )],
        response_body: "native-audio".into(),
    })
    .await;
    let harness =
        TestHarness::audio_affinity_fixture(&primary.base_url, "http://127.0.0.1:9").await;
    let request_id = format!("openai-audio-speech-native-{}", harness.model_name);

    let response = harness
        .json_request(
            Method::POST,
            "/v1/audio/speech",
            &request_id,
            serde_json::json!({
                "model": harness.model_name,
                "input": "say hello from native",
                "voice": "alloy"
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let upstream_request_id = response
        .headers()
        .get("x-upstream-request-id")
        .and_then(|value| value.to_str().ok())
        .expect("audio speech upstream request id")
        .to_string();
    let payload = crate::router::tests::support::response_text(response).await;

    assert_eq!(payload, "native-audio");
    assert_eq!(upstream_request_id, "openai-upstream-audio-speech-123");

    let expected_tokens = 6;
    let token = harness.wait_for_token_used_quota(expected_tokens).await;
    assert_eq!(token.used_quota, expected_tokens);

    let log = harness.wait_for_log_by_request_id(&request_id).await;
    assert_eq!(log.endpoint, "audio/speech");
    assert_eq!(log.request_format, "openai/audio_speech");
    assert_eq!(log.requested_model, harness.model_name);
    assert_eq!(log.upstream_model, harness.model_name);
    assert_eq!(log.model_name, harness.model_name);
    assert_eq!(log.total_tokens, expected_tokens as i32);
    assert_eq!(log.quota, expected_tokens);
    assert_eq!(log.status_code, 200);
    assert_eq!(log.upstream_request_id, "openai-upstream-audio-speech-123");
    assert_eq!(log.status, LogStatus::Success);
    assert!(!log.is_stream);

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn anthropic_audio_transcriptions_route_returns_unsupported_endpoint() {
    let harness =
        TestHarness::anthropic_audio_affinity_fixture("http://127.0.0.1:9", "http://127.0.0.1:10")
            .await;
    let request_id = format!(
        "anthropic-audio-transcriptions-unsupported-{}",
        harness.model_name
    );

    let response = harness
        .multipart_request(MultipartRequestSpec {
            uri: "/v1/audio/transcriptions",
            request_id: &request_id,
            text_fields: &[("model", &harness.model_name)],
            file_field_name: "file",
            file_name: "voice-primary.wav",
            file_content_type: "audio/wav",
            file_bytes: b"voice bytes",
        })
        .await;
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    let payload = crate::router::tests::support::response_json(response).await;

    assert_eq!(payload["error"]["type"], "upstream_error");
    assert_eq!(payload["error"]["code"], "unsupported_endpoint");

    let log = harness.wait_for_log_by_request_id(&request_id).await;
    assert_eq!(log.endpoint, "audio/transcriptions");
    assert_eq!(log.request_format, "openai/audio_transcriptions");
    assert_eq!(log.requested_model, harness.model_name);
    assert_eq!(log.status, LogStatus::Failed);
    assert_eq!(log.status_code, 0);

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn openai_audio_transcriptions_route_native_success() {
    let primary = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: "/v1/audio/transcriptions".into(),
        expected_header_name: "authorization".into(),
        expected_header_value: "Bearer sk-primary".into(),
        expected_body_substring: Some("voice-primary.wav".into()),
        additional_expected_headers: vec![],
        additional_expected_body_substrings: vec![
            "name=\"model\"\r\n\r\n".into(),
            "name=\"file\"; filename=\"voice-primary.wav\"".into(),
        ],
        response_status: StatusCode::OK,
        response_content_type: "application/json".into(),
        response_headers: vec![(
            "x-request-id".into(),
            "openai-upstream-audio-transcriptions-123".into(),
        )],
        response_body: serde_json::json!({
            "text": "native transcript",
            "segments": [],
        })
        .to_string(),
    })
    .await;
    let harness =
        TestHarness::audio_affinity_fixture(&primary.base_url, "http://127.0.0.1:9").await;
    let request_id = format!("openai-audio-transcriptions-native-{}", harness.model_name);

    let response = harness
        .multipart_request(MultipartRequestSpec {
            uri: "/v1/audio/transcriptions",
            request_id: &request_id,
            text_fields: &[("model", &harness.model_name)],
            file_field_name: "file",
            file_name: "voice-primary.wav",
            file_content_type: "audio/wav",
            file_bytes: b"voice bytes",
        })
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let upstream_request_id = response
        .headers()
        .get("x-upstream-request-id")
        .and_then(|value| value.to_str().ok())
        .expect("audio transcription upstream request id")
        .to_string();
    let payload = crate::router::tests::support::response_json(response).await;

    assert_eq!(payload["text"], "native transcript");
    assert_eq!(
        upstream_request_id,
        "openai-upstream-audio-transcriptions-123"
    );

    let expected_tokens = 1;
    let token = harness.wait_for_token_used_quota(expected_tokens).await;
    assert_eq!(token.used_quota, expected_tokens);

    let log = harness.wait_for_log_by_request_id(&request_id).await;
    assert_eq!(log.endpoint, "audio/transcriptions");
    assert_eq!(log.request_format, "openai/audio_transcriptions");
    assert_eq!(log.requested_model, harness.model_name);
    assert_eq!(log.upstream_model, harness.model_name);
    assert_eq!(log.model_name, harness.model_name);
    assert_eq!(log.total_tokens, expected_tokens as i32);
    assert_eq!(log.quota, expected_tokens);
    assert_eq!(log.status_code, 200);
    assert_eq!(
        log.upstream_request_id,
        "openai-upstream-audio-transcriptions-123"
    );
    assert_eq!(log.status, LogStatus::Success);
    assert!(!log.is_stream);

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn anthropic_audio_translations_route_returns_unsupported_endpoint() {
    let harness =
        TestHarness::anthropic_audio_affinity_fixture("http://127.0.0.1:9", "http://127.0.0.1:10")
            .await;
    let request_id = format!(
        "anthropic-audio-translations-unsupported-{}",
        harness.model_name
    );

    let response = harness
        .multipart_request(MultipartRequestSpec {
            uri: "/v1/audio/translations",
            request_id: &request_id,
            text_fields: &[("model", &harness.model_name)],
            file_field_name: "file",
            file_name: "voice-primary.wav",
            file_content_type: "audio/wav",
            file_bytes: b"voice bytes",
        })
        .await;
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    let payload = crate::router::tests::support::response_json(response).await;

    assert_eq!(payload["error"]["type"], "upstream_error");
    assert_eq!(payload["error"]["code"], "unsupported_endpoint");

    let log = harness.wait_for_log_by_request_id(&request_id).await;
    assert_eq!(log.endpoint, "audio/translations");
    assert_eq!(log.request_format, "openai/audio_translations");
    assert_eq!(log.requested_model, harness.model_name);
    assert_eq!(log.status, LogStatus::Failed);
    assert_eq!(log.status_code, 0);

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn openai_audio_translations_route_native_success() {
    let primary = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: "/v1/audio/translations".into(),
        expected_header_name: "authorization".into(),
        expected_header_value: "Bearer sk-primary".into(),
        expected_body_substring: Some("voice-primary.wav".into()),
        additional_expected_headers: vec![],
        additional_expected_body_substrings: vec![
            "name=\"model\"\r\n\r\n".into(),
            "name=\"file\"; filename=\"voice-primary.wav\"".into(),
        ],
        response_status: StatusCode::OK,
        response_content_type: "application/json".into(),
        response_headers: vec![(
            "x-request-id".into(),
            "openai-upstream-audio-translations-123".into(),
        )],
        response_body: serde_json::json!({
            "text": "native translation",
        })
        .to_string(),
    })
    .await;
    let harness =
        TestHarness::audio_affinity_fixture(&primary.base_url, "http://127.0.0.1:9").await;
    let request_id = format!("openai-audio-translations-native-{}", harness.model_name);

    let response = harness
        .multipart_request(MultipartRequestSpec {
            uri: "/v1/audio/translations",
            request_id: &request_id,
            text_fields: &[("model", &harness.model_name)],
            file_field_name: "file",
            file_name: "voice-primary.wav",
            file_content_type: "audio/wav",
            file_bytes: b"voice bytes",
        })
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let upstream_request_id = response
        .headers()
        .get("x-upstream-request-id")
        .and_then(|value| value.to_str().ok())
        .expect("audio translation upstream request id")
        .to_string();
    let payload = crate::router::tests::support::response_json(response).await;

    assert_eq!(payload["text"], "native translation");
    assert_eq!(
        upstream_request_id,
        "openai-upstream-audio-translations-123"
    );

    let expected_tokens = 1;
    let token = harness.wait_for_token_used_quota(expected_tokens).await;
    assert_eq!(token.used_quota, expected_tokens);

    let log = harness.wait_for_log_by_request_id(&request_id).await;
    assert_eq!(log.endpoint, "audio/translations");
    assert_eq!(log.request_format, "openai/audio_translations");
    assert_eq!(log.requested_model, harness.model_name);
    assert_eq!(log.upstream_model, harness.model_name);
    assert_eq!(log.model_name, harness.model_name);
    assert_eq!(log.total_tokens, expected_tokens as i32);
    assert_eq!(log.quota, expected_tokens);
    assert_eq!(log.status_code, 200);
    assert_eq!(
        log.upstream_request_id,
        "openai-upstream-audio-translations-123"
    );
    assert_eq!(log.status, LogStatus::Success);
    assert!(!log.is_stream);

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn openai_completions_route_bridges_stream_and_accounts_usage() {
    let primary = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: "/v1/chat/completions".into(),
        expected_header_name: "authorization".into(),
        expected_header_value: "Bearer sk-primary".into(),
        expected_body_substring: Some("\"messages\"".into()),
        additional_expected_headers: vec![],
        additional_expected_body_substrings: vec!["\"Hello\"".into()],
        response_status: StatusCode::OK,
        response_content_type: "text/event-stream".into(),
        response_headers: vec![("x-request-id".into(), "openai-upstream-completions-123".into())],
        response_body: concat!(
            "data: {\"id\":\"chatcmpl_completion_stream_123\",\"object\":\"chat.completion.chunk\",\"created\":1774427062,\"model\":\"test-model\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"Hello\"},\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"chatcmpl_completion_stream_123\",\"object\":\"chat.completion.chunk\",\"created\":1774427062,\"model\":\"test-model\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\" world\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":12,\"completion_tokens\":7,\"total_tokens\":19}}\n\n",
            "data: [DONE]\n\n"
        )
        .into(),
    })
    .await;
    let harness =
        TestHarness::model_passthrough_affinity_fixture(&primary.base_url, "http://127.0.0.1:9")
            .await;
    let request_id = format!("openai-completions-stream-{}", harness.model_name);

    let response = harness
        .json_request(
            Method::POST,
            "/v1/completions",
            &request_id,
            serde_json::json!({
                "model": harness.model_name,
                "prompt": "Hello",
                "stream": true
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let upstream_request_id = response
        .headers()
        .get("x-upstream-request-id")
        .and_then(|value| value.to_str().ok())
        .expect("openai completions upstream request id")
        .to_string();
    let body = crate::router::tests::support::response_text(response).await;

    assert!(body.contains("\"object\":\"text_completion\""));
    assert!(body.contains("\"text\":\"Hello\""));
    assert!(body.contains("\"text\":\" world\""));
    assert!(body.contains("\"total_tokens\":19"));
    assert_eq!(upstream_request_id, "openai-upstream-completions-123");

    let token = harness.wait_for_token_used_quota(19).await;
    assert_eq!(token.used_quota, 19);

    let log = harness.wait_for_log_by_request_id(&request_id).await;
    assert_eq!(log.endpoint, "completions");
    assert_eq!(log.request_format, "openai/completions");
    assert_eq!(log.requested_model, harness.model_name);
    assert_eq!(log.upstream_model, "test-model");
    assert_eq!(log.model_name, harness.model_name);
    assert_eq!(log.total_tokens, 19);
    assert_eq!(log.quota, 19);
    assert_eq!(log.status_code, 200);
    assert_eq!(log.upstream_request_id, "openai-upstream-completions-123");
    assert_eq!(log.status, LogStatus::Success);
    assert!(log.is_stream);

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn anthropic_completions_route_bridges_non_stream_response() {
    let actual_model = "claude-sonnet-4-20250514";
    let primary = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: "/v1/messages".into(),
        expected_header_name: "x-api-key".into(),
        expected_header_value: "sk-primary".into(),
        expected_body_substring: Some(format!("\"model\":\"{actual_model}\"")),
        additional_expected_headers: vec![("anthropic-version".into(), "2023-06-01".into())],
        additional_expected_body_substrings: vec!["\"messages\"".into()],
        response_status: StatusCode::OK,
        response_content_type: "application/json".into(),
        response_headers: vec![(
            "request-id".into(),
            "anthropic-upstream-completions-123".into(),
        )],
        response_body: serde_json::json!({
            "id": "msg_completion_123",
            "model": actual_model,
            "content": [{"type": "text", "text": "Hello from Claude completions bridge"}],
            "stop_reason": "end_turn",
            "usage": {
                "input_tokens": 12,
                "output_tokens": 7
            }
        })
        .to_string(),
    })
    .await;
    let harness = TestHarness::anthropic_completions_affinity_fixture(
        &primary.base_url,
        "http://127.0.0.1:9",
    )
    .await;
    let request_id = format!("anthropic-completions-bridge-{}", harness.model_name);

    let response = harness
        .json_request(
            Method::POST,
            "/v1/completions",
            &request_id,
            serde_json::json!({
                "model": harness.model_name,
                "prompt": "Hello",
                "stream": false
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let upstream_request_id = response
        .headers()
        .get("x-upstream-request-id")
        .and_then(|value| value.to_str().ok())
        .expect("anthropic completions upstream request id")
        .to_string();
    let payload = crate::router::tests::support::response_json(response).await;

    assert_eq!(payload["id"], "msg_completion_123");
    assert_eq!(payload["object"], "text_completion");
    assert_eq!(payload["model"], actual_model);
    assert_eq!(
        payload["choices"][0]["text"],
        "Hello from Claude completions bridge"
    );
    assert_eq!(payload["usage"]["total_tokens"], 19);
    assert_eq!(upstream_request_id, "anthropic-upstream-completions-123");

    let token = harness.wait_for_token_used_quota(19).await;
    assert_eq!(token.used_quota, 19);

    let log = harness.wait_for_log_by_request_id(&request_id).await;
    assert_eq!(log.endpoint, "completions");
    assert_eq!(log.request_format, "openai/completions");
    assert_eq!(log.requested_model, harness.model_name);
    assert_eq!(log.upstream_model, actual_model);
    assert_eq!(log.model_name, harness.model_name);
    assert_eq!(log.total_tokens, 19);
    assert_eq!(log.quota, 19);
    assert_eq!(log.status_code, 200);
    assert_eq!(
        log.upstream_request_id,
        "anthropic-upstream-completions-123"
    );
    assert_eq!(log.status, LogStatus::Success);
    assert!(!log.is_stream);

    harness.cleanup().await;
}

#[tokio::test]
async fn anthropic_chat_non_stream_mock_upstream_success() {
    let req = sample_mock_chat_request(false);
    let actual_model = "claude-3-5-sonnet-20241022";
    let (_server, response) = send_mock_chat_request(
        3,
        "sk-ant-test",
        &req,
        actual_model,
        MockUpstreamSpec {
            expected_path_and_query: "/v1/messages".into(),
            expected_header_name: "x-api-key".into(),
            expected_header_value: "sk-ant-test".into(),
            expected_body_substring: Some(format!("\"model\":\"{actual_model}\"")),
            additional_expected_headers: vec![],
            additional_expected_body_substrings: vec![],
            response_status: StatusCode::OK,
            response_content_type: "application/json".into(),
            response_headers: vec![],
            response_body: serde_json::json!({
                "id": "msg_123",
                "model": actual_model,
                "content": [{"type": "text", "text": "Hello from Claude"}],
                "stop_reason": "end_turn",
                "usage": {
                    "input_tokens": 12,
                    "output_tokens": 7
                }
            })
            .to_string(),
        },
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let parsed = get_adapter(3)
        .parse_response(response.bytes().await.expect("body"), actual_model)
        .expect("parse anthropic response");
    assert_eq!(parsed.model, actual_model);
    assert_eq!(
        parsed.choices[0].message.content,
        serde_json::json!("Hello from Claude")
    );
    assert_eq!(parsed.usage.total_tokens, 19);
}

#[tokio::test]
async fn anthropic_chat_non_stream_mock_upstream_preserves_thinking_extra_body() {
    let req: ChatCompletionRequest = serde_json::from_value(serde_json::json!({
        "model": "gpt-5.4 xhigh",
        "messages": [{"role": "user", "content": "Hello"}],
        "thinking": {
            "type": "enabled",
            "budget_tokens": 2048
        }
    }))
    .expect("anthropic thinking request");
    let actual_model = "claude-sonnet-4-20250514";
    let (_server, response) = send_mock_chat_request(
        3,
        "sk-ant-test",
        &req,
        actual_model,
        MockUpstreamSpec {
            expected_path_and_query: "/v1/messages".into(),
            expected_header_name: "x-api-key".into(),
            expected_header_value: "sk-ant-test".into(),
            expected_body_substring: Some(format!("\"model\":\"{actual_model}\"")),
            additional_expected_headers: vec![],
            additional_expected_body_substrings: vec![
                "\"thinking\"".into(),
                "\"budget_tokens\":2048".into(),
            ],
            response_status: StatusCode::OK,
            response_content_type: "application/json".into(),
            response_headers: vec![],
            response_body: serde_json::json!({
                "id": "msg_123",
                "model": actual_model,
                "content": [{"type": "text", "text": "Hello from Claude"}],
                "stop_reason": "end_turn",
                "usage": {
                    "input_tokens": 12,
                    "output_tokens": 7
                }
            })
            .to_string(),
        },
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn anthropic_chat_stream_mock_upstream_success() {
    let req = sample_mock_chat_request(true);
    let actual_model = "claude-3-5-sonnet-20241022";
    let (_server, response) = send_mock_chat_request(
        3,
        "sk-ant-test",
        &req,
        actual_model,
        MockUpstreamSpec {
            expected_path_and_query: "/v1/messages".into(),
            expected_header_name: "x-api-key".into(),
            expected_header_value: "sk-ant-test".into(),
            expected_body_substring: Some(format!("\"model\":\"{actual_model}\"")),
            additional_expected_headers: vec![],
            additional_expected_body_substrings: vec![],
            response_status: StatusCode::OK,
            response_content_type: "text/event-stream".into(),
            response_headers: vec![],
            response_body: concat!(
                "event: message_start\n",
                "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_123\",\"model\":\"claude-3-5-sonnet-20241022\",\"usage\":{\"input_tokens\":12,\"output_tokens\":0}}}\n\n",
                "event: content_block_delta\n",
                "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}\n\n",
                "event: message_delta\n",
                "data: {\"type\":\"message_delta\",\"usage\":{\"input_tokens\":12,\"output_tokens\":7},\"delta\":{\"stop_reason\":\"end_turn\"}}\n\n",
                "event: message_stop\n",
                "data: {\"type\":\"message_stop\"}\n\n"
            )
            .into(),
        },
    )
    .await;

    let chunks: Vec<_> = get_adapter(3)
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
        Some(19)
    );
}

#[tokio::test]
async fn anthropic_chat_stream_mock_upstream_emits_reasoning_content() {
    let req = sample_mock_chat_request(true);
    let actual_model = "claude-sonnet-4-20250514";
    let (_server, response) = send_mock_chat_request(
        3,
        "sk-ant-test",
        &req,
        actual_model,
        MockUpstreamSpec {
            expected_path_and_query: "/v1/messages".into(),
            expected_header_name: "x-api-key".into(),
            expected_header_value: "sk-ant-test".into(),
            expected_body_substring: Some(format!("\"model\":\"{actual_model}\"")),
            additional_expected_headers: vec![],
            additional_expected_body_substrings: vec!["\"stream\":true".into()],
            response_status: StatusCode::OK,
            response_content_type: "text/event-stream".into(),
            response_headers: vec![],
            response_body: concat!(
                "event: message_start\n",
                "data: {\"message\":{\"id\":\"msg_think\",\"model\":\"claude-sonnet-4-20250514\",\"usage\":{\"input_tokens\":10,\"output_tokens\":0}}}\n\n",
                "event: content_block_delta\n",
                "data: {\"index\":0,\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"Let me think this through.\"}}\n\n",
                "event: message_delta\n",
                "data: {\"usage\":{\"input_tokens\":10,\"output_tokens\":4},\"delta\":{\"stop_reason\":\"end_turn\"}}\n\n"
            )
            .into(),
        },
    )
    .await;

    let chunks: Vec<_> = get_adapter(3)
        .parse_stream(response, actual_model)
        .expect("parse stream")
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .filter_map(Result::ok)
        .collect();

    assert!(chunks.iter().any(|chunk| {
        chunk.choices[0].delta.reasoning_content.as_deref() == Some("Let me think this through.")
    }));
    let final_chunk = chunks
        .iter()
        .find(|chunk| chunk.usage.is_some())
        .expect("expected usage chunk");
    assert_eq!(
        final_chunk.usage.as_ref().map(|usage| usage.total_tokens),
        Some(14)
    );
}

#[tokio::test]
async fn anthropic_chat_stream_mock_upstream_preserves_version_and_response_request_id() {
    let req = sample_mock_chat_request(true);
    let actual_model = "claude-3-5-sonnet-20241022";
    let (_server, response) = send_mock_chat_request(
        3,
        "sk-ant-test",
        &req,
        actual_model,
        MockUpstreamSpec {
            expected_path_and_query: "/v1/messages".into(),
            expected_header_name: "x-api-key".into(),
            expected_header_value: "sk-ant-test".into(),
            expected_body_substring: Some(format!("\"model\":\"{actual_model}\"")),
            additional_expected_headers: vec![("anthropic-version".into(), "2023-06-01".into())],
            additional_expected_body_substrings: vec!["\"stream\":true".into()],
            response_headers: vec![("anthropic-request-id".into(), "anth_req_123".into())],
            response_status: StatusCode::OK,
            response_content_type: "text/event-stream".into(),
            response_body: concat!(
                "event: message_start\n",
                "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_123\",\"model\":\"claude-3-5-sonnet-20241022\",\"usage\":{\"input_tokens\":12,\"output_tokens\":0}}}\n\n",
                "event: message_delta\n",
                "data: {\"type\":\"message_delta\",\"usage\":{\"input_tokens\":12,\"output_tokens\":7},\"delta\":{\"stop_reason\":\"end_turn\"}}\n\n"
            )
            .into(),
        },
    )
    .await;

    assert_eq!(
        extract_upstream_request_id(response.headers()),
        "anth_req_123"
    );
}

#[tokio::test]
async fn anthropic_chat_mock_upstream_provider_failure() {
    let req = sample_mock_chat_request(false);
    let actual_model = "claude-3-5-sonnet-20241022";
    let (_server, response) = send_mock_chat_request(
        3,
        "sk-ant-test",
        &req,
        actual_model,
        MockUpstreamSpec {
            expected_path_and_query: "/v1/messages".into(),
            expected_header_name: "x-api-key".into(),
            expected_header_value: "sk-ant-test".into(),
            expected_body_substring: Some(format!("\"model\":\"{actual_model}\"")),
            additional_expected_headers: vec![],
            additional_expected_body_substrings: vec![],
            response_status: StatusCode::TOO_MANY_REQUESTS,
            response_content_type: "application/json".into(),
            response_headers: vec![],
            response_body:
                r#"{"type":"error","error":{"type":"rate_limit_error","message":"slow down"}}"#
                    .to_string(),
        },
    )
    .await;

    let status = response.status();
    let headers = response.headers().clone();
    let body = response.bytes().await.expect("body");
    let failure = classify_upstream_provider_failure(3, status, &headers, &body);
    assert_eq!(failure.scope, UpstreamFailureScope::Account);
    assert_eq!(failure.error.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(failure.error.error.error.r#type, "rate_limit_error");
    assert_eq!(failure.error.error.error.message, "slow down");
}

#[tokio::test]
async fn anthropic_chat_stream_mock_upstream_provider_failure_event() {
    let req = sample_mock_chat_request(true);
    let actual_model = "claude-3-5-sonnet-20241022";
    let (_server, response) = send_mock_chat_request(
        3,
        "sk-ant-test",
        &req,
        actual_model,
        MockUpstreamSpec {
            expected_path_and_query: "/v1/messages".into(),
            expected_header_name: "x-api-key".into(),
            expected_header_value: "sk-ant-test".into(),
            expected_body_substring: Some(format!("\"model\":\"{actual_model}\"")),
            additional_expected_headers: vec![],
            additional_expected_body_substrings: vec![],
            response_status: StatusCode::OK,
            response_content_type: "text/event-stream".into(),
            response_headers: vec![],
            response_body: concat!(
                "event: error\n",
                "data: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"upstream overloaded\"}}\n\n"
            )
            .into(),
        },
    )
    .await;

    let results = get_adapter(3)
        .parse_stream(response, actual_model)
        .expect("parse stream")
        .collect::<Vec<_>>()
        .await;

    let error = results
        .into_iter()
        .find_map(Result::err)
        .expect("expected anthropic stream error");
    let stream_error = error
        .downcast_ref::<summer_ai_core::provider::ProviderStreamError>()
        .expect("expected provider stream error");
    assert_eq!(stream_error.info.kind, ProviderErrorKind::Server);
    assert_eq!(stream_error.info.code, "overloaded_error");
    assert_eq!(stream_error.info.message, "upstream overloaded");
}

#[tokio::test]
async fn anthropic_chat_stream_mock_upstream_invalid_request_failure_event() {
    let req = sample_mock_chat_request(true);
    let actual_model = "claude-3-5-sonnet-20241022";
    let (_server, response) = send_mock_chat_request(
        3,
        "sk-ant-test",
        &req,
        actual_model,
        MockUpstreamSpec {
            expected_path_and_query: "/v1/messages".into(),
            expected_header_name: "x-api-key".into(),
            expected_header_value: "sk-ant-test".into(),
            expected_body_substring: Some(format!("\"model\":\"{actual_model}\"")),
            additional_expected_headers: vec![("anthropic-version".into(), "2023-06-01".into())],
            additional_expected_body_substrings: vec!["\"stream\":true".into()],
            response_status: StatusCode::OK,
            response_content_type: "text/event-stream".into(),
            response_headers: vec![],
            response_body: concat!(
                "event: error\n",
                "data: {\"type\":\"error\",\"error\":{\"type\":\"invalid_request_error\",\"message\":\"bad claude payload\"}}\n\n"
            )
            .into(),
        },
    )
    .await;

    let results = get_adapter(3)
        .parse_stream(response, actual_model)
        .expect("parse stream")
        .collect::<Vec<_>>()
        .await;

    let error = results
        .into_iter()
        .find_map(Result::err)
        .expect("expected anthropic stream error");
    let stream_error = error
        .downcast_ref::<summer_ai_core::provider::ProviderStreamError>()
        .expect("expected provider stream error");
    assert_eq!(stream_error.info.kind, ProviderErrorKind::InvalidRequest);
    assert_eq!(stream_error.info.code, "invalid_request_error");
    assert_eq!(stream_error.info.message, "bad claude payload");
}
