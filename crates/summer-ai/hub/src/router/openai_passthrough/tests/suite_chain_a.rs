use super::*;

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn responses_resource_chain_prefers_bound_channel_over_default_fallback() {
    let primary = MockUpstreamServer::spawn(vec![
        MockRoute::json(
            Method::POST,
            "/v1/responses",
            Some("Bearer sk-primary"),
            None,
            StatusCode::OK,
            serde_json::json!({
                "id": "resp_chain_primary",
                "object": "response",
                "status": "completed",
                "model": "__MODEL__",
                "output": [],
                "usage": {
                    "input_tokens": 3,
                    "output_tokens": 2,
                    "total_tokens": 5
                }
            }),
        ),
        MockRoute::json(
            Method::GET,
            "/v1/responses/resp_chain_primary",
            Some("Bearer sk-primary"),
            None,
            StatusCode::OK,
            serde_json::json!({
                "id": "resp_chain_primary",
                "object": "response",
                "status": "completed",
                "route": "primary"
            }),
        ),
        MockRoute::json(
            Method::GET,
            "/v1/responses/resp_chain_primary/input_items",
            Some("Bearer sk-primary"),
            None,
            StatusCode::OK,
            serde_json::json!({
                "object": "list",
                "data": [{
                    "id": "item_primary_1",
                    "type": "message",
                    "route": "primary"
                }]
            }),
        ),
        MockRoute::json(
            Method::POST,
            "/v1/responses/resp_chain_primary/cancel",
            Some("Bearer sk-primary"),
            None,
            StatusCode::OK,
            serde_json::json!({
                "id": "resp_chain_primary",
                "object": "response",
                "status": "cancelled",
                "route": "primary"
            }),
        ),
    ])
    .await;
    let fallback = MockUpstreamServer::spawn(vec![]).await;
    let harness =
        TestHarness::responses_affinity_fixture(&primary.base_url, &fallback.base_url).await;
    primary.replace_placeholder("__MODEL__", &harness.model_name);

    let create_response = harness
        .json_request(
            Method::POST,
            "/v1/responses",
            "responses-chain-create",
            serde_json::json!({
                "model": harness.model_name,
                "input": "hello from responses chain",
                "stream": false
            }),
        )
        .await;
    assert_eq!(create_response.status(), StatusCode::OK);
    let create_payload = response_json(create_response).await;
    assert_eq!(create_payload["id"], "resp_chain_primary");

    let get_response_payload = response_json(
        harness
            .empty_request(
                Method::GET,
                "/v1/responses/resp_chain_primary",
                "responses-chain-get",
            )
            .await,
    )
    .await;
    assert_eq!(get_response_payload["route"], "primary");

    let input_items_payload = response_json(
        harness
            .empty_request(
                Method::GET,
                "/v1/responses/resp_chain_primary/input_items",
                "responses-chain-input-items",
            )
            .await,
    )
    .await;
    assert_eq!(input_items_payload["data"][0]["route"], "primary");

    let cancel_payload = response_json(
        harness
            .empty_request(
                Method::POST,
                "/v1/responses/resp_chain_primary/cancel",
                "responses-chain-cancel",
            )
            .await,
    )
    .await;
    assert_eq!(cancel_payload["route"], "primary");

    assert_eq!(primary.hit_count("/v1/responses"), 1);
    assert_eq!(primary.hit_count("/v1/responses/resp_chain_primary"), 1);
    assert_eq!(
        primary.hit_count("/v1/responses/resp_chain_primary/input_items"),
        1
    );
    assert_eq!(
        primary.hit_count("/v1/responses/resp_chain_primary/cancel"),
        1
    );
    assert_eq!(fallback.total_hits(), 0);

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn assistants_thread_runs_chain_reuses_assistant_and_run_affinity() {
    let primary = MockUpstreamServer::spawn(vec![
        MockRoute::json(
            Method::POST,
            "/v1/assistants",
            Some("Bearer sk-primary"),
            None,
            StatusCode::OK,
            serde_json::json!({
                "id": "asst_chain_primary",
                "object": "assistant",
                "model": "__MODEL__"
            }),
        ),
        MockRoute::json(
            Method::POST,
            "/v1/threads/runs",
            Some("Bearer sk-primary"),
            None,
            StatusCode::OK,
            serde_json::json!({
                "id": "run_chain_primary",
                "object": "thread.run",
                "thread_id": "thread_chain_primary",
                "assistant_id": "asst_chain_primary",
                "model": "__MODEL__",
                "status": "completed",
                "usage": {
                    "prompt_tokens": 4,
                    "completion_tokens": 3,
                    "total_tokens": 7
                }
            }),
        ),
        MockRoute::json(
            Method::POST,
            "/v1/threads/thread_chain_primary/runs/run_chain_primary/submit_tool_outputs",
            Some("Bearer sk-primary"),
            None,
            StatusCode::OK,
            serde_json::json!({
                "id": "run_chain_primary",
                "object": "thread.run",
                "thread_id": "thread_chain_primary",
                "assistant_id": "asst_chain_primary",
                "model": "__MODEL__",
                "status": "completed",
                "route": "primary",
                "usage": {
                    "prompt_tokens": 2,
                    "completion_tokens": 1,
                    "total_tokens": 3
                }
            }),
        ),
        MockRoute::json(
            Method::GET,
            "/v1/threads/thread_chain_primary/runs/run_chain_primary",
            Some("Bearer sk-primary"),
            None,
            StatusCode::OK,
            serde_json::json!({
                "id": "run_chain_primary",
                "object": "thread.run",
                "thread_id": "thread_chain_primary",
                "assistant_id": "asst_chain_primary",
                "status": "completed",
                "route": "primary"
            }),
        ),
        MockRoute::json(
            Method::GET,
            "/v1/threads/thread_chain_primary/runs/run_chain_primary/steps",
            Some("Bearer sk-primary"),
            None,
            StatusCode::OK,
            serde_json::json!({
                "object": "list",
                "data": [{
                    "id": "step_chain_primary",
                    "object": "thread.run.step",
                    "route": "primary"
                }]
            }),
        ),
        MockRoute::json(
            Method::GET,
            "/v1/threads/thread_chain_primary/runs/run_chain_primary/steps/step_chain_primary",
            Some("Bearer sk-primary"),
            None,
            StatusCode::OK,
            serde_json::json!({
                "id": "step_chain_primary",
                "object": "thread.run.step",
                "route": "primary"
            }),
        ),
    ])
    .await;
    let fallback = MockUpstreamServer::spawn(vec![]).await;
    let harness =
        TestHarness::assistants_threads_affinity_fixture(&primary.base_url, &fallback.base_url)
            .await;
    primary.replace_placeholder("__MODEL__", &harness.model_name);

    let create_assistant_payload = response_json(
        harness
            .json_request(
                Method::POST,
                "/v1/assistants",
                "assistant-chain-create",
                serde_json::json!({
                    "model": harness.model_name,
                    "name": "integration assistant"
                }),
            )
            .await,
    )
    .await;
    assert_eq!(create_assistant_payload["id"], "asst_chain_primary");

    let create_run_payload = response_json(
        harness
            .json_request(
                Method::POST,
                "/v1/threads/runs",
                "assistant-chain-run",
                serde_json::json!({
                    "assistant_id": "asst_chain_primary",
                    "thread": {
                        "messages": [{
                            "role": "user",
                            "content": "hello"
                        }]
                    }
                }),
            )
            .await,
    )
    .await;
    assert_eq!(create_run_payload["id"], "run_chain_primary");

    let submit_payload = response_json(
        harness
            .json_request(
                Method::POST,
                "/v1/threads/thread_chain_primary/runs/run_chain_primary/submit_tool_outputs",
                "assistant-chain-submit",
                serde_json::json!({
                    "tool_outputs": [{
                        "tool_call_id": "call_123",
                        "output": "done"
                    }]
                }),
            )
            .await,
    )
    .await;
    assert_eq!(submit_payload["route"], "primary");

    let get_run_payload = response_json(
        harness
            .empty_request(
                Method::GET,
                "/v1/threads/thread_chain_primary/runs/run_chain_primary",
                "assistant-chain-get-run",
            )
            .await,
    )
    .await;
    assert_eq!(get_run_payload["route"], "primary");

    let list_steps_payload = response_json(
        harness
            .empty_request(
                Method::GET,
                "/v1/threads/thread_chain_primary/runs/run_chain_primary/steps",
                "assistant-chain-list-steps",
            )
            .await,
    )
    .await;
    assert_eq!(list_steps_payload["data"][0]["route"], "primary");

    let get_step_payload = response_json(
        harness
            .empty_request(
                Method::GET,
                "/v1/threads/thread_chain_primary/runs/run_chain_primary/steps/step_chain_primary",
                "assistant-chain-get-step",
            )
            .await,
    )
    .await;
    assert_eq!(get_step_payload["route"], "primary");

    assert_eq!(primary.hit_count("/v1/assistants"), 1);
    assert_eq!(primary.hit_count("/v1/threads/runs"), 1);
    assert_eq!(
        primary.hit_count(
            "/v1/threads/thread_chain_primary/runs/run_chain_primary/submit_tool_outputs"
        ),
        1
    );
    assert_eq!(
        primary.hit_count("/v1/threads/thread_chain_primary/runs/run_chain_primary"),
        1
    );
    assert_eq!(
        primary.hit_count("/v1/threads/thread_chain_primary/runs/run_chain_primary/steps"),
        1
    );
    assert_eq!(
        primary.hit_count(
            "/v1/threads/thread_chain_primary/runs/run_chain_primary/steps/step_chain_primary"
        ),
        1
    );
    assert_eq!(fallback.total_hits(), 0);

    harness.cleanup().await;
}
