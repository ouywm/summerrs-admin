use super::*;

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn uploads_chain_keeps_affinity_after_default_route_switch() {
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
            Method::GET,
            "/v1/uploads/upload_chain_primary",
            Some("Bearer sk-primary"),
            None,
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
                "id": "file_chain_uploaded",
                "object": "file",
                "purpose": "assistants",
                "route": "primary"
            }),
        ),
        MockRoute::json(
            Method::POST,
            "/v1/uploads/upload_chain_primary/cancel",
            Some("Bearer sk-primary"),
            None,
            StatusCode::OK,
            serde_json::json!({
                "id": "upload_chain_primary",
                "object": "upload",
                "status": "cancelled",
                "route": "primary"
            }),
        ),
    ])
    .await;
    let fallback = MockUpstreamServer::spawn(vec![MockRoute::json(
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
    )])
    .await;
    let harness =
        TestHarness::uploads_affinity_fixture(&primary.base_url, &fallback.base_url).await;

    let create_upload_payload = response_json(
        harness
            .json_request(
                Method::POST,
                "/v1/uploads",
                "uploads-chain-create-primary",
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

    harness.promote_fallback_for_scopes(&["uploads"]).await;

    let create_upload_fallback_payload = response_json(
        harness
            .json_request(
                Method::POST,
                "/v1/uploads",
                "uploads-chain-create-fallback",
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
    assert_eq!(create_upload_fallback_payload["route"], "fallback");

    let get_upload_payload = response_json(
        harness
            .empty_request(
                Method::GET,
                "/v1/uploads/upload_chain_primary",
                "uploads-chain-get-primary",
            )
            .await,
    )
    .await;
    assert_eq!(get_upload_payload["route"], "primary");

    let add_part_payload = response_json(
        harness
            .multipart_request(MultipartRequestSpec {
                uri: "/v1/uploads/upload_chain_primary/parts",
                request_id: "uploads-chain-add-part",
                text_fields: &[],
                file_field_name: "data",
                file_name: "upload-part.txt",
                file_content_type: "text/plain",
                file_bytes: b"hello upload part",
            })
            .await,
    )
    .await;
    assert_eq!(add_part_payload["route"], "primary");

    let complete_upload_payload = response_json(
        harness
            .json_request(
                Method::POST,
                "/v1/uploads/upload_chain_primary/complete",
                "uploads-chain-complete-primary",
                serde_json::json!({
                    "part_ids": ["part_chain_primary"]
                }),
            )
            .await,
    )
    .await;
    assert_eq!(complete_upload_payload["route"], "primary");

    let cancel_upload_payload = response_json(
        harness
            .empty_request(
                Method::POST,
                "/v1/uploads/upload_chain_primary/cancel",
                "uploads-chain-cancel-primary",
            )
            .await,
    )
    .await;
    assert_eq!(cancel_upload_payload["route"], "primary");

    assert_eq!(primary.hit_count("/v1/uploads"), 1);
    assert_eq!(primary.hit_count("/v1/uploads/upload_chain_primary"), 1);
    assert_eq!(
        primary.hit_count("/v1/uploads/upload_chain_primary/parts"),
        1
    );
    assert_eq!(
        primary.hit_count("/v1/uploads/upload_chain_primary/complete"),
        1
    );
    assert_eq!(
        primary.hit_count("/v1/uploads/upload_chain_primary/cancel"),
        1
    );
    assert_eq!(fallback.hit_count("/v1/uploads"), 1);

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn fine_tuning_chain_keeps_affinity_after_default_route_switch() {
    let primary = MockUpstreamServer::spawn(vec![
        MockRoute::json(
            Method::POST,
            "/v1/fine_tuning/jobs",
            Some("Bearer sk-primary"),
            Some("file_train_primary"),
            StatusCode::OK,
            serde_json::json!({
                "id": "ftjob_chain_primary",
                "object": "fine_tuning.job",
                "status": "validating_files",
                "route": "primary"
            }),
        ),
        MockRoute::json(
            Method::GET,
            "/v1/fine_tuning/jobs/ftjob_chain_primary",
            Some("Bearer sk-primary"),
            None,
            StatusCode::OK,
            serde_json::json!({
                "id": "ftjob_chain_primary",
                "object": "fine_tuning.job",
                "status": "succeeded",
                "route": "primary"
            }),
        ),
        MockRoute::json(
            Method::POST,
            "/v1/fine_tuning/jobs/ftjob_chain_primary/cancel",
            Some("Bearer sk-primary"),
            None,
            StatusCode::OK,
            serde_json::json!({
                "id": "ftjob_chain_primary",
                "object": "fine_tuning.job",
                "status": "cancelled",
                "route": "primary"
            }),
        ),
        MockRoute::json(
            Method::GET,
            "/v1/fine_tuning/jobs/ftjob_chain_primary/events",
            Some("Bearer sk-primary"),
            None,
            StatusCode::OK,
            serde_json::json!({
                "object": "list",
                "data": [{
                    "id": "ftevent_chain_primary",
                    "object": "fine_tuning.job.event",
                    "route": "primary"
                }]
            }),
        ),
        MockRoute::json(
            Method::GET,
            "/v1/fine_tuning/jobs/ftjob_chain_primary/checkpoints",
            Some("Bearer sk-primary"),
            None,
            StatusCode::OK,
            serde_json::json!({
                "object": "list",
                "data": [{
                    "id": "ftckpt_chain_primary",
                    "object": "fine_tuning.job.checkpoint",
                    "route": "primary"
                }]
            }),
        ),
    ])
    .await;
    let fallback = MockUpstreamServer::spawn(vec![MockRoute::json(
        Method::GET,
        "/v1/fine_tuning/jobs",
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
        TestHarness::fine_tuning_affinity_fixture(&primary.base_url, &fallback.base_url).await;

    let create_job_payload = response_json(
        harness
            .json_request(
                Method::POST,
                "/v1/fine_tuning/jobs",
                "fine-tuning-chain-create",
                serde_json::json!({
                    "training_file": "file_train_primary",
                    "model": harness.model_name
                }),
            )
            .await,
    )
    .await;
    assert_eq!(
        create_job_payload["id"], "ftjob_chain_primary",
        "unexpected create fine-tuning job payload: {create_job_payload}"
    );

    harness.promote_fallback_for_scopes(&["fine_tuning"]).await;

    let list_jobs_payload = response_json(
        harness
            .empty_request(
                Method::GET,
                "/v1/fine_tuning/jobs",
                "fine-tuning-chain-list",
            )
            .await,
    )
    .await;
    assert_eq!(list_jobs_payload["route"], "fallback");

    let get_job_payload = response_json(
        harness
            .empty_request(
                Method::GET,
                "/v1/fine_tuning/jobs/ftjob_chain_primary",
                "fine-tuning-chain-get",
            )
            .await,
    )
    .await;
    assert_eq!(get_job_payload["route"], "primary");

    let list_events_payload = response_json(
        harness
            .empty_request(
                Method::GET,
                "/v1/fine_tuning/jobs/ftjob_chain_primary/events",
                "fine-tuning-chain-events",
            )
            .await,
    )
    .await;
    assert_eq!(list_events_payload["data"][0]["route"], "primary");

    let list_checkpoints_payload = response_json(
        harness
            .empty_request(
                Method::GET,
                "/v1/fine_tuning/jobs/ftjob_chain_primary/checkpoints",
                "fine-tuning-chain-checkpoints",
            )
            .await,
    )
    .await;
    assert_eq!(list_checkpoints_payload["data"][0]["route"], "primary");

    let cancel_job_payload = response_json(
        harness
            .empty_request(
                Method::POST,
                "/v1/fine_tuning/jobs/ftjob_chain_primary/cancel",
                "fine-tuning-chain-cancel",
            )
            .await,
    )
    .await;
    assert_eq!(cancel_job_payload["route"], "primary");

    assert_eq!(primary.hit_count("/v1/fine_tuning/jobs"), 1);
    assert_eq!(
        primary.hit_count("/v1/fine_tuning/jobs/ftjob_chain_primary"),
        1
    );
    assert_eq!(
        primary.hit_count("/v1/fine_tuning/jobs/ftjob_chain_primary/events"),
        1
    );
    assert_eq!(
        primary.hit_count("/v1/fine_tuning/jobs/ftjob_chain_primary/checkpoints"),
        1
    );
    assert_eq!(
        primary.hit_count("/v1/fine_tuning/jobs/ftjob_chain_primary/cancel"),
        1
    );
    assert_eq!(fallback.hit_count("/v1/fine_tuning/jobs"), 1);

    harness.cleanup().await;
}
