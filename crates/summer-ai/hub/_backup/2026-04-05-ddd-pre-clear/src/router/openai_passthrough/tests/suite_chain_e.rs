use super::*;

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn file_and_vector_store_delete_clear_affinity_after_successful_delete() {
    let primary = MockUpstreamServer::spawn(vec![
        MockRoute::json(
            Method::POST,
            "/v1/files",
            Some("Bearer sk-primary"),
            Some("delete-me.txt"),
            StatusCode::OK,
            serde_json::json!({
                "id": "file_delete_primary",
                "object": "file",
                "purpose": "assistants",
                "route": "primary"
            }),
        ),
        MockRoute::json(
            Method::DELETE,
            "/v1/files/file_delete_primary",
            Some("Bearer sk-primary"),
            None,
            StatusCode::OK,
            serde_json::json!({
                "id": "file_delete_primary",
                "object": "file.deleted",
                "deleted": true,
                "route": "primary"
            }),
        ),
        MockRoute::json(
            Method::GET,
            "/v1/files/file_delete_primary",
            Some("Bearer sk-primary"),
            None,
            StatusCode::OK,
            serde_json::json!({
                "id": "file_delete_primary",
                "object": "file",
                "route": "stale-primary"
            }),
        ),
        MockRoute::json(
            Method::POST,
            "/v1/vector_stores",
            Some("Bearer sk-primary"),
            Some("vs-delete-primary"),
            StatusCode::OK,
            serde_json::json!({
                "id": "vs_delete_primary",
                "object": "vector_store",
                "route": "primary"
            }),
        ),
        MockRoute::json(
            Method::DELETE,
            "/v1/vector_stores/vs_delete_primary",
            Some("Bearer sk-primary"),
            None,
            StatusCode::OK,
            serde_json::json!({
                "id": "vs_delete_primary",
                "object": "vector_store.deleted",
                "deleted": true,
                "route": "primary"
            }),
        ),
        MockRoute::json(
            Method::GET,
            "/v1/vector_stores/vs_delete_primary",
            Some("Bearer sk-primary"),
            None,
            StatusCode::OK,
            serde_json::json!({
                "id": "vs_delete_primary",
                "object": "vector_store",
                "route": "stale-primary"
            }),
        ),
    ])
    .await;
    let fallback = MockUpstreamServer::spawn(vec![
        MockRoute::json(
            Method::GET,
            "/v1/files/file_delete_primary",
            Some("Bearer sk-fallback"),
            None,
            StatusCode::OK,
            serde_json::json!({
                "id": "file_delete_primary",
                "object": "file",
                "route": "fallback"
            }),
        ),
        MockRoute::json(
            Method::GET,
            "/v1/vector_stores/vs_delete_primary",
            Some("Bearer sk-fallback"),
            None,
            StatusCode::OK,
            serde_json::json!({
                "id": "vs_delete_primary",
                "object": "vector_store",
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
                request_id: "delete-affinity-create-file",
                text_fields: &[("purpose", "assistants")],
                file_field_name: "file",
                file_name: "delete-me.txt",
                file_content_type: "text/plain",
                file_bytes: b"delete affinity file",
            })
            .await,
    )
    .await;
    assert_eq!(create_file_payload["id"], "file_delete_primary");

    let create_vector_store_payload = response_json(
        harness
            .json_request(
                Method::POST,
                "/v1/vector_stores",
                "delete-affinity-create-vector-store",
                serde_json::json!({
                    "name": "vs-delete-primary"
                }),
            )
            .await,
    )
    .await;
    assert_eq!(create_vector_store_payload["id"], "vs_delete_primary");

    let delete_file_payload = response_json(
        harness
            .empty_request(
                Method::DELETE,
                "/v1/files/file_delete_primary",
                "delete-affinity-delete-file",
            )
            .await,
    )
    .await;
    assert_eq!(delete_file_payload["route"], "primary");

    let delete_vector_store_payload = response_json(
        harness
            .empty_request(
                Method::DELETE,
                "/v1/vector_stores/vs_delete_primary",
                "delete-affinity-delete-vector-store",
            )
            .await,
    )
    .await;
    assert_eq!(delete_vector_store_payload["route"], "primary");

    harness
        .promote_fallback_for_scopes(&["files", "vector_stores"])
        .await;

    let get_deleted_file_payload = response_json(
        harness
            .empty_request(
                Method::GET,
                "/v1/files/file_delete_primary",
                "delete-affinity-get-file",
            )
            .await,
    )
    .await;
    assert_eq!(get_deleted_file_payload["route"], "fallback");

    let get_deleted_vector_store_payload = response_json(
        harness
            .empty_request(
                Method::GET,
                "/v1/vector_stores/vs_delete_primary",
                "delete-affinity-get-vector-store",
            )
            .await,
    )
    .await;
    assert_eq!(get_deleted_vector_store_payload["route"], "fallback");

    assert_eq!(primary.hit_count("/v1/files"), 1);
    assert_eq!(primary.hit_count("/v1/files/file_delete_primary"), 1);
    assert_eq!(primary.hit_count("/v1/vector_stores"), 1);
    assert_eq!(primary.hit_count("/v1/vector_stores/vs_delete_primary"), 1);
    assert_eq!(fallback.hit_count("/v1/files/file_delete_primary"), 1);
    assert_eq!(fallback.hit_count("/v1/vector_stores/vs_delete_primary"), 1);

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn assistant_and_thread_delete_clear_affinity_after_successful_delete() {
    let primary = MockUpstreamServer::spawn(vec![
        MockRoute::json(
            Method::POST,
            "/v1/assistants",
            Some("Bearer sk-primary"),
            None,
            StatusCode::OK,
            serde_json::json!({
                "id": "asst_delete_primary",
                "object": "assistant",
                "model": "__MODEL__",
                "route": "primary"
            }),
        ),
        MockRoute::json(
            Method::POST,
            "/v1/threads",
            Some("Bearer sk-primary"),
            None,
            StatusCode::OK,
            serde_json::json!({
                "id": "thread_delete_primary",
                "object": "thread",
                "route": "primary"
            }),
        ),
        MockRoute::json(
            Method::DELETE,
            "/v1/assistants/asst_delete_primary",
            Some("Bearer sk-primary"),
            None,
            StatusCode::OK,
            serde_json::json!({
                "id": "asst_delete_primary",
                "object": "assistant.deleted",
                "deleted": true,
                "route": "primary"
            }),
        ),
        MockRoute::json(
            Method::GET,
            "/v1/assistants/asst_delete_primary",
            Some("Bearer sk-primary"),
            None,
            StatusCode::OK,
            serde_json::json!({
                "id": "asst_delete_primary",
                "object": "assistant",
                "route": "stale-primary"
            }),
        ),
        MockRoute::json(
            Method::DELETE,
            "/v1/threads/thread_delete_primary",
            Some("Bearer sk-primary"),
            None,
            StatusCode::OK,
            serde_json::json!({
                "id": "thread_delete_primary",
                "object": "thread.deleted",
                "deleted": true,
                "route": "primary"
            }),
        ),
        MockRoute::json(
            Method::GET,
            "/v1/threads/thread_delete_primary",
            Some("Bearer sk-primary"),
            None,
            StatusCode::OK,
            serde_json::json!({
                "id": "thread_delete_primary",
                "object": "thread",
                "route": "stale-primary"
            }),
        ),
    ])
    .await;
    let fallback = MockUpstreamServer::spawn(vec![
        MockRoute::json(
            Method::GET,
            "/v1/assistants/asst_delete_primary",
            Some("Bearer sk-fallback"),
            None,
            StatusCode::OK,
            serde_json::json!({
                "id": "asst_delete_primary",
                "object": "assistant",
                "route": "fallback"
            }),
        ),
        MockRoute::json(
            Method::GET,
            "/v1/threads/thread_delete_primary",
            Some("Bearer sk-fallback"),
            None,
            StatusCode::OK,
            serde_json::json!({
                "id": "thread_delete_primary",
                "object": "thread",
                "route": "fallback"
            }),
        ),
    ])
    .await;
    let harness = TestHarness::assistants_threads_fallback_affinity_fixture(
        &primary.base_url,
        &fallback.base_url,
    )
    .await;
    primary.replace_placeholder("__MODEL__", &harness.model_name);

    let create_assistant_payload = response_json(
        harness
            .json_request(
                Method::POST,
                "/v1/assistants",
                "delete-affinity-create-assistant",
                serde_json::json!({
                    "model": harness.model_name,
                    "name": "delete-affinity-assistant"
                }),
            )
            .await,
    )
    .await;
    assert_eq!(create_assistant_payload["id"], "asst_delete_primary");

    let create_thread_payload = response_json(
        harness
            .json_request(
                Method::POST,
                "/v1/threads",
                "delete-affinity-create-thread",
                serde_json::json!({
                    "messages": [{
                        "role": "user",
                        "content": "hello"
                    }]
                }),
            )
            .await,
    )
    .await;
    assert_eq!(create_thread_payload["id"], "thread_delete_primary");

    let delete_assistant_payload = response_json(
        harness
            .empty_request(
                Method::DELETE,
                "/v1/assistants/asst_delete_primary",
                "delete-affinity-delete-assistant",
            )
            .await,
    )
    .await;
    assert_eq!(delete_assistant_payload["route"], "primary");

    let delete_thread_payload = response_json(
        harness
            .empty_request(
                Method::DELETE,
                "/v1/threads/thread_delete_primary",
                "delete-affinity-delete-thread",
            )
            .await,
    )
    .await;
    assert_eq!(delete_thread_payload["route"], "primary");

    harness
        .promote_fallback_for_scopes(&["assistants", "threads"])
        .await;

    let get_deleted_assistant_payload = response_json(
        harness
            .empty_request(
                Method::GET,
                "/v1/assistants/asst_delete_primary",
                "delete-affinity-get-assistant",
            )
            .await,
    )
    .await;
    assert_eq!(get_deleted_assistant_payload["route"], "fallback");

    let get_deleted_thread_payload = response_json(
        harness
            .empty_request(
                Method::GET,
                "/v1/threads/thread_delete_primary",
                "delete-affinity-get-thread",
            )
            .await,
    )
    .await;
    assert_eq!(get_deleted_thread_payload["route"], "fallback");

    assert_eq!(primary.hit_count("/v1/assistants"), 1);
    assert_eq!(primary.hit_count("/v1/assistants/asst_delete_primary"), 1);
    assert_eq!(primary.hit_count("/v1/threads"), 1);
    assert_eq!(primary.hit_count("/v1/threads/thread_delete_primary"), 1);
    assert_eq!(fallback.hit_count("/v1/assistants/asst_delete_primary"), 1);
    assert_eq!(fallback.hit_count("/v1/threads/thread_delete_primary"), 1);

    harness.cleanup().await;
}
