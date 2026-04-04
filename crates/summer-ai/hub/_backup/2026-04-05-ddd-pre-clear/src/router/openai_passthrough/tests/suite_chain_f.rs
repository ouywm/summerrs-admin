use super::*;

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn upload_complete_binds_file_affinity_after_default_route_switch() {
    let primary = MockUpstreamServer::spawn(vec![
        MockRoute::json(
            Method::POST,
            "/v1/uploads",
            Some("Bearer sk-primary"),
            Some("upload-chain-primary.bin"),
            StatusCode::OK,
            serde_json::json!({
                "id": "upload_chain_primary",
                "object": "upload",
                "status": "in_progress",
                "route": "primary"
            }),
        ),
        MockRoute::json(
            Method::POST,
            "/v1/uploads/upload_chain_primary/parts",
            Some("Bearer sk-primary"),
            Some("upload-part.txt"),
            StatusCode::OK,
            serde_json::json!({
                "id": "part_chain_primary",
                "object": "upload.part",
                "route": "primary"
            }),
        ),
        MockRoute::json(
            Method::POST,
            "/v1/uploads/upload_chain_primary/complete",
            Some("Bearer sk-primary"),
            Some("part_chain_primary"),
            StatusCode::OK,
            serde_json::json!({
                "id": "file_completed_primary",
                "object": "file",
                "purpose": "assistants",
                "route": "primary"
            }),
        ),
        MockRoute::json(
            Method::GET,
            "/v1/uploads/upload_chain_primary",
            Some("Bearer sk-primary"),
            None,
            StatusCode::OK,
            serde_json::json!({
                "id": "upload_chain_primary",
                "object": "upload",
                "route": "primary"
            }),
        ),
        MockRoute::json(
            Method::GET,
            "/v1/files/file_completed_primary",
            Some("Bearer sk-primary"),
            None,
            StatusCode::OK,
            serde_json::json!({
                "id": "file_completed_primary",
                "object": "file",
                "route": "primary"
            }),
        ),
    ])
    .await;
    let fallback = MockUpstreamServer::spawn(vec![
        MockRoute::json(
            Method::POST,
            "/v1/uploads",
            Some("Bearer sk-fallback"),
            Some("upload-chain-fallback.bin"),
            StatusCode::OK,
            serde_json::json!({
                "id": "upload_chain_fallback",
                "object": "upload",
                "status": "in_progress",
                "route": "fallback"
            }),
        ),
        MockRoute::json(
            Method::GET,
            "/v1/uploads/upload_chain_primary",
            Some("Bearer sk-fallback"),
            None,
            StatusCode::OK,
            serde_json::json!({
                "id": "upload_chain_primary",
                "object": "upload",
                "route": "fallback"
            }),
        ),
        MockRoute::json(
            Method::GET,
            "/v1/files/file_completed_primary",
            Some("Bearer sk-fallback"),
            None,
            StatusCode::OK,
            serde_json::json!({
                "id": "file_completed_primary",
                "object": "file",
                "route": "fallback"
            }),
        ),
    ])
    .await;
    let harness =
        TestHarness::uploads_files_affinity_fixture(&primary.base_url, &fallback.base_url).await;

    let create_upload_payload = response_json(
        harness
            .json_request(
                Method::POST,
                "/v1/uploads",
                "uploads-file-affinity-create-primary",
                serde_json::json!({
                    "filename": "upload-chain-primary.bin",
                    "purpose": "assistants",
                    "bytes": 18,
                    "mime_type": "application/octet-stream"
                }),
            )
            .await,
    )
    .await;
    assert_eq!(create_upload_payload["id"], "upload_chain_primary");

    let add_part_payload = response_json(
        harness
            .multipart_request(MultipartRequestSpec {
                uri: "/v1/uploads/upload_chain_primary/parts",
                request_id: "uploads-file-affinity-add-part",
                text_fields: &[],
                file_field_name: "data",
                file_name: "upload-part.txt",
                file_content_type: "text/plain",
                file_bytes: b"hello upload part",
            })
            .await,
    )
    .await;
    assert_eq!(add_part_payload["id"], "part_chain_primary");

    let complete_upload_payload = response_json(
        harness
            .json_request(
                Method::POST,
                "/v1/uploads/upload_chain_primary/complete",
                "uploads-file-affinity-complete",
                serde_json::json!({
                    "part_ids": ["part_chain_primary"]
                }),
            )
            .await,
    )
    .await;
    assert_eq!(complete_upload_payload["id"], "file_completed_primary");

    harness
        .promote_fallback_for_scopes(&["uploads", "files"])
        .await;

    let create_fallback_upload_payload = response_json(
        harness
            .json_request(
                Method::POST,
                "/v1/uploads",
                "uploads-file-affinity-create-fallback",
                serde_json::json!({
                    "filename": "upload-chain-fallback.bin",
                    "purpose": "assistants",
                    "bytes": 20,
                    "mime_type": "application/octet-stream"
                }),
            )
            .await,
    )
    .await;
    assert_eq!(create_fallback_upload_payload["route"], "fallback");

    let get_original_upload_payload = response_json(
        harness
            .empty_request(
                Method::GET,
                "/v1/uploads/upload_chain_primary",
                "uploads-file-affinity-get-upload",
            )
            .await,
    )
    .await;
    assert_eq!(get_original_upload_payload["route"], "primary");

    let get_completed_file_payload = response_json(
        harness
            .empty_request(
                Method::GET,
                "/v1/files/file_completed_primary",
                "uploads-file-affinity-get-file",
            )
            .await,
    )
    .await;
    assert_eq!(get_completed_file_payload["route"], "primary");

    assert_eq!(primary.hit_count("/v1/uploads"), 1);
    assert_eq!(
        primary.hit_count("/v1/uploads/upload_chain_primary/parts"),
        1
    );
    assert_eq!(
        primary.hit_count("/v1/uploads/upload_chain_primary/complete"),
        1
    );
    assert_eq!(primary.hit_count("/v1/uploads/upload_chain_primary"), 1);
    assert_eq!(primary.hit_count("/v1/files/file_completed_primary"), 1);
    assert_eq!(fallback.hit_count("/v1/uploads"), 1);
    assert_eq!(fallback.hit_count("/v1/uploads/upload_chain_primary"), 0);
    assert_eq!(fallback.hit_count("/v1/files/file_completed_primary"), 0);

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn delete_vector_store_file_keeps_file_and_vector_store_affinity() {
    let primary = MockUpstreamServer::spawn(vec![
        MockRoute::json(
            Method::POST,
            "/v1/vector_stores",
            Some("Bearer sk-primary"),
            Some("vs-keep-primary"),
            StatusCode::OK,
            serde_json::json!({
                "id": "vs_keep_primary",
                "object": "vector_store",
                "route": "primary"
            }),
        ),
        MockRoute::json(
            Method::POST,
            "/v1/vector_stores/vs_keep_primary/files",
            Some("Bearer sk-primary"),
            Some("file_keep_primary"),
            StatusCode::OK,
            serde_json::json!({
                "id": "file_keep_primary",
                "object": "vector_store.file",
                "vector_store_id": "vs_keep_primary",
                "route": "primary"
            }),
        ),
        MockRoute::json(
            Method::DELETE,
            "/v1/vector_stores/vs_keep_primary/files/file_keep_primary",
            Some("Bearer sk-primary"),
            None,
            StatusCode::OK,
            serde_json::json!({
                "id": "file_keep_primary",
                "object": "vector_store.file.deleted",
                "deleted": true,
                "route": "primary"
            }),
        ),
        MockRoute::json(
            Method::GET,
            "/v1/vector_stores/vs_keep_primary",
            Some("Bearer sk-primary"),
            None,
            StatusCode::OK,
            serde_json::json!({
                "id": "vs_keep_primary",
                "object": "vector_store",
                "route": "primary"
            }),
        ),
        MockRoute::json(
            Method::GET,
            "/v1/files/file_keep_primary",
            Some("Bearer sk-primary"),
            None,
            StatusCode::OK,
            serde_json::json!({
                "id": "file_keep_primary",
                "object": "file",
                "route": "primary"
            }),
        ),
    ])
    .await;
    let fallback = MockUpstreamServer::spawn(vec![
        MockRoute::json(
            Method::GET,
            "/v1/vector_stores/vs_keep_primary",
            Some("Bearer sk-fallback"),
            None,
            StatusCode::OK,
            serde_json::json!({
                "id": "vs_keep_primary",
                "object": "vector_store",
                "route": "fallback"
            }),
        ),
        MockRoute::json(
            Method::GET,
            "/v1/files/file_keep_primary",
            Some("Bearer sk-fallback"),
            None,
            StatusCode::OK,
            serde_json::json!({
                "id": "file_keep_primary",
                "object": "file",
                "route": "fallback"
            }),
        ),
    ])
    .await;
    let harness =
        TestHarness::files_vector_stores_affinity_fixture(&primary.base_url, &fallback.base_url)
            .await;

    let create_vector_store_payload = response_json(
        harness
            .json_request(
                Method::POST,
                "/v1/vector_stores",
                "keep-affinity-create-vector-store",
                serde_json::json!({
                    "name": "vs-keep-primary"
                }),
            )
            .await,
    )
    .await;
    assert_eq!(create_vector_store_payload["id"], "vs_keep_primary");

    let attach_file_payload = response_json(
        harness
            .json_request(
                Method::POST,
                "/v1/vector_stores/vs_keep_primary/files",
                "keep-affinity-attach-file",
                serde_json::json!({
                    "file_id": "file_keep_primary"
                }),
            )
            .await,
    )
    .await;
    assert_eq!(attach_file_payload["id"], "file_keep_primary");

    let delete_vector_store_file_payload = response_json(
        harness
            .empty_request(
                Method::DELETE,
                "/v1/vector_stores/vs_keep_primary/files/file_keep_primary",
                "keep-affinity-delete-vector-store-file",
            )
            .await,
    )
    .await;
    assert_eq!(delete_vector_store_file_payload["route"], "primary");

    harness
        .promote_fallback_for_scopes(&["files", "vector_stores"])
        .await;

    let get_vector_store_payload = response_json(
        harness
            .empty_request(
                Method::GET,
                "/v1/vector_stores/vs_keep_primary",
                "keep-affinity-get-vector-store",
            )
            .await,
    )
    .await;
    assert_eq!(get_vector_store_payload["route"], "primary");

    let get_file_payload = response_json(
        harness
            .empty_request(
                Method::GET,
                "/v1/files/file_keep_primary",
                "keep-affinity-get-file",
            )
            .await,
    )
    .await;
    assert_eq!(get_file_payload["route"], "primary");

    assert_eq!(primary.hit_count("/v1/vector_stores"), 1);
    assert_eq!(
        primary.hit_count("/v1/vector_stores/vs_keep_primary/files"),
        1
    );
    assert_eq!(
        primary.hit_count("/v1/vector_stores/vs_keep_primary/files/file_keep_primary"),
        1
    );
    assert_eq!(primary.hit_count("/v1/vector_stores/vs_keep_primary"), 1);
    assert_eq!(primary.hit_count("/v1/files/file_keep_primary"), 1);
    assert_eq!(fallback.hit_count("/v1/vector_stores/vs_keep_primary"), 0);
    assert_eq!(fallback.hit_count("/v1/files/file_keep_primary"), 0);

    harness.cleanup().await;
}
