use super::*;

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn model_passthrough_endpoints_follow_default_route_switch() {
    let primary = MockUpstreamServer::spawn(vec![
        MockRoute::json(
            Method::POST,
            "/v1/chat/completions",
            Some("Bearer sk-primary"),
            Some("tell me a joke"),
            StatusCode::OK,
            serde_json::json!({
                "id": "chatcmpl_primary",
                "object": "chat.completion",
                "created": 1,
                "model": "__MODEL__",
                "choices": [{
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": "primary"
                    },
                    "finish_reason": "stop"
                }],
                "usage": {
                    "prompt_tokens": 2,
                    "completion_tokens": 1,
                    "total_tokens": 3
                }
            }),
        ),
        MockRoute::json(
            Method::POST,
            "/v1/images/generations",
            Some("Bearer sk-primary"),
            Some("draw a sunset"),
            StatusCode::OK,
            serde_json::json!({
                "created": 1,
                "data": [{
                    "url": "https://primary.example/image.png",
                    "route": "primary"
                }]
            }),
        ),
        MockRoute::json(
            Method::POST,
            "/v1/images/edits",
            Some("Bearer sk-primary"),
            Some("edit-primary.png"),
            StatusCode::OK,
            serde_json::json!({
                "created": 1,
                "data": [{
                    "b64_json": "primary-edit",
                    "route": "primary"
                }]
            }),
        ),
        MockRoute::json(
            Method::POST,
            "/v1/images/variations",
            Some("Bearer sk-primary"),
            Some("variation-primary.png"),
            StatusCode::OK,
            serde_json::json!({
                "created": 1,
                "data": [{
                    "b64_json": "primary-variation",
                    "route": "primary"
                }]
            }),
        ),
        MockRoute::json(
            Method::POST,
            "/v1/audio/transcriptions",
            Some("Bearer sk-primary"),
            Some("voice-primary.wav"),
            StatusCode::OK,
            serde_json::json!({
                "text": "primary transcript",
                "route": "primary"
            }),
        ),
        MockRoute::json(
            Method::POST,
            "/v1/audio/translations",
            Some("Bearer sk-primary"),
            Some("voice-translation-primary.wav"),
            StatusCode::OK,
            serde_json::json!({
                "text": "primary translation",
                "route": "primary"
            }),
        ),
        MockRoute::raw(
            Method::POST,
            "/v1/audio/speech",
            Some("Bearer sk-primary"),
            Some("say hello from primary"),
            StatusCode::OK,
            "audio/mpeg",
            "primary-audio",
        ),
        MockRoute::json(
            Method::POST,
            "/v1/moderations",
            Some("Bearer sk-primary"),
            Some("moderate primary text"),
            StatusCode::OK,
            serde_json::json!({
                "id": "modr_primary",
                "model": "__MODEL__",
                "results": [{
                    "flagged": false,
                    "route": "primary"
                }]
            }),
        ),
        MockRoute::json(
            Method::POST,
            "/v1/rerank",
            Some("Bearer sk-primary"),
            Some("rerank primary query"),
            StatusCode::OK,
            serde_json::json!({
                "results": [{
                    "index": 0,
                    "relevance_score": 0.91,
                    "route": "primary"
                }]
            }),
        ),
    ])
    .await;
    let fallback = MockUpstreamServer::spawn(vec![
        MockRoute::json(
            Method::POST,
            "/v1/chat/completions",
            Some("Bearer sk-fallback"),
            Some("tell me a joke"),
            StatusCode::OK,
            serde_json::json!({
                "id": "chatcmpl_fallback",
                "object": "chat.completion",
                "created": 1,
                "model": "__MODEL__",
                "choices": [{
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": "fallback"
                    },
                    "finish_reason": "stop"
                }],
                "usage": {
                    "prompt_tokens": 2,
                    "completion_tokens": 1,
                    "total_tokens": 3
                }
            }),
        ),
        MockRoute::json(
            Method::POST,
            "/v1/images/generations",
            Some("Bearer sk-fallback"),
            Some("draw a sunset"),
            StatusCode::OK,
            serde_json::json!({
                "created": 1,
                "data": [{
                    "url": "https://fallback.example/image.png",
                    "route": "fallback"
                }]
            }),
        ),
        MockRoute::json(
            Method::POST,
            "/v1/images/edits",
            Some("Bearer sk-fallback"),
            Some("edit-fallback.png"),
            StatusCode::OK,
            serde_json::json!({
                "created": 1,
                "data": [{
                    "b64_json": "fallback-edit",
                    "route": "fallback"
                }]
            }),
        ),
        MockRoute::json(
            Method::POST,
            "/v1/images/variations",
            Some("Bearer sk-fallback"),
            Some("variation-fallback.png"),
            StatusCode::OK,
            serde_json::json!({
                "created": 1,
                "data": [{
                    "b64_json": "fallback-variation",
                    "route": "fallback"
                }]
            }),
        ),
        MockRoute::json(
            Method::POST,
            "/v1/audio/transcriptions",
            Some("Bearer sk-fallback"),
            Some("voice-fallback.wav"),
            StatusCode::OK,
            serde_json::json!({
                "text": "fallback transcript",
                "route": "fallback"
            }),
        ),
        MockRoute::json(
            Method::POST,
            "/v1/audio/translations",
            Some("Bearer sk-fallback"),
            Some("voice-translation-fallback.wav"),
            StatusCode::OK,
            serde_json::json!({
                "text": "fallback translation",
                "route": "fallback"
            }),
        ),
        MockRoute::raw(
            Method::POST,
            "/v1/audio/speech",
            Some("Bearer sk-fallback"),
            Some("say hello from fallback"),
            StatusCode::OK,
            "audio/mpeg",
            "fallback-audio",
        ),
        MockRoute::json(
            Method::POST,
            "/v1/moderations",
            Some("Bearer sk-fallback"),
            Some("moderate fallback text"),
            StatusCode::OK,
            serde_json::json!({
                "id": "modr_fallback",
                "model": "__MODEL__",
                "results": [{
                    "flagged": false,
                    "route": "fallback"
                }]
            }),
        ),
        MockRoute::json(
            Method::POST,
            "/v1/rerank",
            Some("Bearer sk-fallback"),
            Some("rerank fallback query"),
            StatusCode::OK,
            serde_json::json!({
                "results": [{
                    "index": 0,
                    "relevance_score": 0.88,
                    "route": "fallback"
                }]
            }),
        ),
    ])
    .await;
    let harness =
        TestHarness::model_passthrough_affinity_fixture(&primary.base_url, &fallback.base_url)
            .await;
    primary.replace_placeholder("__MODEL__", &harness.model_name);
    fallback.replace_placeholder("__MODEL__", &harness.model_name);

    let completions_primary_payload = response_json(
        harness
            .json_request(
                Method::POST,
                "/v1/completions",
                "model-passthrough-completions-primary",
                serde_json::json!({
                    "model": harness.model_name,
                    "prompt": "tell me a joke"
                }),
            )
            .await,
    )
    .await;
    assert_eq!(completions_primary_payload["choices"][0]["text"], "primary");

    harness
        .promote_fallback_for_scopes(&["completions", "images", "audio", "moderations", "rerank"])
        .await;

    let completions_fallback_payload = response_json(
        harness
            .json_request(
                Method::POST,
                "/v1/completions",
                "model-passthrough-completions-fallback",
                serde_json::json!({
                    "model": harness.model_name,
                    "prompt": "tell me a joke"
                }),
            )
            .await,
    )
    .await;
    assert_eq!(
        completions_fallback_payload["choices"][0]["text"],
        "fallback"
    );

    let image_generations_payload = response_json(
        harness
            .json_request(
                Method::POST,
                "/v1/images/generations",
                "model-passthrough-image-generations",
                serde_json::json!({
                    "model": harness.model_name,
                    "prompt": "draw a sunset"
                }),
            )
            .await,
    )
    .await;
    assert_eq!(image_generations_payload["data"][0]["route"], "fallback");

    let image_edits_payload = response_json(
        harness
            .multipart_request(MultipartRequestSpec {
                uri: "/v1/images/edits",
                request_id: "model-passthrough-image-edits",
                text_fields: &[("model", &harness.model_name)],
                file_field_name: "image",
                file_name: "edit-fallback.png",
                file_content_type: "image/png",
                file_bytes: b"fallback edit image",
            })
            .await,
    )
    .await;
    assert_eq!(image_edits_payload["data"][0]["route"], "fallback");

    let image_variations_payload = response_json(
        harness
            .multipart_request(MultipartRequestSpec {
                uri: "/v1/images/variations",
                request_id: "model-passthrough-image-variations",
                text_fields: &[("model", &harness.model_name)],
                file_field_name: "image",
                file_name: "variation-fallback.png",
                file_content_type: "image/png",
                file_bytes: b"fallback variation image",
            })
            .await,
    )
    .await;
    assert_eq!(image_variations_payload["data"][0]["route"], "fallback");

    let audio_transcriptions_payload = response_json(
        harness
            .multipart_request(MultipartRequestSpec {
                uri: "/v1/audio/transcriptions",
                request_id: "model-passthrough-audio-transcriptions",
                text_fields: &[("model", &harness.model_name)],
                file_field_name: "file",
                file_name: "voice-fallback.wav",
                file_content_type: "audio/wav",
                file_bytes: b"fallback audio bytes",
            })
            .await,
    )
    .await;
    assert_eq!(audio_transcriptions_payload["route"], "fallback");

    let audio_translations_payload = response_json(
        harness
            .multipart_request(MultipartRequestSpec {
                uri: "/v1/audio/translations",
                request_id: "model-passthrough-audio-translations",
                text_fields: &[("model", &harness.model_name)],
                file_field_name: "file",
                file_name: "voice-translation-fallback.wav",
                file_content_type: "audio/wav",
                file_bytes: b"fallback translation audio bytes",
            })
            .await,
    )
    .await;
    assert_eq!(audio_translations_payload["route"], "fallback");

    let audio_speech_payload = response_text(
        harness
            .json_request(
                Method::POST,
                "/v1/audio/speech",
                "model-passthrough-audio-speech",
                serde_json::json!({
                    "model": harness.model_name,
                    "input": "say hello from fallback",
                    "voice": "alloy"
                }),
            )
            .await,
    )
    .await;
    assert_eq!(audio_speech_payload, "fallback-audio");

    let moderations_payload = response_json(
        harness
            .json_request(
                Method::POST,
                "/v1/moderations",
                "model-passthrough-moderations",
                serde_json::json!({
                    "model": harness.model_name,
                    "input": "moderate fallback text"
                }),
            )
            .await,
    )
    .await;
    assert_eq!(moderations_payload["results"][0]["route"], "fallback");

    let rerank_payload = response_json(
        harness
            .json_request(
                Method::POST,
                "/v1/rerank",
                "model-passthrough-rerank",
                serde_json::json!({
                    "model": harness.model_name,
                    "query": "rerank fallback query",
                    "documents": ["a", "b"]
                }),
            )
            .await,
    )
    .await;
    assert_eq!(rerank_payload["results"][0]["route"], "fallback");

    assert_eq!(primary.hit_count("/v1/chat/completions"), 1);
    assert_eq!(fallback.hit_count("/v1/chat/completions"), 1);
    assert_eq!(fallback.hit_count("/v1/images/generations"), 1);
    assert_eq!(fallback.hit_count("/v1/images/edits"), 1);
    assert_eq!(fallback.hit_count("/v1/images/variations"), 1);
    assert_eq!(fallback.hit_count("/v1/audio/transcriptions"), 1);
    assert_eq!(fallback.hit_count("/v1/audio/translations"), 1);
    assert_eq!(fallback.hit_count("/v1/audio/speech"), 1);
    assert_eq!(fallback.hit_count("/v1/moderations"), 1);
    assert_eq!(fallback.hit_count("/v1/rerank"), 1);

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn image_generations_route_settles_fallback_usage_and_persists_log() {
    let primary = MockUpstreamServer::spawn(vec![
        MockRoute::json(
            Method::POST,
            "/v1/images/generations",
            Some("Bearer sk-primary"),
            Some("draw a sunset"),
            StatusCode::OK,
            serde_json::json!({
                "created": 1,
                "data": [{
                    "url": "https://primary.example/image.png",
                    "route": "primary"
                }]
            }),
        )
        .with_response_headers(vec![("x-request-id", "img-upstream-primary-123")]),
    ])
    .await;
    let harness =
        TestHarness::model_passthrough_affinity_fixture(&primary.base_url, "http://127.0.0.1:9")
            .await;
    let request_body = serde_json::json!({
        "model": harness.model_name,
        "prompt": "draw a sunset"
    });
    let expected_tokens = i64::from(estimate_json_tokens(&request_body));
    let request_id = format!(
        "model-passthrough-image-generations-accounting-{}",
        harness.model_name
    );

    let response = harness
        .json_request(
            Method::POST,
            "/v1/images/generations",
            &request_id,
            request_body,
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let upstream_request_id = response
        .headers()
        .get("x-upstream-request-id")
        .and_then(|value| value.to_str().ok())
        .expect("image generations upstream request id")
        .to_string();
    let payload = response_json(response).await;
    assert_eq!(payload["data"][0]["route"], "primary");
    assert_eq!(upstream_request_id, "img-upstream-primary-123");

    let token = harness.wait_for_token_used_quota(expected_tokens).await;
    assert_eq!(token.used_quota, expected_tokens);

    let log = harness.wait_for_log_by_request_id(&request_id).await;
    assert_eq!(log.endpoint, "images/generations");
    assert_eq!(log.request_format, "openai/images_generations");
    assert_eq!(log.requested_model, harness.model_name);
    assert_eq!(log.upstream_model, harness.model_name);
    assert_eq!(log.model_name, harness.model_name);
    assert_eq!(log.upstream_request_id, "img-upstream-primary-123");
    assert_eq!(log.prompt_tokens, expected_tokens as i32);
    assert_eq!(log.completion_tokens, 0);
    assert_eq!(log.total_tokens, expected_tokens as i32);
    assert_eq!(log.quota, expected_tokens);
    assert_eq!(log.status_code, 200);
    assert_eq!(log.status, LogStatus::Success);
    assert!(!log.is_stream);

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn image_generations_route_falls_back_after_primary_rate_limit() {
    let primary = MockUpstreamServer::spawn(vec![MockRoute::json(
        Method::POST,
        "/v1/images/generations",
        Some("Bearer sk-primary"),
        Some("draw a sunset"),
        StatusCode::TOO_MANY_REQUESTS,
        serde_json::json!({
            "error": {
                "message": "primary image rate limited",
                "type": "rate_limit_error"
            }
        }),
    )])
    .await;
    let fallback = MockUpstreamServer::spawn(vec![MockRoute::json(
        Method::POST,
        "/v1/images/generations",
        Some("Bearer sk-fallback"),
        Some("draw a sunset"),
        StatusCode::OK,
        serde_json::json!({
            "created": 1,
            "data": [{
                "url": "https://fallback.example/image.png",
                "route": "fallback"
            }]
        }),
    )])
    .await;
    let harness =
        TestHarness::model_passthrough_affinity_fixture(&primary.base_url, &fallback.base_url)
            .await;
    let request_body = serde_json::json!({
        "model": harness.model_name,
        "prompt": "draw a sunset"
    });
    let expected_tokens = i64::from(estimate_json_tokens(&request_body));
    let request_id = format!("image-generations-fallback-{}", harness.model_name);

    let response = harness
        .json_request(
            Method::POST,
            "/v1/images/generations",
            &request_id,
            request_body,
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(payload["data"][0]["route"], "fallback");

    let token = harness.wait_for_token_used_quota(expected_tokens).await;
    assert_eq!(token.used_quota, expected_tokens);

    let log = harness.wait_for_log_by_request_id(&request_id).await;
    assert_eq!(log.endpoint, "images/generations");
    assert_eq!(log.request_format, "openai/images_generations");
    assert_eq!(log.requested_model, harness.model_name);
    assert_eq!(log.upstream_model, harness.model_name);
    assert_eq!(log.total_tokens, expected_tokens as i32);
    assert_eq!(log.quota, expected_tokens);
    assert_eq!(log.status, LogStatus::Success);

    let primary_account = harness.wait_for_primary_account_rate_limited().await;
    assert_eq!(primary_account.failure_streak, 1);
    assert!(primary_account.rate_limited_until.is_some());

    let primary_channel = harness.primary_channel_model().await;
    assert_eq!(primary_channel.failure_streak, 1);
    assert_eq!(primary_channel.last_health_status, 3);

    assert_eq!(primary.hit_count("/v1/images/generations"), 1);
    assert_eq!(fallback.hit_count("/v1/images/generations"), 1);

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn image_generations_route_skips_rate_limited_primary_on_next_request() {
    let primary = MockUpstreamServer::spawn(vec![MockRoute::json(
        Method::POST,
        "/v1/images/generations",
        Some("Bearer sk-primary"),
        Some("draw a sunset"),
        StatusCode::TOO_MANY_REQUESTS,
        serde_json::json!({
            "error": {
                "message": "primary image rate limited",
                "type": "rate_limit_error"
            }
        }),
    )])
    .await;
    let fallback = MockUpstreamServer::spawn(vec![MockRoute::json(
        Method::POST,
        "/v1/images/generations",
        Some("Bearer sk-fallback"),
        Some("draw a sunset"),
        StatusCode::OK,
        serde_json::json!({
            "created": 1,
            "data": [{
                "url": "https://fallback.example/image.png",
                "route": "fallback"
            }]
        }),
    )])
    .await;
    let harness =
        TestHarness::model_passthrough_affinity_fixture(&primary.base_url, &fallback.base_url)
            .await;
    let request_body = serde_json::json!({
        "model": harness.model_name,
        "prompt": "draw a sunset"
    });
    let expected_tokens = i64::from(estimate_json_tokens(&request_body));
    let first_request_id = format!("image-generations-rate-limit-first-{}", harness.model_name);
    let second_request_id = format!("image-generations-rate-limit-second-{}", harness.model_name);

    let first_response = harness
        .json_request(
            Method::POST,
            "/v1/images/generations",
            &first_request_id,
            request_body.clone(),
        )
        .await;
    assert_eq!(first_response.status(), StatusCode::OK);
    let first_payload = response_json(first_response).await;
    assert_eq!(first_payload["data"][0]["route"], "fallback");

    let primary_account = harness.wait_for_primary_account_rate_limited().await;
    assert_eq!(primary_account.failure_streak, 1);
    assert!(primary_account.rate_limited_until.is_some());

    let second_response = harness
        .json_request(
            Method::POST,
            "/v1/images/generations",
            &second_request_id,
            request_body,
        )
        .await;
    assert_eq!(second_response.status(), StatusCode::OK);
    let second_payload = response_json(second_response).await;
    assert_eq!(second_payload["data"][0]["route"], "fallback");

    let token = harness.wait_for_token_used_quota(expected_tokens * 2).await;
    assert_eq!(token.used_quota, expected_tokens * 2);

    assert_eq!(primary.hit_count("/v1/images/generations"), 1);
    assert_eq!(fallback.hit_count("/v1/images/generations"), 2);

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn image_generations_route_quarantines_primary_account_after_auth_failure() {
    let primary = MockUpstreamServer::spawn(vec![MockRoute::json(
        Method::POST,
        "/v1/images/generations",
        Some("Bearer sk-primary"),
        Some("draw a sunset"),
        StatusCode::UNAUTHORIZED,
        serde_json::json!({
            "error": {
                "message": "invalid api key",
                "type": "authentication_error"
            }
        }),
    )])
    .await;
    let fallback = MockUpstreamServer::spawn(vec![MockRoute::json(
        Method::POST,
        "/v1/images/generations",
        Some("Bearer sk-fallback"),
        Some("draw a sunset"),
        StatusCode::OK,
        serde_json::json!({
            "created": 1,
            "data": [{
                "url": "https://fallback.example/image-auth.png",
                "route": "fallback"
            }]
        }),
    )])
    .await;
    let harness =
        TestHarness::model_passthrough_affinity_fixture(&primary.base_url, &fallback.base_url)
            .await;
    let request_body = serde_json::json!({
        "model": harness.model_name,
        "prompt": "draw a sunset"
    });
    let expected_tokens = i64::from(estimate_json_tokens(&request_body));
    let first_request_id = format!("image-generations-auth-first-{}", harness.model_name);
    let second_request_id = format!("image-generations-auth-second-{}", harness.model_name);

    let first_response = harness
        .json_request(
            Method::POST,
            "/v1/images/generations",
            &first_request_id,
            request_body.clone(),
        )
        .await;
    assert_eq!(first_response.status(), StatusCode::OK);
    let first_payload = response_json(first_response).await;
    assert_eq!(first_payload["data"][0]["route"], "fallback");

    let primary_account = harness.wait_for_primary_account_disabled().await;
    assert_eq!(primary_account.status, AccountStatus::Disabled);
    assert!(!primary_account.schedulable);
    assert_eq!(primary_account.failure_streak, 1);

    let primary_channel = harness.primary_channel_model().await;
    assert_eq!(primary_channel.failure_streak, 0);
    assert_eq!(primary_channel.last_health_status, 2);

    let second_response = harness
        .json_request(
            Method::POST,
            "/v1/images/generations",
            &second_request_id,
            request_body,
        )
        .await;
    assert_eq!(second_response.status(), StatusCode::OK);
    let second_payload = response_json(second_response).await;
    assert_eq!(second_payload["data"][0]["route"], "fallback");

    let token = harness.wait_for_token_used_quota(expected_tokens * 2).await;
    assert_eq!(token.used_quota, expected_tokens * 2);

    assert_eq!(primary.hit_count("/v1/images/generations"), 1);
    assert_eq!(fallback.hit_count("/v1/images/generations"), 2);

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn image_generations_route_falls_back_after_primary_overload() {
    let primary = MockUpstreamServer::spawn(vec![MockRoute::json(
        Method::POST,
        "/v1/images/generations",
        Some("Bearer sk-primary"),
        Some("draw a sunset"),
        StatusCode::SERVICE_UNAVAILABLE,
        serde_json::json!({
            "error": {
                "message": "primary image upstream overloaded",
                "type": "server_error"
            }
        }),
    )])
    .await;
    let fallback = MockUpstreamServer::spawn(vec![MockRoute::json(
        Method::POST,
        "/v1/images/generations",
        Some("Bearer sk-fallback"),
        Some("draw a sunset"),
        StatusCode::OK,
        serde_json::json!({
            "created": 1,
            "data": [{
                "url": "https://fallback.example/image-overload.png",
                "route": "fallback"
            }]
        }),
    )])
    .await;
    let harness =
        TestHarness::model_passthrough_affinity_fixture(&primary.base_url, &fallback.base_url)
            .await;
    let request_body = serde_json::json!({
        "model": harness.model_name,
        "prompt": "draw a sunset"
    });
    let expected_tokens = i64::from(estimate_json_tokens(&request_body));
    let request_id = format!("image-generations-overload-{}", harness.model_name);

    let response = harness
        .json_request(
            Method::POST,
            "/v1/images/generations",
            &request_id,
            request_body,
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(payload["data"][0]["route"], "fallback");

    let token = harness.wait_for_token_used_quota(expected_tokens).await;
    assert_eq!(token.used_quota, expected_tokens);

    let log = harness.wait_for_log_by_request_id(&request_id).await;
    assert_eq!(log.endpoint, "images/generations");
    assert_eq!(log.request_format, "openai/images_generations");
    assert_eq!(log.requested_model, harness.model_name);
    assert_eq!(log.upstream_model, harness.model_name);
    assert_eq!(log.total_tokens, expected_tokens as i32);
    assert_eq!(log.quota, expected_tokens);
    assert_eq!(log.status, LogStatus::Success);

    let primary_account = harness.wait_for_primary_account_overloaded().await;
    assert_eq!(primary_account.failure_streak, 1);
    assert!(primary_account.overload_until.is_some());
    assert!(primary_account.rate_limited_until.is_none());

    let primary_channel = harness.primary_channel_model().await;
    assert_eq!(primary_channel.failure_streak, 1);
    assert_eq!(primary_channel.last_health_status, 3);

    assert_eq!(primary.hit_count("/v1/images/generations"), 1);
    assert_eq!(fallback.hit_count("/v1/images/generations"), 1);

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn uploads_create_without_model_does_not_consume_quota_or_persist_usage_log() {
    let primary = MockUpstreamServer::spawn(vec![MockRoute::json(
        Method::POST,
        "/v1/uploads",
        Some("Bearer sk-primary"),
        Some("upload-boundary-primary.bin"),
        StatusCode::OK,
        serde_json::json!({
            "id": "upload_boundary_primary",
            "object": "upload",
            "status": "in_progress",
            "route": "primary"
        }),
    )])
    .await;
    let harness =
        TestHarness::uploads_affinity_fixture(&primary.base_url, "http://127.0.0.1:9").await;
    let request_id = format!("uploads-boundary-create-{}", harness.model_name);

    let response = harness
        .json_request(
            Method::POST,
            "/v1/uploads",
            &request_id,
            serde_json::json!({
                "filename": "upload-boundary-primary.bin",
                "purpose": "assistants",
                "bytes": 18,
                "mime_type": "application/octet-stream"
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(payload["id"], "upload_boundary_primary");
    assert_eq!(payload["route"], "primary");

    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    let token = harness.token_model().await;
    assert_eq!(token.used_quota, 0);
    harness.assert_no_log_by_request_id(&request_id).await;

    harness.cleanup().await;
}
