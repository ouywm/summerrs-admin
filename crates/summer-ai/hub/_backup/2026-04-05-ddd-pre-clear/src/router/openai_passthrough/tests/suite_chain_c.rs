use super::*;

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn batches_chain_keeps_affinity_after_default_route_switch() {
    let primary = MockUpstreamServer::spawn(vec![
        MockRoute::json(
            Method::POST,
            "/v1/batches",
            Some("Bearer sk-primary"),
            Some("/v1/responses"),
            StatusCode::OK,
            serde_json::json!({
                "id": "batch_chain_primary",
                "object": "batch",
                "endpoint": "/v1/responses",
                "status": "validating",
                "route": "primary"
            }),
        ),
        MockRoute::json(
            Method::GET,
            "/v1/batches/batch_chain_primary",
            Some("Bearer sk-primary"),
            None,
            StatusCode::OK,
            serde_json::json!({
                "id": "batch_chain_primary",
                "object": "batch",
                "status": "completed",
                "route": "primary"
            }),
        ),
        MockRoute::json(
            Method::POST,
            "/v1/batches/batch_chain_primary/cancel",
            Some("Bearer sk-primary"),
            None,
            StatusCode::OK,
            serde_json::json!({
                "id": "batch_chain_primary",
                "object": "batch",
                "status": "cancelling",
                "route": "primary"
            }),
        ),
    ])
    .await;
    let fallback = MockUpstreamServer::spawn(vec![MockRoute::json(
        Method::GET,
        "/v1/batches",
        Some("Bearer sk-fallback"),
        None,
        StatusCode::OK,
        serde_json::json!({
            "object": "list",
            "data": [],
            "route": "fallback"
        }),
    )])
    .await;
    let harness =
        TestHarness::batches_affinity_fixture(&primary.base_url, &fallback.base_url).await;

    let create_batch_payload = response_json(
        harness
            .json_request(
                Method::POST,
                "/v1/batches",
                "batches-chain-create",
                serde_json::json!({
                    "input_file_id": "file_input_batch",
                    "endpoint": "/v1/responses",
                    "completion_window": "24h"
                }),
            )
            .await,
    )
    .await;
    assert_eq!(create_batch_payload["id"], "batch_chain_primary");

    harness.promote_fallback_for_scopes(&["batches"]).await;

    let list_batches_payload = response_json(
        harness
            .empty_request(Method::GET, "/v1/batches", "batches-chain-list")
            .await,
    )
    .await;
    assert_eq!(list_batches_payload["route"], "fallback");

    let get_batch_payload = response_json(
        harness
            .empty_request(
                Method::GET,
                "/v1/batches/batch_chain_primary",
                "batches-chain-get",
            )
            .await,
    )
    .await;
    assert_eq!(get_batch_payload["route"], "primary");

    let cancel_batch_payload = response_json(
        harness
            .empty_request(
                Method::POST,
                "/v1/batches/batch_chain_primary/cancel",
                "batches-chain-cancel",
            )
            .await,
    )
    .await;
    assert_eq!(cancel_batch_payload["route"], "primary");

    assert_eq!(primary.hit_count("/v1/batches"), 1);
    assert_eq!(primary.hit_count("/v1/batches/batch_chain_primary"), 1);
    assert_eq!(
        primary.hit_count("/v1/batches/batch_chain_primary/cancel"),
        1
    );
    assert_eq!(fallback.hit_count("/v1/batches"), 1);

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn vector_store_file_batch_chain_keeps_affinity_after_default_route_switch() {
    let primary = MockUpstreamServer::spawn(vec![
        MockRoute::json(
            Method::POST,
            "/v1/vector_stores",
            Some("Bearer sk-primary"),
            Some("vs-batch-primary"),
            StatusCode::OK,
            serde_json::json!({
                "id": "vs_batch_primary",
                "object": "vector_store",
                "name": "vs-batch-primary",
                "route": "primary"
            }),
        ),
        MockRoute::json(
            Method::GET,
            "/v1/vector_stores/vs_batch_primary/files",
            Some("Bearer sk-primary"),
            None,
            StatusCode::OK,
            serde_json::json!({
                "object": "list",
                "data": [{
                    "id": "file_chain_primary",
                    "object": "vector_store.file",
                    "route": "primary"
                }]
            }),
        ),
        MockRoute::json(
            Method::POST,
            "/v1/vector_stores/vs_batch_primary/file_batches",
            Some("Bearer sk-primary"),
            Some("file_chain_primary"),
            StatusCode::OK,
            serde_json::json!({
                "id": "vsfb_chain_primary",
                "object": "vector_store.file_batch",
                "vector_store_id": "vs_batch_primary",
                "status": "in_progress",
                "route": "primary"
            }),
        ),
        MockRoute::json(
            Method::GET,
            "/v1/vector_stores/vs_batch_primary/file_batches",
            Some("Bearer sk-primary"),
            None,
            StatusCode::OK,
            serde_json::json!({
                "object": "list",
                "data": [{
                    "id": "vsfb_chain_primary",
                    "object": "vector_store.file_batch",
                    "route": "primary"
                }]
            }),
        ),
        MockRoute::json(
            Method::GET,
            "/v1/vector_stores/vs_batch_primary/file_batches/vsfb_chain_primary",
            Some("Bearer sk-primary"),
            None,
            StatusCode::OK,
            serde_json::json!({
                "id": "vsfb_chain_primary",
                "object": "vector_store.file_batch",
                "vector_store_id": "vs_batch_primary",
                "route": "primary"
            }),
        ),
        MockRoute::json(
            Method::POST,
            "/v1/vector_stores/vs_batch_primary/file_batches/vsfb_chain_primary/cancel",
            Some("Bearer sk-primary"),
            None,
            StatusCode::OK,
            serde_json::json!({
                "id": "vsfb_chain_primary",
                "object": "vector_store.file_batch",
                "vector_store_id": "vs_batch_primary",
                "status": "cancelled",
                "route": "primary"
            }),
        ),
    ])
    .await;
    let fallback = MockUpstreamServer::spawn(vec![MockRoute::json(
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
    )])
    .await;
    let harness =
        TestHarness::files_vector_stores_affinity_fixture(&primary.base_url, &fallback.base_url)
            .await;

    let create_vector_store_payload = response_json(
        harness
            .json_request(
                Method::POST,
                "/v1/vector_stores",
                "vs-file-batch-create-store",
                serde_json::json!({
                    "name": "vs-batch-primary"
                }),
            )
            .await,
    )
    .await;
    assert_eq!(create_vector_store_payload["id"], "vs_batch_primary");

    harness
        .promote_fallback_for_scopes(&["vector_stores"])
        .await;

    let list_vector_stores_payload = response_json(
        harness
            .empty_request(
                Method::GET,
                "/v1/vector_stores",
                "vs-file-batch-list-stores",
            )
            .await,
    )
    .await;
    assert_eq!(list_vector_stores_payload["route"], "fallback");

    let list_vector_store_files_payload = response_json(
        harness
            .empty_request(
                Method::GET,
                "/v1/vector_stores/vs_batch_primary/files",
                "vs-file-batch-list-files",
            )
            .await,
    )
    .await;
    assert_eq!(
        list_vector_store_files_payload["data"][0]["route"],
        "primary"
    );

    let create_file_batch_payload = response_json(
        harness
            .json_request(
                Method::POST,
                "/v1/vector_stores/vs_batch_primary/file_batches",
                "vs-file-batch-create-batch",
                serde_json::json!({
                    "file_ids": ["file_chain_primary"]
                }),
            )
            .await,
    )
    .await;
    assert_eq!(create_file_batch_payload["id"], "vsfb_chain_primary");

    let list_file_batches_payload = response_json(
        harness
            .empty_request(
                Method::GET,
                "/v1/vector_stores/vs_batch_primary/file_batches",
                "vs-file-batch-list-batches",
            )
            .await,
    )
    .await;
    assert_eq!(list_file_batches_payload["data"][0]["route"], "primary");

    let get_file_batch_payload = response_json(
        harness
            .empty_request(
                Method::GET,
                "/v1/vector_stores/vs_batch_primary/file_batches/vsfb_chain_primary",
                "vs-file-batch-get-batch",
            )
            .await,
    )
    .await;
    assert_eq!(get_file_batch_payload["route"], "primary");

    let cancel_file_batch_payload = response_json(
        harness
            .empty_request(
                Method::POST,
                "/v1/vector_stores/vs_batch_primary/file_batches/vsfb_chain_primary/cancel",
                "vs-file-batch-cancel-batch",
            )
            .await,
    )
    .await;
    assert_eq!(cancel_file_batch_payload["route"], "primary");

    assert_eq!(primary.hit_count("/v1/vector_stores"), 1);
    assert_eq!(
        primary.hit_count("/v1/vector_stores/vs_batch_primary/files"),
        1
    );
    assert_eq!(
        primary.hit_count("/v1/vector_stores/vs_batch_primary/file_batches"),
        2
    );
    assert_eq!(
        primary.hit_count("/v1/vector_stores/vs_batch_primary/file_batches/vsfb_chain_primary"),
        1
    );
    assert_eq!(
        primary
            .hit_count("/v1/vector_stores/vs_batch_primary/file_batches/vsfb_chain_primary/cancel"),
        1
    );
    assert_eq!(fallback.hit_count("/v1/vector_stores"), 1);

    harness.cleanup().await;
}
