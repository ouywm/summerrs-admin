use super::*;
use crate::router::openai_passthrough::relay_stream::{
    model_from_json_body, usage_accounting_model,
};
use crate::router::openai_passthrough::resource::resource_affinity_lookup_keys;

#[test]
fn openai_passthrough_router_module_does_not_reexport_test_helpers() {
    let source = std::fs::read_to_string(format!(
        "{}/src/router/openai_passthrough/mod.rs",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("read router/openai_passthrough/mod.rs");

    assert!(
        !source.contains("pub(crate) use self::resource::resource_affinity_lookup_keys;"),
        "router/openai_passthrough/mod.rs should not re-export test helpers"
    );
    assert!(
        !source
            .contains("pub(crate) use self::support::detect_unusable_upstream_success_response;"),
        "router/openai_passthrough/mod.rs should not re-export support test helpers"
    );
}

#[test]
fn model_from_json_body_uses_default() {
    let body = serde_json::json!({"input": "hello"});
    assert_eq!(
        model_from_json_body(&body, Some("omni-moderation-latest")).as_deref(),
        Some("omni-moderation-latest")
    );
}

#[test]
fn model_from_json_body_prefers_explicit_model() {
    let body = serde_json::json!({
        "model": "gpt-5.4",
        "input": "hello"
    });
    assert_eq!(
        model_from_json_body(&body, Some("omni-moderation-latest")).as_deref(),
        Some("gpt-5.4")
    );
}

#[test]
fn json_body_requests_stream_detects_true_flag() {
    assert!(json_body_requests_stream(&serde_json::json!({
        "model": "gpt-5.4",
        "stream": true
    })));
    assert!(!json_body_requests_stream(&serde_json::json!({
        "model": "gpt-5.4"
    })));
}

#[test]
fn estimate_total_tokens_uses_max_output_tokens() {
    let body = serde_json::json!({
        "model": "gpt-5.4",
        "input": "hello",
        "max_output_tokens": 512
    });
    assert!(estimate_total_tokens_for_rate_limit(&body) >= 512);
}

#[test]
fn extract_usage_supports_chat_shape() {
    let usage = extract_usage_from_value(&serde_json::json!({
        "usage": {
            "prompt_tokens": 10,
            "completion_tokens": 20,
            "total_tokens": 30
        }
    }))
    .unwrap();
    assert_eq!(usage.total_tokens, 30);
}

#[test]
fn extract_usage_supports_responses_shape() {
    let usage = extract_usage_from_value(&serde_json::json!({
        "usage": {
            "input_tokens": 11,
            "output_tokens": 22,
            "total_tokens": 33,
            "input_tokens_details": {"cached_tokens": 4},
            "output_tokens_details": {"reasoning_tokens": 5}
        }
    }))
    .unwrap();
    assert_eq!(usage.prompt_tokens, 11);
    assert_eq!(usage.cached_tokens, 4);
    assert_eq!(usage.reasoning_tokens, 5);
}

#[test]
fn extract_model_from_response_value_supports_nested_response() {
    let payload = serde_json::json!({
        "type": "response.completed",
        "response": {
            "id": "resp_123",
            "model": "gpt-5.4"
        }
    });
    assert_eq!(
        extract_model_from_response_value(&payload).as_deref(),
        Some("gpt-5.4")
    );
}

#[test]
fn generic_stream_tracker_collects_resource_refs() {
    let body = Bytes::from_static(
            br#"data: {"id":"run_123","thread_id":"thread_123","assistant_id":"asst_123","usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}

"#,
        );
    let start = std::time::Instant::now();
    let mut first_token_time = None;
    let mut tracker = GenericStreamTracker::default();

    tracker.ingest(&body, &start, &mut first_token_time);

    assert_eq!(tracker.resource_id, "run_123");
    assert!(
        tracker
            .resource_refs
            .contains(&("thread", "thread_123".to_string()))
    );
    assert!(
        tracker
            .resource_refs
            .contains(&("assistant", "asst_123".to_string()))
    );
}

#[test]
fn referenced_resource_ids_extract_known_fields() {
    use crate::router::openai_passthrough::resource::referenced_resource_ids;

    let body = serde_json::json!({
        "assistant_id": "asst_123",
        "file_id": "file_123",
        "previous_response_id": "resp_123"
    });

    let refs = referenced_resource_ids(&body);
    assert!(refs.contains(&("assistant", "asst_123".to_string())));
    assert!(refs.contains(&("file", "file_123".to_string())));
    assert!(refs.contains(&("response", "resp_123".to_string())));
}

#[test]
fn referenced_resource_ids_extract_nested_resource_fields() {
    let body = serde_json::json!({
        "input_file_id": "file_input",
        "tool_resources": {
            "code_interpreter": {
                "file_ids": ["file_a", "file_b"]
            },
            "file_search": {
                "vector_store_ids": ["vs_1", "vs_2"]
            }
        }
    });

    let refs = referenced_resource_ids(&body);
    assert!(refs.contains(&("file", "file_input".to_string())));
    assert!(refs.contains(&("file", "file_a".to_string())));
    assert!(refs.contains(&("file", "file_b".to_string())));
    assert!(refs.contains(&("vector_store", "vs_1".to_string())));
    assert!(refs.contains(&("vector_store", "vs_2".to_string())));
}

#[test]
fn resource_affinity_lookup_keys_keeps_explicit_keys_first() {
    let body = serde_json::json!({
        "assistant_id": "asst_123",
        "thread_id": "thread_123",
        "run_id": "run_123"
    });

    let keys = resource_affinity_lookup_keys(
        &[("run", "run_123".into()), ("thread", "thread_123".into())],
        Some(&body),
    );

    assert_eq!(keys[0], ("run", "run_123".into()));
    assert_eq!(keys[1], ("thread", "thread_123".into()));
    assert!(keys.contains(&("assistant", "asst_123".into())));
}

#[test]
fn resource_affinity_lookup_keys_deduplicates_exact_duplicates() {
    let body = serde_json::json!({
        "thread_id": "thread_123",
        "run_id": "run_123"
    });

    let keys = resource_affinity_lookup_keys(
        &[("thread", "thread_123".into()), ("run", "run_123".into())],
        Some(&body),
    );

    assert_eq!(keys.len(), 2);
    assert_eq!(keys[0], ("thread", "thread_123".into()));
    assert_eq!(keys[1], ("run", "run_123".into()));
}

#[test]
fn resource_affinity_lookup_keys_covers_vector_store_file_chain() {
    let body = serde_json::json!({
        "vector_store_id": "vs_123",
        "file_id": "file_123"
    });

    let keys = resource_affinity_lookup_keys(&[("vector_store", "vs_123".into())], Some(&body));

    assert_eq!(keys[0], ("vector_store", "vs_123".into()));
    assert!(keys.contains(&("file", "file_123".into())));
}

#[test]
fn resource_affinity_lookup_keys_covers_response_chain() {
    let body = serde_json::json!({
        "response_id": "resp_current",
        "previous_response_id": "resp_prev"
    });

    let keys = resource_affinity_lookup_keys(&[], Some(&body));

    assert_eq!(keys[0], ("response", "resp_current".into()));
    assert_eq!(keys[1], ("response", "resp_prev".into()));
}

#[test]
fn resource_affinity_lookup_keys_prefers_thread_path_over_assistant_body_reference() {
    let body = serde_json::json!({
        "assistant_id": "asst_123"
    });

    let keys = resource_affinity_lookup_keys(&[("thread", "thread_123".into())], Some(&body));

    assert_eq!(keys[0], ("thread", "thread_123".into()));
    assert_eq!(keys[1], ("assistant", "asst_123".into()));
}

#[test]
fn resource_affinity_lookup_keys_prefers_run_then_thread_for_submit_tool_outputs() {
    let keys = resource_affinity_lookup_keys(
        &[("run", "run_123".into()), ("thread", "thread_123".into())],
        None,
    );

    assert_eq!(keys[0], ("run", "run_123".into()));
    assert_eq!(keys[1], ("thread", "thread_123".into()));
}

#[test]
fn resource_affinity_lookup_keys_prefers_file_before_vector_store_for_nested_file_routes() {
    let keys = resource_affinity_lookup_keys(
        &[
            ("file", "file_123".into()),
            ("vector_store", "vs_123".into()),
        ],
        None,
    );

    assert_eq!(keys[0], ("file", "file_123".into()));
    assert_eq!(keys[1], ("vector_store", "vs_123".into()));
}

#[test]
fn resource_affinity_lookup_keys_appends_nested_tool_resources_after_explicit_chain_keys() {
    let body = serde_json::json!({
        "tool_resources": {
            "code_interpreter": {
                "file_ids": ["file_a"]
            },
            "file_search": {
                "vector_store_ids": ["vs_1"]
            }
        }
    });

    let keys = resource_affinity_lookup_keys(
        &[("run", "run_123".into()), ("thread", "thread_123".into())],
        Some(&body),
    );

    assert_eq!(keys[0], ("run", "run_123".into()));
    assert_eq!(keys[1], ("thread", "thread_123".into()));
    assert!(keys.contains(&("file", "file_a".into())));
    assert!(keys.contains(&("vector_store", "vs_1".into())));
}

#[test]
fn usage_accounting_model_falls_back_to_upstream_model() {
    assert_eq!(
        usage_accounting_model(None, "gpt-5.4"),
        Some("gpt-5.4".to_string())
    );
}

#[test]
fn usage_accounting_model_prefers_requested_model() {
    assert_eq!(
        usage_accounting_model(Some("gpt-5.4 xhigh"), "gpt-5.4"),
        Some("gpt-5.4 xhigh".to_string())
    );
}

#[test]
fn usage_accounting_model_returns_none_when_both_inputs_are_blank() {
    assert_eq!(usage_accounting_model(Some("   "), " "), None);
}

#[test]
fn build_upstream_url_preserves_query_string() {
    use crate::router::openai_passthrough::support::build_upstream_url;

    assert_eq!(
        build_upstream_url(
            "https://example.com/",
            "/v1/files",
            Some("limit=20&after=file_123")
        ),
        "https://example.com/v1/files?limit=20&after=file_123"
    );
}

#[test]
fn build_upstream_url_avoids_duplicate_v1_for_azure_openai_base() {
    use crate::router::openai_passthrough::support::build_upstream_url;

    assert_eq!(
        build_upstream_url(
            "https://example-resource.openai.azure.com/openai/v1/",
            "/v1/models",
            Some("api-version=preview")
        ),
        "https://example-resource.openai.azure.com/openai/v1/models?api-version=preview"
    );
}

#[test]
fn apply_upstream_auth_uses_api_key_for_azure_channels() {
    use crate::router::openai_passthrough::support::apply_upstream_auth;

    let request = apply_upstream_auth(
        reqwest::Client::new().get("https://example-resource.openai.azure.com/openai/v1/models"),
        14,
        "azure-key",
    )
    .build()
    .expect("build request");

    assert_eq!(
        request
            .headers()
            .get("api-key")
            .and_then(|value| value.to_str().ok()),
        Some("azure-key")
    );
    assert!(request.headers().get("authorization").is_none());
}

#[test]
fn should_forward_header_filters_sensitive_headers() {
    use crate::router::openai_passthrough::support::should_forward_header;

    assert!(!should_forward_header(&header::AUTHORIZATION, false));
    assert!(!should_forward_header(&header::CONTENT_LENGTH, false));
    assert!(should_forward_header(
        &header::HeaderName::from_static("x-request-id"),
        false
    ));
    assert!(should_forward_header(
        &header::HeaderName::from_static("openai-beta"),
        false
    ));
}

#[test]
fn payload_has_text_delta_for_chat_chunk_and_responses_event() {
    assert!(payload_has_text_delta(&serde_json::json!({
        "choices": [{
            "delta": {"content": "hello"}
        }]
    })));
    assert!(payload_has_text_delta(&serde_json::json!({
        "type": "response.output_text.delta",
        "delta": "world"
    })));
}

#[test]
fn detect_unusable_upstream_success_response_returns_message() {
    let payload = serde_json::json!({
        "error": {
            "message": "endpoint disabled",
            "code": "unsupported_endpoint"
        }
    });
    assert_eq!(
        detect_unusable_upstream_success_response(&payload).as_deref(),
        Some("endpoint disabled")
    );
}

#[test]
fn detect_unusable_upstream_success_response_prefers_code_when_message_missing() {
    let payload = serde_json::json!({
        "error": {
            "code": "unsupported_endpoint"
        }
    });
    assert_eq!(
        detect_unusable_upstream_success_response(&payload).as_deref(),
        Some("unsupported_endpoint")
    );
}

#[test]
fn detect_unusable_upstream_success_response_ignores_missing_error() {
    let payload = serde_json::json!({
        "result": "ok"
    });
    assert!(detect_unusable_upstream_success_response(&payload).is_none());
}

#[test]
fn unusable_success_response_message_flags_empty_body() {
    let body = Bytes::from_static(b"   ");
    assert_eq!(
        unusable_success_response_message(StatusCode::OK, &body, "responses", false,).as_deref(),
        Some("upstream returned an empty success response for endpoint responses")
    );
}

#[test]
fn unusable_success_response_message_allows_empty_body_when_configured() {
    let body = Bytes::from_static(b"   ");
    assert!(
        unusable_success_response_message(StatusCode::OK, &body, "files/content", true).is_none()
    );
}
