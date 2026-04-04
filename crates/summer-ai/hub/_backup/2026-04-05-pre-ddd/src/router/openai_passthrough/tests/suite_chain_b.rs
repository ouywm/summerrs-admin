use super::*;

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn threads_runs_create_without_request_model_settles_usage_from_response_model() {
    let primary = MockUpstreamServer::spawn(vec![MockRoute::json(
        Method::POST,
        "/v1/threads/runs",
        Some("Bearer sk-primary"),
        None,
        StatusCode::OK,
        serde_json::json!({
            "id": "run_usage_primary",
            "object": "thread.run",
            "thread_id": "thread_usage_primary",
            "assistant_id": "asst_usage_primary",
            "model": "__MODEL__",
            "status": "completed",
            "route": "primary",
            "usage": {
                "prompt_tokens": 4,
                "completion_tokens": 3,
                "total_tokens": 7
            }
        }),
    )])
    .await;
    let harness =
        TestHarness::assistants_threads_affinity_fixture(&primary.base_url, "http://127.0.0.1:9")
            .await;
    primary.replace_placeholder("__MODEL__", &harness.model_name);
    let request_id = format!("threads-runs-usage-{}", harness.model_name);

    let response = harness
        .json_request(
            Method::POST,
            "/v1/threads/runs",
            &request_id,
            serde_json::json!({
                "assistant_id": "asst_usage_primary",
                "thread": {
                    "messages": [{
                        "role": "user",
                        "content": "hello"
                    }]
                }
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(payload["id"], "run_usage_primary");
    assert_eq!(payload["route"], "primary");

    let token = harness.wait_for_token_used_quota(7).await;
    assert_eq!(token.used_quota, 7);

    let log = harness.wait_for_log_by_request_id(&request_id).await;
    assert_eq!(log.endpoint, "threads/runs");
    assert_eq!(log.request_format, "openai/threads_runs");
    assert_eq!(log.requested_model, harness.model_name);
    assert_eq!(log.upstream_model, harness.model_name);
    assert_eq!(log.model_name, harness.model_name);
    assert_eq!(log.prompt_tokens, 4);
    assert_eq!(log.completion_tokens, 3);
    assert_eq!(log.total_tokens, 7);
    assert_eq!(log.quota, 7);
    assert_eq!(log.status_code, 200);
    assert_eq!(log.status, LogStatus::Success);
    assert!(!log.is_stream);

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn threads_runs_stream_without_request_model_settles_usage_from_stream_tail() {
    let primary = MockUpstreamServer::spawn(vec![MockRoute::raw(
            Method::POST,
            "/v1/threads/runs",
            Some("Bearer sk-primary"),
            Some("\"stream\":true"),
            StatusCode::OK,
            "text/event-stream",
            concat!(
                "data: {\"id\":\"run_stream_primary\",\"object\":\"thread.run\",\"thread_id\":\"thread_stream_primary\",\"assistant_id\":\"asst_stream_primary\",\"model\":\"__MODEL__\",\"status\":\"in_progress\"}\n\n",
                "data: {\"id\":\"run_stream_primary\",\"object\":\"thread.run\",\"thread_id\":\"thread_stream_primary\",\"assistant_id\":\"asst_stream_primary\",\"model\":\"__MODEL__\",\"status\":\"completed\",\"usage\":{\"prompt_tokens\":4,\"completion_tokens\":3,\"total_tokens\":7}}\n\n",
                "data: [DONE]\n\n"
            ),
        )
        .with_response_headers(vec![("x-request-id", "run-stream-upstream-123")])])
        .await;
    let harness =
        TestHarness::assistants_threads_affinity_fixture(&primary.base_url, "http://127.0.0.1:9")
            .await;
    primary.replace_placeholder("__MODEL__", &harness.model_name);
    let request_id = format!("threads-runs-stream-usage-{}", harness.model_name);

    let response = harness
        .json_request(
            Method::POST,
            "/v1/threads/runs",
            &request_id,
            serde_json::json!({
                "assistant_id": "asst_stream_primary",
                "stream": true,
                "thread": {
                    "messages": [{
                        "role": "user",
                        "content": "hello"
                    }]
                }
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let upstream_request_id = response
        .headers()
        .get("x-upstream-request-id")
        .and_then(|value| value.to_str().ok())
        .expect("thread run stream upstream request id")
        .to_string();
    let body = response_text(response).await;
    assert!(body.contains("run_stream_primary"));
    assert!(body.contains("\"total_tokens\":7"));
    assert_eq!(upstream_request_id, "run-stream-upstream-123");

    let token = harness.wait_for_token_used_quota(7).await;
    assert_eq!(token.used_quota, 7);

    let log = harness.wait_for_log_by_request_id(&request_id).await;
    assert_eq!(log.endpoint, "threads/runs");
    assert_eq!(log.request_format, "openai/threads_runs");
    assert_eq!(log.requested_model, harness.model_name);
    assert_eq!(log.upstream_model, harness.model_name);
    assert_eq!(log.model_name, harness.model_name);
    assert_eq!(log.upstream_request_id, "run-stream-upstream-123");
    assert_eq!(log.prompt_tokens, 4);
    assert_eq!(log.completion_tokens, 3);
    assert_eq!(log.total_tokens, 7);
    assert_eq!(log.quota, 7);
    assert_eq!(log.status_code, 200);
    assert_eq!(log.status, LogStatus::Success);
    assert!(log.is_stream);

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn threads_runs_stream_falls_back_after_primary_overload() {
    let primary = MockUpstreamServer::spawn(vec![MockRoute::json(
        Method::POST,
        "/v1/threads/runs",
        Some("Bearer sk-primary"),
        Some("\"stream\":true"),
        StatusCode::SERVICE_UNAVAILABLE,
        serde_json::json!({
            "error": {
                "message": "primary thread run upstream overloaded",
                "type": "server_error"
            }
        }),
    )])
    .await;
    let fallback = MockUpstreamServer::spawn(vec![MockRoute::raw(
            Method::POST,
            "/v1/threads/runs",
            Some("Bearer sk-fallback"),
            Some("\"stream\":true"),
            StatusCode::OK,
            "text/event-stream",
            concat!(
                "data: {\"id\":\"run_stream_fallback\",\"object\":\"thread.run\",\"thread_id\":\"thread_stream_fallback\",\"assistant_id\":\"asst_stream_fallback\",\"model\":\"__MODEL__\",\"status\":\"in_progress\",\"route\":\"fallback\"}\n\n",
                "data: {\"id\":\"run_stream_fallback\",\"object\":\"thread.run\",\"thread_id\":\"thread_stream_fallback\",\"assistant_id\":\"asst_stream_fallback\",\"model\":\"__MODEL__\",\"status\":\"completed\",\"route\":\"fallback\",\"usage\":{\"prompt_tokens\":4,\"completion_tokens\":3,\"total_tokens\":7}}\n\n",
                "data: [DONE]\n\n"
            ),
        )
        .with_response_headers(vec![("x-request-id", "run-stream-fallback-upstream-123")])])
        .await;
    let harness =
        TestHarness::assistants_threads_affinity_fixture(&primary.base_url, &fallback.base_url)
            .await;
    fallback.replace_placeholder("__MODEL__", &harness.model_name);
    let request_id = format!("threads-runs-stream-fallback-{}", harness.model_name);

    let response = harness
        .json_request(
            Method::POST,
            "/v1/threads/runs",
            &request_id,
            serde_json::json!({
                "assistant_id": "asst_stream_fallback",
                "stream": true,
                "thread": {
                    "messages": [{
                        "role": "user",
                        "content": "hello"
                    }]
                }
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let upstream_request_id = response
        .headers()
        .get("x-upstream-request-id")
        .and_then(|value| value.to_str().ok())
        .expect("thread run fallback stream upstream request id")
        .to_string();
    let body = response_text(response).await;
    assert!(body.contains("run_stream_fallback"));
    assert!(body.contains("\"route\":\"fallback\""));
    assert!(body.contains("\"total_tokens\":7"));
    assert_eq!(upstream_request_id, "run-stream-fallback-upstream-123");

    let token = harness.wait_for_token_used_quota(7).await;
    assert_eq!(token.used_quota, 7);

    let log = harness.wait_for_log_by_request_id(&request_id).await;
    assert_eq!(log.endpoint, "threads/runs");
    assert_eq!(log.request_format, "openai/threads_runs");
    assert_eq!(log.requested_model, harness.model_name);
    assert_eq!(log.upstream_model, harness.model_name);
    assert_eq!(log.model_name, harness.model_name);
    assert_eq!(log.upstream_request_id, "run-stream-fallback-upstream-123");
    assert_eq!(log.total_tokens, 7);
    assert_eq!(log.quota, 7);
    assert_eq!(log.status, LogStatus::Success);
    assert!(log.is_stream);

    let primary_account = harness.wait_for_primary_account_overloaded().await;
    assert_eq!(primary_account.failure_streak, 1);
    assert!(primary_account.overload_until.is_some());
    assert!(primary_account.rate_limited_until.is_none());

    let primary_channel = harness.primary_channel_model().await;
    assert_eq!(primary_channel.failure_streak, 1);
    assert_eq!(primary_channel.last_health_status, 3);

    assert_eq!(primary.hit_count("/v1/threads/runs"), 1);
    assert_eq!(fallback.hit_count("/v1/threads/runs"), 1);

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn threads_runs_stream_falls_back_after_primary_rate_limit() {
    let primary = MockUpstreamServer::spawn(vec![MockRoute::json(
        Method::POST,
        "/v1/threads/runs",
        Some("Bearer sk-primary"),
        Some("\"stream\":true"),
        StatusCode::TOO_MANY_REQUESTS,
        serde_json::json!({
            "error": {
                "message": "primary thread run rate limited",
                "type": "rate_limit_error"
            }
        }),
    )])
    .await;
    let fallback = MockUpstreamServer::spawn(vec![MockRoute::raw(
            Method::POST,
            "/v1/threads/runs",
            Some("Bearer sk-fallback"),
            Some("\"stream\":true"),
            StatusCode::OK,
            "text/event-stream",
            concat!(
                "data: {\"id\":\"run_stream_rate_limit_fallback\",\"object\":\"thread.run\",\"thread_id\":\"thread_stream_rate_limit_fallback\",\"assistant_id\":\"asst_stream_rate_limit_fallback\",\"model\":\"__MODEL__\",\"status\":\"in_progress\",\"route\":\"fallback\"}\n\n",
                "data: {\"id\":\"run_stream_rate_limit_fallback\",\"object\":\"thread.run\",\"thread_id\":\"thread_stream_rate_limit_fallback\",\"assistant_id\":\"asst_stream_rate_limit_fallback\",\"model\":\"__MODEL__\",\"status\":\"completed\",\"route\":\"fallback\",\"usage\":{\"prompt_tokens\":4,\"completion_tokens\":3,\"total_tokens\":7}}\n\n",
                "data: [DONE]\n\n"
            ),
        )
        .with_response_headers(vec![("x-request-id", "run-stream-rate-limit-upstream-123")])])
        .await;
    let harness =
        TestHarness::assistants_threads_affinity_fixture(&primary.base_url, &fallback.base_url)
            .await;
    fallback.replace_placeholder("__MODEL__", &harness.model_name);
    let request_id = format!("threads-runs-stream-rate-limit-{}", harness.model_name);

    let response = harness
        .json_request(
            Method::POST,
            "/v1/threads/runs",
            &request_id,
            serde_json::json!({
                "assistant_id": "asst_stream_rate_limit_fallback",
                "stream": true,
                "thread": {
                    "messages": [{
                        "role": "user",
                        "content": "hello"
                    }]
                }
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let upstream_request_id = response
        .headers()
        .get("x-upstream-request-id")
        .and_then(|value| value.to_str().ok())
        .expect("thread run rate limit fallback stream upstream request id")
        .to_string();
    let body = response_text(response).await;
    assert!(body.contains("run_stream_rate_limit_fallback"));
    assert!(body.contains("\"route\":\"fallback\""));
    assert!(body.contains("\"total_tokens\":7"));
    assert_eq!(upstream_request_id, "run-stream-rate-limit-upstream-123");

    let token = harness.wait_for_token_used_quota(7).await;
    assert_eq!(token.used_quota, 7);

    let log = harness.wait_for_log_by_request_id(&request_id).await;
    assert_eq!(log.endpoint, "threads/runs");
    assert_eq!(log.request_format, "openai/threads_runs");
    assert_eq!(log.requested_model, harness.model_name);
    assert_eq!(log.upstream_model, harness.model_name);
    assert_eq!(log.model_name, harness.model_name);
    assert_eq!(
        log.upstream_request_id,
        "run-stream-rate-limit-upstream-123"
    );
    assert_eq!(log.total_tokens, 7);
    assert_eq!(log.quota, 7);
    assert_eq!(log.status, LogStatus::Success);
    assert!(log.is_stream);

    let primary_account = harness.wait_for_primary_account_rate_limited().await;
    assert_eq!(primary_account.failure_streak, 1);
    assert!(primary_account.rate_limited_until.is_some());
    assert!(primary_account.overload_until.is_none());

    let primary_channel = harness.primary_channel_model().await;
    assert_eq!(primary_channel.failure_streak, 1);
    assert_eq!(primary_channel.last_health_status, 3);

    assert_eq!(primary.hit_count("/v1/threads/runs"), 1);
    assert_eq!(fallback.hit_count("/v1/threads/runs"), 1);

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn files_vector_store_chain_keeps_affinity_after_default_route_switch() {
    let primary = MockUpstreamServer::spawn(vec![
        MockRoute::json(
            Method::POST,
            "/v1/files",
            Some("Bearer sk-primary"),
            Some("hello-file.txt"),
            StatusCode::OK,
            serde_json::json!({
                "id": "file_chain_primary",
                "object": "file",
                "purpose": "assistants",
                "route": "primary"
            }),
        ),
        MockRoute::json(
            Method::GET,
            "/v1/files/file_chain_primary",
            Some("Bearer sk-primary"),
            None,
            StatusCode::OK,
            serde_json::json!({
                "id": "file_chain_primary",
                "object": "file",
                "route": "primary"
            }),
        ),
        MockRoute::raw(
            Method::GET,
            "/v1/files/file_chain_primary/content",
            Some("Bearer sk-primary"),
            None,
            StatusCode::OK,
            "text/plain",
            "primary-file-content",
        ),
        MockRoute::json(
            Method::POST,
            "/v1/vector_stores",
            Some("Bearer sk-primary"),
            Some("vs-chain-primary"),
            StatusCode::OK,
            serde_json::json!({
                "id": "vs_chain_primary",
                "object": "vector_store",
                "name": "vs-chain-primary",
                "route": "primary"
            }),
        ),
        MockRoute::json(
            Method::GET,
            "/v1/vector_stores/vs_chain_primary",
            Some("Bearer sk-primary"),
            None,
            StatusCode::OK,
            serde_json::json!({
                "id": "vs_chain_primary",
                "object": "vector_store",
                "route": "primary"
            }),
        ),
        MockRoute::json(
            Method::POST,
            "/v1/vector_stores/vs_chain_primary/files",
            Some("Bearer sk-primary"),
            Some("file_chain_primary"),
            StatusCode::OK,
            serde_json::json!({
                "id": "file_chain_primary",
                "object": "vector_store.file",
                "vector_store_id": "vs_chain_primary",
                "route": "primary"
            }),
        ),
        MockRoute::json(
            Method::GET,
            "/v1/vector_stores/vs_chain_primary/files/file_chain_primary",
            Some("Bearer sk-primary"),
            None,
            StatusCode::OK,
            serde_json::json!({
                "id": "file_chain_primary",
                "object": "vector_store.file",
                "vector_store_id": "vs_chain_primary",
                "route": "primary"
            }),
        ),
        MockRoute::json(
            Method::POST,
            "/v1/vector_stores/vs_chain_primary/search",
            Some("Bearer sk-primary"),
            Some("find file_chain_primary"),
            StatusCode::OK,
            serde_json::json!({
                "object": "list",
                "data": [{
                    "id": "chunk_primary_1",
                    "route": "primary"
                }]
            }),
        ),
    ])
    .await;
    let fallback = MockUpstreamServer::spawn(vec![
        MockRoute::json(
            Method::GET,
            "/v1/files",
            Some("Bearer sk-fallback"),
            None,
            StatusCode::OK,
            serde_json::json!({
                "object": "list",
                "data": [],
                "route": "fallback"
            }),
        ),
        MockRoute::json(
            Method::GET,
            "/v1/vector_stores",
            Some("Bearer sk-fallback"),
            None,
            StatusCode::OK,
            serde_json::json!({
                "object": "list",
                "data": [],
                "route": "fallback"
            }),
        ),
    ])
    .await;
    let harness =
        TestHarness::files_vector_stores_affinity_fixture(&primary.base_url, &fallback.base_url)
            .await;

    let create_file_payload = response_json(
        harness
            .multipart_request(MultipartRequestSpec {
                uri: "/v1/files",
                request_id: "files-chain-create-file",
                text_fields: &[("purpose", "assistants")],
                file_field_name: "file",
                file_name: "hello-file.txt",
                file_content_type: "text/plain",
                file_bytes: b"hello from file chain",
            })
            .await,
    )
    .await;
    assert_eq!(create_file_payload["id"], "file_chain_primary");

    let create_vector_store_payload = response_json(
        harness
            .json_request(
                Method::POST,
                "/v1/vector_stores",
                "files-chain-create-vs",
                serde_json::json!({
                    "name": "vs-chain-primary"
                }),
            )
            .await,
    )
    .await;
    assert_eq!(create_vector_store_payload["id"], "vs_chain_primary");

    harness
        .promote_fallback_for_scopes(&["files", "vector_stores"])
        .await;

    let list_files_payload = response_json(
        harness
            .empty_request(Method::GET, "/v1/files", "files-chain-list-files")
            .await,
    )
    .await;
    assert_eq!(list_files_payload["route"], "fallback");

    let list_vector_stores_payload = response_json(
        harness
            .empty_request(
                Method::GET,
                "/v1/vector_stores",
                "files-chain-list-vector-stores",
            )
            .await,
    )
    .await;
    assert_eq!(list_vector_stores_payload["route"], "fallback");

    let get_file_payload = response_json(
        harness
            .empty_request(
                Method::GET,
                "/v1/files/file_chain_primary",
                "files-chain-get-file",
            )
            .await,
    )
    .await;
    assert_eq!(get_file_payload["route"], "primary");

    let file_content = response_text(
        harness
            .empty_request(
                Method::GET,
                "/v1/files/file_chain_primary/content",
                "files-chain-get-content",
            )
            .await,
    )
    .await;
    assert_eq!(file_content, "primary-file-content");

    let get_vector_store_payload = response_json(
        harness
            .empty_request(
                Method::GET,
                "/v1/vector_stores/vs_chain_primary",
                "files-chain-get-vector-store",
            )
            .await,
    )
    .await;
    assert_eq!(get_vector_store_payload["route"], "primary");

    let create_vector_store_file_payload = response_json(
        harness
            .json_request(
                Method::POST,
                "/v1/vector_stores/vs_chain_primary/files",
                "files-chain-attach-file",
                serde_json::json!({
                    "file_id": "file_chain_primary"
                }),
            )
            .await,
    )
    .await;
    assert_eq!(create_vector_store_file_payload["route"], "primary");

    let get_vector_store_file_payload = response_json(
        harness
            .empty_request(
                Method::GET,
                "/v1/vector_stores/vs_chain_primary/files/file_chain_primary",
                "files-chain-get-vector-store-file",
            )
            .await,
    )
    .await;
    assert_eq!(get_vector_store_file_payload["route"], "primary");

    let search_payload = response_json(
        harness
            .json_request(
                Method::POST,
                "/v1/vector_stores/vs_chain_primary/search",
                "files-chain-search",
                serde_json::json!({
                    "query": "find file_chain_primary"
                }),
            )
            .await,
    )
    .await;
    assert_eq!(search_payload["data"][0]["route"], "primary");

    assert_eq!(primary.hit_count("/v1/files"), 1);
    assert_eq!(primary.hit_count("/v1/files/file_chain_primary"), 1);
    assert_eq!(primary.hit_count("/v1/files/file_chain_primary/content"), 1);
    assert_eq!(primary.hit_count("/v1/vector_stores"), 1);
    assert_eq!(primary.hit_count("/v1/vector_stores/vs_chain_primary"), 1);
    assert_eq!(
        primary.hit_count("/v1/vector_stores/vs_chain_primary/files"),
        1
    );
    assert_eq!(
        primary.hit_count("/v1/vector_stores/vs_chain_primary/files/file_chain_primary"),
        1
    );
    assert_eq!(
        primary.hit_count("/v1/vector_stores/vs_chain_primary/search"),
        1
    );
    assert_eq!(fallback.hit_count("/v1/files"), 1);
    assert_eq!(fallback.hit_count("/v1/vector_stores"), 1);

    harness.cleanup().await;
}
