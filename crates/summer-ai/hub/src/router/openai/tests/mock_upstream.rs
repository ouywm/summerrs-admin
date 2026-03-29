use std::sync::{Arc, Mutex};

use super::super::*;
use crate::router::test_support::TestHarness;
use summer_ai_model::entity::log::LogStatus;
use summer_web::axum::{
    Router,
    body::{Body, to_bytes},
    extract::{Request, State},
    http::Method,
    http::header::CONTENT_TYPE,
    response::IntoResponse,
};
use tokio::sync::oneshot;

#[derive(Clone)]
struct MockUpstreamSpec {
    expected_path_and_query: String,
    expected_header_name: String,
    expected_header_value: String,
    expected_body_substring: Option<String>,
    additional_expected_headers: Vec<(String, String)>,
    additional_expected_body_substrings: Vec<String>,
    response_status: StatusCode,
    response_content_type: String,
    response_headers: Vec<(String, String)>,
    response_body: String,
}

struct MockUpstreamServer {
    base_url: String,
    hits: Arc<Mutex<Vec<String>>>,
    shutdown_tx: Option<oneshot::Sender<()>>,
    _task: tokio::task::JoinHandle<()>,
}

struct MockUpstreamState {
    spec: MockUpstreamSpec,
    hits: Arc<Mutex<Vec<String>>>,
}

impl MockUpstreamServer {
    fn hit_count(&self, path_and_query: &str) -> usize {
        self.hits
            .lock()
            .expect("mock upstream hits mutex")
            .iter()
            .filter(|hit| hit.as_str() == path_and_query)
            .count()
    }
}

impl Drop for MockUpstreamServer {
    fn drop(&mut self) {
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(());
        }
    }
}

async fn mock_upstream_handler(
    State(state): State<Arc<MockUpstreamState>>,
    req: Request,
) -> summer_web::axum::response::Response {
    let spec = &state.spec;
    let path_and_query = req
        .uri()
        .path_and_query()
        .map(|value| value.as_str().to_string())
        .unwrap_or_else(|| req.uri().path().to_string());
    state
        .hits
        .lock()
        .expect("mock upstream hits mutex")
        .push(path_and_query.clone());
    if path_and_query != spec.expected_path_and_query {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("unexpected path: {path_and_query}"),
        )
            .into_response();
    }

    let header_value = req
        .headers()
        .get(&spec.expected_header_name)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    if header_value != spec.expected_header_value {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!(
                "unexpected header {}: {}",
                spec.expected_header_name, header_value
            ),
        )
            .into_response();
    }

    for (header_name, expected_header_value) in &spec.additional_expected_headers {
        let header_value = req
            .headers()
            .get(header_name)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();
        if &header_value != expected_header_value {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("unexpected header {header_name}: {header_value}"),
            )
                .into_response();
        }
    }

    let request_body = if spec.expected_body_substring.is_some()
        || !spec.additional_expected_body_substrings.is_empty()
    {
        let body = to_bytes(req.into_body(), usize::MAX)
            .await
            .expect("request body");
        Some(String::from_utf8_lossy(&body).to_string())
    } else {
        None
    };

    if let Some(expected_body_substring) = spec.expected_body_substring.as_ref() {
        let body = request_body.as_deref().unwrap_or_default();
        if !body.contains(expected_body_substring) {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("unexpected body: {body}"),
            )
                .into_response();
        }
    }

    if spec.additional_expected_body_substrings.is_empty() && spec.response_headers.is_empty() {
        return summer_web::axum::http::Response::builder()
            .status(spec.response_status)
            .header(CONTENT_TYPE, spec.response_content_type.as_str())
            .body(Body::from(spec.response_body.clone()))
            .expect("mock upstream response");
    }

    let body = request_body.as_deref().unwrap_or_default();
    for expected_body_substring in &spec.additional_expected_body_substrings {
        if !body.contains(expected_body_substring) {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("unexpected body: {body}"),
            )
                .into_response();
        }
    }

    let mut builder = summer_web::axum::http::Response::builder()
        .status(spec.response_status)
        .header(CONTENT_TYPE, spec.response_content_type.as_str());
    for (header_name, header_value) in &spec.response_headers {
        builder = builder.header(header_name, header_value);
    }

    builder
        .body(Body::from(spec.response_body.clone()))
        .expect("mock upstream response")
}

async fn spawn_mock_upstream(spec: MockUpstreamSpec) -> MockUpstreamServer {
    let hits = Arc::new(Mutex::new(Vec::new()));
    let state = Arc::new(MockUpstreamState {
        spec,
        hits: hits.clone(),
    });
    let router = Router::new()
        .fallback(mock_upstream_handler)
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock upstream");
    let addr = listener.local_addr().expect("local addr");
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        let _ = summer_web::axum::serve(listener, router)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.await;
            })
            .await;
    });

    MockUpstreamServer {
        base_url: format!("http://{addr}"),
        hits,
        shutdown_tx: Some(shutdown_tx),
        _task: task,
    }
}

fn sample_mock_chat_request(stream: bool) -> ChatCompletionRequest {
    serde_json::from_value(serde_json::json!({
        "model": "gpt-5.4 xhigh",
        "messages": [{"role": "user", "content": "Hello"}],
        "stream": stream
    }))
    .expect("sample chat request")
}

fn sample_mock_responses_request(stream: bool) -> ResponsesRequest {
    serde_json::from_value(serde_json::json!({
        "model": "gpt-5.4 xhigh",
        "input": "Hello",
        "stream": stream
    }))
    .expect("sample responses request")
}

fn sample_mock_embeddings_request() -> EmbeddingRequest {
    serde_json::from_value(serde_json::json!({
        "model": "text-embedding-3-large",
        "input": "hello"
    }))
    .expect("sample embeddings request")
}

fn extract_responses_event(body: &str, event_type: &str) -> serde_json::Value {
    body.lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find(|event| event.get("type").and_then(serde_json::Value::as_str) == Some(event_type))
        .unwrap_or_else(|| panic!("missing responses event {event_type} in body: {body}"))
}

async fn send_mock_chat_request(
    channel_type: i16,
    api_key: &str,
    req: &ChatCompletionRequest,
    actual_model: &str,
    spec: MockUpstreamSpec,
) -> (MockUpstreamServer, reqwest::Response) {
    let server = spawn_mock_upstream(spec).await;
    let client = reqwest::Client::new();
    let request_builder = get_adapter(channel_type)
        .build_request(&client, &server.base_url, api_key, req, actual_model)
        .expect("build request");
    let response = request_builder.send().await.expect("send request");
    (server, response)
}

async fn send_mock_responses_request(
    channel_type: i16,
    api_key: &str,
    req: &ResponsesRequest,
    actual_model: &str,
    spec: MockUpstreamSpec,
) -> (MockUpstreamServer, reqwest::Response) {
    let server = spawn_mock_upstream(spec).await;
    let response = send_mock_responses_request_to_base_url(
        channel_type,
        &server.base_url,
        api_key,
        req,
        actual_model,
    )
    .await;
    (server, response)
}

async fn send_mock_responses_request_to_base_url(
    channel_type: i16,
    base_url: &str,
    api_key: &str,
    req: &ResponsesRequest,
    actual_model: &str,
) -> reqwest::Response {
    let client = reqwest::Client::new();
    let raw_request = serde_json::to_value(req).expect("responses request json");
    let request_builder = get_adapter(channel_type)
        .build_responses_request(&client, base_url, api_key, &raw_request, actual_model)
        .expect("build responses request");
    request_builder
        .send()
        .await
        .expect("send responses request")
}

async fn send_mock_embeddings_request(
    channel_type: i16,
    api_key: &str,
    req: &EmbeddingRequest,
    actual_model: &str,
    spec: MockUpstreamSpec,
) -> (MockUpstreamServer, reqwest::Response) {
    let server = spawn_mock_upstream(spec).await;
    let client = reqwest::Client::new();
    let raw_request = serde_json::to_value(req).expect("embeddings request json");
    let request_builder = get_adapter(channel_type)
        .build_embeddings_request(
            &client,
            &server.base_url,
            api_key,
            &raw_request,
            actual_model,
        )
        .expect("build embeddings request");
    let response = request_builder
        .send()
        .await
        .expect("send embeddings request");
    (server, response)
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

#[tokio::test]
async fn gemini_chat_non_stream_mock_upstream_converts_file_uri_image_to_file_data() {
    let req: ChatCompletionRequest = serde_json::from_value(serde_json::json!({
        "model": "gpt-5.4 xhigh",
        "messages": [{
            "role": "user",
            "content": [{
                "type": "image_url",
                "image_url": {
                    "url": "https://generativelanguage.googleapis.com/v1beta/files/file-123",
                    "mime_type": "image/png"
                }
            }]
        }]
    }))
    .expect("gemini file uri request");
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
                "\"fileData\"".into(),
                "\"mimeType\":\"image/png\"".into(),
                "\"fileUri\":\"https://generativelanguage.googleapis.com/v1beta/files/file-123\""
                    .into(),
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
async fn azure_legacy_chat_non_stream_mock_upstream_success() {
    let req = sample_mock_chat_request(false);
    let actual_model = "gpt-4o-deployment";
    let (_server, response) = send_mock_chat_request(
        14,
        "azure-key",
        &req,
        actual_model,
        MockUpstreamSpec {
            expected_path_and_query: format!(
                "/openai/deployments/{actual_model}/chat/completions?api-version=2024-10-21"
            ),
            expected_header_name: "api-key".into(),
            expected_header_value: "azure-key".into(),
            expected_body_substring: None,
            additional_expected_headers: vec![],
            additional_expected_body_substrings: vec![],
            response_status: StatusCode::OK,
            response_content_type: "application/json".into(),
            response_headers: vec![],
            response_body: serde_json::json!({
                "id": "chatcmpl_azure_123",
                "object": "chat.completion",
                "created": 1_774_277_000,
                "model": actual_model,
                "choices": [{
                    "index": 0,
                    "message": {"role": "assistant", "content": "Hello from Azure"},
                    "finish_reason": "stop"
                }],
                "usage": {
                    "prompt_tokens": 12,
                    "completion_tokens": 7,
                    "total_tokens": 19
                }
            })
            .to_string(),
        },
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let parsed = get_adapter(14)
        .parse_response(response.bytes().await.expect("body"), actual_model)
        .expect("parse azure response");
    assert_eq!(parsed.model, actual_model);
    assert_eq!(
        parsed.choices[0].message.content,
        serde_json::json!("Hello from Azure")
    );
    assert_eq!(parsed.usage.total_tokens, 19);
}

#[tokio::test]
async fn azure_v1_responses_non_stream_mock_upstream_success() {
    let req = sample_mock_responses_request(false);
    let actual_model = "gpt-4.1-deployment";
    let server = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: "/openai/v1/responses".into(),
        expected_header_name: "api-key".into(),
        expected_header_value: "azure-key".into(),
        expected_body_substring: Some(format!("\"model\":\"{actual_model}\"")),
        additional_expected_headers: vec![],
        additional_expected_body_substrings: vec![],
        response_status: StatusCode::OK,
        response_content_type: "application/json".into(),
        response_headers: vec![],
        response_body: serde_json::json!({
            "id": "resp_azure_123",
            "object": "response",
            "model": actual_model,
            "status": "completed",
            "usage": {
                "input_tokens": 12,
                "output_tokens": 7,
                "total_tokens": 19
            },
            "output_text": "hello from azure responses"
        })
        .to_string(),
    })
    .await;

    let base_url = format!("{}/openai/v1", server.base_url);
    let response =
        send_mock_responses_request_to_base_url(14, &base_url, "azure-key", &req, actual_model)
            .await;

    assert_eq!(response.status(), StatusCode::OK);
    let parsed: ResponsesResponse =
        serde_json::from_slice(&response.bytes().await.expect("body")).expect("responses json");
    assert_eq!(parsed.id, "resp_azure_123");
    assert_eq!(parsed.model, actual_model);
    assert_eq!(
        parsed.usage.as_ref().map(|usage| usage.total_tokens),
        Some(19)
    );
}

#[tokio::test]
async fn azure_legacy_embeddings_non_stream_mock_upstream_success() {
    let req = sample_mock_embeddings_request();
    let actual_model = "text-embedding-3-large-deployment";
    let (_server, response) = send_mock_embeddings_request(
        14,
        "azure-key",
        &req,
        actual_model,
        MockUpstreamSpec {
            expected_path_and_query: format!(
                "/openai/deployments/{actual_model}/embeddings?api-version=2024-10-21"
            ),
            expected_header_name: "api-key".into(),
            expected_header_value: "azure-key".into(),
            expected_body_substring: Some("\"input\":\"hello\"".into()),
            additional_expected_headers: vec![],
            additional_expected_body_substrings: vec![],
            response_status: StatusCode::OK,
            response_content_type: "application/json".into(),
            response_headers: vec![],
            response_body: serde_json::json!({
                "object": "list",
                "data": [{
                    "object": "embedding",
                    "index": 0,
                    "embedding": [0.1, 0.2]
                }],
                "usage": {
                    "prompt_tokens": 8,
                    "completion_tokens": 0,
                    "total_tokens": 8
                }
            })
            .to_string(),
        },
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let parsed: EmbeddingResponse =
        serde_json::from_slice(&response.bytes().await.expect("body")).expect("embeddings json");
    assert_eq!(parsed.data.len(), 1);
    assert_eq!(parsed.usage.total_tokens, 8);
}

#[tokio::test]
async fn responses_non_stream_mock_upstream_success() {
    let req = sample_mock_responses_request(false);
    let actual_model = "gpt-5.4-mini";
    let (_server, response) = send_mock_responses_request(
        1,
        "sk-openai-test",
        &req,
        actual_model,
        MockUpstreamSpec {
            expected_path_and_query: "/v1/responses".into(),
            expected_header_name: "authorization".into(),
            expected_header_value: "Bearer sk-openai-test".into(),
            expected_body_substring: Some(format!("\"model\":\"{actual_model}\"")),
            additional_expected_headers: vec![],
            additional_expected_body_substrings: vec![],
            response_status: StatusCode::OK,
            response_content_type: "application/json".into(),
            response_headers: vec![],
            response_body: serde_json::json!({
                "id": "resp_123",
                "object": "response",
                "model": actual_model,
                "status": "completed",
                "usage": {
                    "input_tokens": 12,
                    "output_tokens": 7,
                    "total_tokens": 19
                },
                "output_text": "hello"
            })
            .to_string(),
        },
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let parsed: ResponsesResponse =
        serde_json::from_slice(&response.bytes().await.expect("body")).expect("responses json");
    assert_eq!(parsed.id, "resp_123");
    assert_eq!(parsed.model, actual_model);
    assert_eq!(
        parsed.usage.as_ref().map(|usage| usage.total_tokens),
        Some(19)
    );
}

#[tokio::test]
async fn responses_stream_tracker_parses_completed_event_from_mock_upstream() {
    let req = sample_mock_responses_request(true);
    let actual_model = "gpt-5.4-mini";
    let (_server, response) = send_mock_responses_request(
        1,
        "sk-openai-test",
        &req,
        actual_model,
        MockUpstreamSpec {
            expected_path_and_query: "/v1/responses".into(),
            expected_header_name: "authorization".into(),
            expected_header_value: "Bearer sk-openai-test".into(),
            expected_body_substring: Some(format!("\"model\":\"{actual_model}\"")),
            additional_expected_headers: vec![],
            additional_expected_body_substrings: vec![],
            response_status: StatusCode::OK,
            response_content_type: "text/event-stream".into(),
            response_headers: vec![],
            response_body: concat!(
                "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_123\",\"model\":\"gpt-5.4-mini\"}}\n\n",
                "data: {\"type\":\"response.output_text.delta\",\"delta\":\"hel\"}\n\n",
                "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_123\",\"model\":\"gpt-5.4-mini\",\"usage\":{\"input_tokens\":12,\"output_tokens\":7,\"total_tokens\":19}}}\n\n",
                "data: [DONE]\n\n"
            )
            .into(),
        },
    )
    .await;

    let body = response.bytes().await.expect("body");
    let mut tracker = ResponsesStreamTracker::default();
    let start = std::time::Instant::now();
    let mut first_token_time = None;
    tracker.ingest(&body, &start, &mut first_token_time);

    assert_eq!(tracker.response_id, "resp_123");
    assert_eq!(tracker.upstream_model, actual_model);
    assert_eq!(
        tracker.usage.as_ref().map(|usage| usage.total_tokens),
        Some(19)
    );
    assert!(first_token_time.is_some());
}

#[tokio::test]
async fn responses_mock_upstream_provider_failure() {
    let req = sample_mock_responses_request(false);
    let actual_model = "gpt-5.4-mini";
    let (_server, response) = send_mock_responses_request(
        1,
        "sk-openai-test",
        &req,
        actual_model,
        MockUpstreamSpec {
            expected_path_and_query: "/v1/responses".into(),
            expected_header_name: "authorization".into(),
            expected_header_value: "Bearer sk-openai-test".into(),
            expected_body_substring: Some(format!("\"model\":\"{actual_model}\"")),
            additional_expected_headers: vec![],
            additional_expected_body_substrings: vec![],
            response_status: StatusCode::TOO_MANY_REQUESTS,
            response_content_type: "application/json".into(),
            response_headers: vec![],
            response_body: r#"{"error":{"message":"slow down","type":"rate_limit_error","code":"rate_limit_error"}}"#
                .to_string(),
        },
    )
    .await;

    let status = response.status();
    let headers = response.headers().clone();
    let body = response.bytes().await.expect("body");
    let failure = classify_upstream_provider_failure(1, status, &headers, &body);
    assert_eq!(failure.scope, UpstreamFailureScope::Account);
    assert_eq!(failure.error.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(failure.error.error.error.r#type, "rate_limit_error");
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn anthropic_responses_route_bridges_non_stream_chat_response() {
    let actual_model = "claude-sonnet-4-20250514";
    let primary = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: "/v1/messages".into(),
        expected_header_name: "x-api-key".into(),
        expected_header_value: "sk-primary".into(),
        expected_body_substring: Some(format!("\"model\":\"{actual_model}\"")),
        additional_expected_headers: vec![("anthropic-version".into(), "2023-06-01".into())],
        additional_expected_body_substrings: vec!["\"text\":\"Hello\"".into()],
        response_status: StatusCode::OK,
        response_content_type: "application/json".into(),
        response_headers: vec![(
            "anthropic-request-id".into(),
            "anthropic-upstream-responses-123".into(),
        )],
        response_body: serde_json::json!({
            "id": "msg_resp_123",
            "model": actual_model,
            "content": [{"type": "text", "text": "Hello from Claude responses bridge"}],
            "stop_reason": "end_turn",
            "usage": {
                "input_tokens": 12,
                "output_tokens": 7
            }
        })
        .to_string(),
    })
    .await;
    let harness =
        TestHarness::anthropic_responses_affinity_fixture(&primary.base_url, "http://127.0.0.1:9")
            .await;
    let request_id = format!("anthropic-responses-bridge-{}", harness.model_name);

    let response = harness
        .json_request(
            Method::POST,
            "/v1/responses",
            &request_id,
            serde_json::json!({
                "model": harness.model_name,
                "input": "Hello",
                "stream": false
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let upstream_request_id = response
        .headers()
        .get("x-upstream-request-id")
        .and_then(|value| value.to_str().ok())
        .expect("anthropic upstream request id")
        .to_string();
    let payload = crate::router::test_support::response_json(response).await;

    assert_eq!(payload["object"], "response");
    assert_eq!(payload["id"], "msg_resp_123");
    assert_eq!(payload["model"], actual_model);
    assert_eq!(payload["output_text"], "Hello from Claude responses bridge");
    assert_eq!(payload["usage"]["total_tokens"], 19);
    assert_eq!(upstream_request_id, "anthropic-upstream-responses-123");

    let token = harness.wait_for_token_used_quota(19).await;
    assert_eq!(token.used_quota, 19);

    let log = harness.wait_for_log_by_request_id(&request_id).await;
    assert_eq!(log.endpoint, "responses");
    assert_eq!(log.request_format, "openai/responses");
    assert_eq!(log.requested_model, harness.model_name);
    assert_eq!(log.upstream_model, actual_model);
    assert_eq!(log.model_name, harness.model_name);
    assert_eq!(log.total_tokens, 19);
    assert_eq!(log.quota, 19);
    assert_eq!(log.status_code, 200);
    assert_eq!(log.upstream_request_id, "anthropic-upstream-responses-123");
    assert_eq!(log.status, LogStatus::Success);
    assert!(!log.is_stream);

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn gemini_responses_route_bridges_stream_to_response_events() {
    let actual_model = "gemini-2.5-pro";
    let primary = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: format!(
            "/v1beta/models/{actual_model}:streamGenerateContent?alt=sse"
        ),
        expected_header_name: "x-goog-api-key".into(),
        expected_header_value: "sk-primary".into(),
        expected_body_substring: Some("\"contents\"".into()),
        additional_expected_headers: vec![],
        additional_expected_body_substrings: vec!["\"text\":\"Hello\"".into()],
        response_status: StatusCode::OK,
        response_content_type: "text/event-stream".into(),
        response_headers: vec![("x-request-id".into(), "gemini-upstream-stream-123".into())],
        response_body: concat!(
            "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"Hello\"}]}}]}\n\n",
            "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\" world\"}]},\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":12,\"candidatesTokenCount\":7,\"totalTokenCount\":19}}\n\n"
        )
        .into(),
    })
    .await;
    let harness =
        TestHarness::gemini_responses_affinity_fixture(&primary.base_url, "http://127.0.0.1:9")
            .await;
    let request_id = format!("gemini-responses-bridge-stream-{}", harness.model_name);

    let response = harness
        .json_request(
            Method::POST,
            "/v1/responses",
            &request_id,
            serde_json::json!({
                "model": harness.model_name,
                "input": "Hello",
                "stream": true
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = crate::router::test_support::response_text(response).await;

    assert!(body.contains("\"type\":\"response.created\""));
    assert!(body.contains("\"type\":\"response.output_text.delta\""));
    assert!(body.contains("\"type\":\"response.completed\""));
    assert!(body.contains("\"total_tokens\":19"));
    assert!(body.contains("Hello world"));

    let token = harness.wait_for_token_used_quota(19).await;
    assert_eq!(token.used_quota, 19);

    let log = harness.wait_for_log_by_request_id(&request_id).await;
    assert_eq!(log.endpoint, "responses");
    assert_eq!(log.request_format, "openai/responses");
    assert_eq!(log.requested_model, harness.model_name);
    assert_eq!(log.upstream_model, actual_model);
    assert_eq!(log.model_name, harness.model_name);
    assert_eq!(log.total_tokens, 19);
    assert_eq!(log.quota, 19);
    assert_eq!(log.status_code, 200);
    assert_eq!(log.upstream_request_id, "gemini-upstream-stream-123");
    assert_eq!(log.status, LogStatus::Success);
    assert!(log.is_stream);

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn anthropic_responses_route_follow_up_reads_bridged_cache() {
    let actual_model = "claude-sonnet-4-20250514";
    let primary = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: "/v1/messages".into(),
        expected_header_name: "x-api-key".into(),
        expected_header_value: "sk-primary".into(),
        expected_body_substring: Some(format!("\"model\":\"{actual_model}\"")),
        additional_expected_headers: vec![("anthropic-version".into(), "2023-06-01".into())],
        additional_expected_body_substrings: vec!["\"text\":\"Hello\"".into()],
        response_status: StatusCode::OK,
        response_content_type: "application/json".into(),
        response_headers: vec![],
        response_body: serde_json::json!({
            "id": "msg_resp_followup_123",
            "model": actual_model,
            "content": [{"type": "text", "text": "Hello from bridged cache"}],
            "stop_reason": "end_turn",
            "usage": {
                "input_tokens": 12,
                "output_tokens": 7
            }
        })
        .to_string(),
    })
    .await;
    let harness =
        TestHarness::anthropic_responses_affinity_fixture(&primary.base_url, "http://127.0.0.1:9")
            .await;

    let create_response = harness
        .json_request(
            Method::POST,
            "/v1/responses",
            "anthropic-responses-bridge-followup-create",
            serde_json::json!({
                "model": harness.model_name,
                "input": "Hello",
                "stream": false
            }),
        )
        .await;
    assert_eq!(create_response.status(), StatusCode::OK);
    let create_payload = crate::router::test_support::response_json(create_response).await;
    let response_id = create_payload["id"]
        .as_str()
        .expect("bridged response id")
        .to_string();

    let get_response = harness
        .empty_request(
            Method::GET,
            &format!("/v1/responses/{response_id}"),
            "anthropic-responses-bridge-followup-get",
        )
        .await;
    assert_eq!(get_response.status(), StatusCode::OK);
    let get_payload = crate::router::test_support::response_json(get_response).await;
    assert_eq!(get_payload["id"], response_id);
    assert_eq!(get_payload["status"], "completed");
    assert_eq!(get_payload["output_text"], "Hello from bridged cache");

    let input_items_response = harness
        .empty_request(
            Method::GET,
            &format!("/v1/responses/{response_id}/input_items"),
            "anthropic-responses-bridge-followup-input-items",
        )
        .await;
    assert_eq!(input_items_response.status(), StatusCode::OK);
    let input_items_payload =
        crate::router::test_support::response_json(input_items_response).await;
    assert_eq!(input_items_payload["object"], "list");
    assert_eq!(input_items_payload["data"][0]["role"], "user");
    assert_eq!(input_items_payload["data"][0]["content"], "Hello");

    let cancel_response = harness
        .empty_request(
            Method::POST,
            &format!("/v1/responses/{response_id}/cancel"),
            "anthropic-responses-bridge-followup-cancel",
        )
        .await;
    assert_eq!(cancel_response.status(), StatusCode::OK);
    let cancel_payload = crate::router::test_support::response_json(cancel_response).await;
    assert_eq!(cancel_payload["id"], response_id);
    assert_eq!(cancel_payload["status"], "cancelled");

    let get_cancelled_response = harness
        .empty_request(
            Method::GET,
            &format!("/v1/responses/{response_id}"),
            "anthropic-responses-bridge-followup-get-cancelled",
        )
        .await;
    assert_eq!(get_cancelled_response.status(), StatusCode::OK);
    let cancelled_payload =
        crate::router::test_support::response_json(get_cancelled_response).await;
    assert_eq!(cancelled_payload["status"], "cancelled");

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn gemini_responses_route_stream_follow_up_reads_bridged_cache() {
    let actual_model = "gemini-2.5-pro";
    let primary = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: format!(
            "/v1beta/models/{actual_model}:streamGenerateContent?alt=sse"
        ),
        expected_header_name: "x-goog-api-key".into(),
        expected_header_value: "sk-primary".into(),
        expected_body_substring: Some("\"contents\"".into()),
        additional_expected_headers: vec![],
        additional_expected_body_substrings: vec!["\"text\":\"Hello\"".into()],
        response_status: StatusCode::OK,
        response_content_type: "text/event-stream".into(),
        response_headers: vec![],
        response_body: concat!(
            "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"Hello\"}]}}]}\n\n",
            "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\" world\"}]},\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":12,\"candidatesTokenCount\":7,\"totalTokenCount\":19}}\n\n"
        )
        .into(),
    })
    .await;
    let harness =
        TestHarness::gemini_responses_affinity_fixture(&primary.base_url, "http://127.0.0.1:9")
            .await;

    let create_response = harness
        .json_request(
            Method::POST,
            "/v1/responses",
            "gemini-responses-bridge-followup-create",
            serde_json::json!({
                "model": harness.model_name,
                "input": "Hello",
                "stream": true
            }),
        )
        .await;
    assert_eq!(create_response.status(), StatusCode::OK);
    let create_body = crate::router::test_support::response_text(create_response).await;
    let created_event = extract_responses_event(&create_body, "response.created");
    let response_id = created_event["response"]["id"]
        .as_str()
        .expect("stream bridged response id")
        .to_string();

    let cancel_response = harness
        .empty_request(
            Method::POST,
            &format!("/v1/responses/{response_id}/cancel"),
            "gemini-responses-bridge-followup-cancel",
        )
        .await;
    assert_eq!(cancel_response.status(), StatusCode::OK);
    let cancel_payload = crate::router::test_support::response_json(cancel_response).await;
    assert_eq!(cancel_payload["id"], response_id);
    assert_eq!(cancel_payload["status"], "cancelled");

    let get_response = harness
        .empty_request(
            Method::GET,
            &format!("/v1/responses/{response_id}"),
            "gemini-responses-bridge-followup-get",
        )
        .await;
    assert_eq!(get_response.status(), StatusCode::OK);
    let get_payload = crate::router::test_support::response_json(get_response).await;
    assert_eq!(get_payload["id"], response_id);
    assert_eq!(get_payload["status"], "cancelled");
    assert_eq!(get_payload["output_text"], "Hello world");

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn gemini_responses_route_stream_falls_back_after_primary_overload() {
    let actual_model = "gemini-2.5-pro";
    let primary = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: format!(
            "/v1beta/models/{actual_model}:streamGenerateContent?alt=sse"
        ),
        expected_header_name: "x-goog-api-key".into(),
        expected_header_value: "sk-primary".into(),
        expected_body_substring: Some("\"contents\"".into()),
        additional_expected_headers: vec![],
        additional_expected_body_substrings: vec!["\"text\":\"Hello\"".into()],
        response_status: StatusCode::SERVICE_UNAVAILABLE,
        response_content_type: "application/json".into(),
        response_headers: vec![],
        response_body:
            r#"{"error":{"message":"gemini upstream overloaded","type":"server_error","code":"server_error"}}"#
                .to_string(),
    })
    .await;
    let fallback = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: format!(
            "/v1beta/models/{actual_model}:streamGenerateContent?alt=sse"
        ),
        expected_header_name: "x-goog-api-key".into(),
        expected_header_value: "sk-fallback".into(),
        expected_body_substring: Some("\"contents\"".into()),
        additional_expected_headers: vec![],
        additional_expected_body_substrings: vec!["\"text\":\"Hello\"".into()],
        response_status: StatusCode::OK,
        response_content_type: "text/event-stream".into(),
        response_headers: vec![("x-request-id".into(), "gemini-fallback-stream-123".into())],
        response_body: concat!(
            "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"Hello\"}]}}]}\n\n",
            "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\" fallback\"}]},\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":12,\"candidatesTokenCount\":7,\"totalTokenCount\":19}}\n\n"
        )
        .into(),
    })
    .await;
    let harness = TestHarness::gemini_responses_fallback_affinity_fixture(
        &primary.base_url,
        &fallback.base_url,
    )
    .await;
    let request_id = format!("gemini-responses-stream-fallback-{}", harness.model_name);

    let response = harness
        .json_request(
            Method::POST,
            "/v1/responses",
            &request_id,
            serde_json::json!({
                "model": harness.model_name,
                "input": "Hello",
                "stream": true
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let upstream_request_id = response
        .headers()
        .get("x-upstream-request-id")
        .and_then(|value| value.to_str().ok())
        .expect("gemini fallback responses stream upstream request id")
        .to_string();
    let body = crate::router::test_support::response_text(response).await;

    assert!(body.contains("\"type\":\"response.created\""));
    assert!(body.contains("\"type\":\"response.completed\""));
    assert!(body.contains("\"total_tokens\":19"));
    assert!(body.contains("Hello fallback"));
    assert_eq!(upstream_request_id, "gemini-fallback-stream-123");

    let token = harness.wait_for_token_used_quota(19).await;
    assert_eq!(token.used_quota, 19);

    let log = harness.wait_for_log_by_request_id(&request_id).await;
    assert_eq!(log.endpoint, "responses");
    assert_eq!(log.request_format, "openai/responses");
    assert_eq!(log.requested_model, harness.model_name);
    assert_eq!(log.upstream_model, actual_model);
    assert_eq!(log.model_name, harness.model_name);
    assert_eq!(log.total_tokens, 19);
    assert_eq!(log.quota, 19);
    assert_eq!(log.status_code, 200);
    assert_eq!(log.upstream_request_id, "gemini-fallback-stream-123");
    assert_eq!(log.status, LogStatus::Success);
    assert!(log.is_stream);

    let primary_account = harness.wait_for_primary_account_overloaded().await;
    assert_eq!(primary_account.failure_streak, 1);
    assert!(primary_account.overload_until.is_some());
    assert!(primary_account.rate_limited_until.is_none());

    let primary_channel = harness.primary_channel_model().await;
    assert_eq!(primary_channel.failure_streak, 1);
    assert_eq!(primary_channel.last_health_status, 3);

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn gemini_responses_route_stream_skips_overloaded_primary_on_next_request() {
    let actual_model = "gemini-2.5-pro";
    let primary = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: format!(
            "/v1beta/models/{actual_model}:streamGenerateContent?alt=sse"
        ),
        expected_header_name: "x-goog-api-key".into(),
        expected_header_value: "sk-primary".into(),
        expected_body_substring: Some("\"contents\"".into()),
        additional_expected_headers: vec![],
        additional_expected_body_substrings: vec!["\"text\":\"Hello\"".into()],
        response_status: StatusCode::SERVICE_UNAVAILABLE,
        response_content_type: "application/json".into(),
        response_headers: vec![],
        response_body:
            r#"{"error":{"message":"gemini upstream overloaded","type":"server_error","code":"server_error"}}"#
                .to_string(),
    })
    .await;
    let fallback = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: format!(
            "/v1beta/models/{actual_model}:streamGenerateContent?alt=sse"
        ),
        expected_header_name: "x-goog-api-key".into(),
        expected_header_value: "sk-fallback".into(),
        expected_body_substring: Some("\"contents\"".into()),
        additional_expected_headers: vec![],
        additional_expected_body_substrings: vec!["\"text\":\"Hello\"".into()],
        response_status: StatusCode::OK,
        response_content_type: "text/event-stream".into(),
        response_headers: vec![],
        response_body: concat!(
            "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"Hello\"}]}}]}\n\n",
            "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\" fallback\"}]},\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":12,\"candidatesTokenCount\":7,\"totalTokenCount\":19}}\n\n"
        )
        .into(),
    })
    .await;
    let harness = TestHarness::gemini_responses_fallback_affinity_fixture(
        &primary.base_url,
        &fallback.base_url,
    )
    .await;
    let first_request_id = format!("gemini-responses-overload-first-{}", harness.model_name);
    let second_request_id = format!("gemini-responses-overload-second-{}", harness.model_name);

    let first_response = harness
        .json_request(
            Method::POST,
            "/v1/responses",
            &first_request_id,
            serde_json::json!({
                "model": harness.model_name,
                "input": "Hello",
                "stream": true
            }),
        )
        .await;
    assert_eq!(first_response.status(), StatusCode::OK);
    let first_body = crate::router::test_support::response_text(first_response).await;
    assert!(first_body.contains("\"type\":\"response.completed\""));
    assert!(first_body.contains("Hello fallback"));

    let primary_account = harness.wait_for_primary_account_overloaded().await;
    assert_eq!(primary_account.failure_streak, 1);
    assert!(primary_account.overload_until.is_some());

    let second_response = harness
        .json_request(
            Method::POST,
            "/v1/responses",
            &second_request_id,
            serde_json::json!({
                "model": harness.model_name,
                "input": "Hello",
                "stream": true
            }),
        )
        .await;
    assert_eq!(second_response.status(), StatusCode::OK);
    let second_body = crate::router::test_support::response_text(second_response).await;
    assert!(second_body.contains("\"type\":\"response.completed\""));
    assert!(second_body.contains("Hello fallback"));

    let token = harness.wait_for_token_used_quota(38).await;
    assert_eq!(token.used_quota, 38);

    assert_eq!(
        primary.hit_count(&format!(
            "/v1beta/models/{actual_model}:streamGenerateContent?alt=sse"
        )),
        1
    );
    assert_eq!(
        fallback.hit_count(&format!(
            "/v1beta/models/{actual_model}:streamGenerateContent?alt=sse"
        )),
        2
    );

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn anthropic_responses_route_falls_back_after_primary_rate_limit() {
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
            "anthropic-fallback-responses-123".into(),
        )],
        response_body: serde_json::json!({
            "id": "msg_resp_fallback_123",
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
    let harness = TestHarness::anthropic_responses_fallback_affinity_fixture(
        &primary.base_url,
        &fallback.base_url,
    )
    .await;
    let request_id = format!("anthropic-responses-fallback-{}", harness.model_name);

    let response = harness
        .json_request(
            Method::POST,
            "/v1/responses",
            &request_id,
            serde_json::json!({
                "model": harness.model_name,
                "input": "Hello",
                "stream": false
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let upstream_request_id = response
        .headers()
        .get("x-upstream-request-id")
        .and_then(|value| value.to_str().ok())
        .expect("anthropic fallback responses upstream request id")
        .to_string();
    let payload = crate::router::test_support::response_json(response).await;

    assert_eq!(payload["id"], "msg_resp_fallback_123");
    assert_eq!(payload["output_text"], "Hello from Claude fallback");
    assert_eq!(payload["usage"]["total_tokens"], 19);
    assert_eq!(upstream_request_id, "anthropic-fallback-responses-123");

    let token = harness.wait_for_token_used_quota(19).await;
    assert_eq!(token.used_quota, 19);

    let log = harness.wait_for_log_by_request_id(&request_id).await;
    assert_eq!(log.endpoint, "responses");
    assert_eq!(log.request_format, "openai/responses");
    assert_eq!(log.requested_model, harness.model_name);
    assert_eq!(log.upstream_model, actual_model);
    assert_eq!(log.model_name, harness.model_name);
    assert_eq!(log.total_tokens, 19);
    assert_eq!(log.quota, 19);
    assert_eq!(log.status_code, 200);
    assert_eq!(log.upstream_request_id, "anthropic-fallback-responses-123");
    assert_eq!(log.status, LogStatus::Success);

    let primary_account = harness.wait_for_primary_account_rate_limited().await;
    assert_eq!(primary_account.failure_streak, 1);
    assert!(primary_account.rate_limited_until.is_some());
    assert!(primary_account.overload_until.is_none());

    let primary_channel = harness.primary_channel_model().await;
    assert_eq!(primary_channel.failure_streak, 1);
    assert_eq!(primary_channel.last_health_status, 3);

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn anthropic_responses_route_skips_rate_limited_primary_on_next_request() {
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
            "id": "msg_resp_fallback_skip_123",
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
    let harness = TestHarness::anthropic_responses_fallback_affinity_fixture(
        &primary.base_url,
        &fallback.base_url,
    )
    .await;
    let first_request_id = format!(
        "anthropic-responses-rate-limit-first-{}",
        harness.model_name
    );
    let second_request_id = format!(
        "anthropic-responses-rate-limit-second-{}",
        harness.model_name
    );

    let first_response = harness
        .json_request(
            Method::POST,
            "/v1/responses",
            &first_request_id,
            serde_json::json!({
                "model": harness.model_name,
                "input": "Hello",
                "stream": false
            }),
        )
        .await;
    assert_eq!(first_response.status(), StatusCode::OK);
    let first_payload = crate::router::test_support::response_json(first_response).await;
    assert_eq!(first_payload["id"], "msg_resp_fallback_skip_123");

    let primary_account = harness.wait_for_primary_account_rate_limited().await;
    assert_eq!(primary_account.failure_streak, 1);
    assert!(primary_account.rate_limited_until.is_some());

    let second_response = harness
        .json_request(
            Method::POST,
            "/v1/responses",
            &second_request_id,
            serde_json::json!({
                "model": harness.model_name,
                "input": "Hello",
                "stream": false
            }),
        )
        .await;
    assert_eq!(second_response.status(), StatusCode::OK);
    let second_payload = crate::router::test_support::response_json(second_response).await;
    assert_eq!(second_payload["id"], "msg_resp_fallback_skip_123");

    let token = harness.wait_for_token_used_quota(38).await;
    assert_eq!(token.used_quota, 38);

    assert_eq!(primary.hit_count("/v1/messages"), 1);
    assert_eq!(fallback.hit_count("/v1/messages"), 2);

    harness.cleanup().await;
}

#[tokio::test]
async fn embeddings_non_stream_mock_upstream_success() {
    let req = sample_mock_embeddings_request();
    let actual_model = "text-embedding-3-small";
    let (_server, response) = send_mock_embeddings_request(
        1,
        "sk-openai-test",
        &req,
        actual_model,
        MockUpstreamSpec {
            expected_path_and_query: "/v1/embeddings".into(),
            expected_header_name: "authorization".into(),
            expected_header_value: "Bearer sk-openai-test".into(),
            expected_body_substring: Some(format!("\"model\":\"{actual_model}\"")),
            additional_expected_headers: vec![],
            additional_expected_body_substrings: vec![],
            response_status: StatusCode::OK,
            response_content_type: "application/json".into(),
            response_headers: vec![],
            response_body: serde_json::json!({
                "object": "list",
                "data": [{
                    "object": "embedding",
                    "index": 0,
                    "embedding": [0.1, 0.2]
                }],
                "usage": {
                    "prompt_tokens": 8,
                    "completion_tokens": 0,
                    "total_tokens": 8
                }
            })
            .to_string(),
        },
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let parsed: EmbeddingResponse =
        serde_json::from_slice(&response.bytes().await.expect("body")).expect("embeddings json");
    assert_eq!(parsed.data.len(), 1);
    assert_eq!(parsed.usage.total_tokens, 8);
}

#[tokio::test]
async fn gemini_embeddings_non_stream_mock_upstream_success() {
    let req = sample_mock_embeddings_request();
    let actual_model = "text-embedding-004";
    let (_server, response) = send_mock_embeddings_request(
        24,
        "gem-key",
        &req,
        actual_model,
        MockUpstreamSpec {
            expected_path_and_query: format!("/v1beta/models/{actual_model}:embedContent"),
            expected_header_name: "x-goog-api-key".into(),
            expected_header_value: "gem-key".into(),
            expected_body_substring: Some("\"content\"".into()),
            additional_expected_headers: vec![],
            additional_expected_body_substrings: vec![
                "\"model\":\"models/text-embedding-004\"".into(),
                "\"text\":\"hello\"".into(),
            ],
            response_status: StatusCode::OK,
            response_content_type: "application/json".into(),
            response_headers: vec![],
            response_body: serde_json::json!({
                "embedding": {
                    "values": [1.0, 2.0]
                }
            })
            .to_string(),
        },
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let parsed = get_adapter(24)
        .parse_embeddings_response(response.bytes().await.expect("body"), actual_model, 8)
        .expect("parse gemini embeddings response");
    assert_eq!(parsed.data.len(), 1);
    assert_eq!(parsed.data[0].embedding, serde_json::json!([1.0, 2.0]));
    assert_eq!(parsed.usage.total_tokens, 8);
}

#[tokio::test]
async fn gemini_embeddings_batch_mock_upstream_success() {
    let req: EmbeddingRequest = serde_json::from_value(serde_json::json!({
        "model": "text-embedding-004",
        "input": ["hello", "world"]
    }))
    .expect("gemini batch embeddings request");
    let actual_model = "text-embedding-004";
    let (_server, response) = send_mock_embeddings_request(
        24,
        "gem-key",
        &req,
        actual_model,
        MockUpstreamSpec {
            expected_path_and_query: format!("/v1beta/models/{actual_model}:batchEmbedContents"),
            expected_header_name: "x-goog-api-key".into(),
            expected_header_value: "gem-key".into(),
            expected_body_substring: Some("\"requests\"".into()),
            additional_expected_headers: vec![],
            additional_expected_body_substrings: vec![
                "\"model\":\"models/text-embedding-004\"".into(),
                "\"text\":\"hello\"".into(),
                "\"text\":\"world\"".into(),
            ],
            response_status: StatusCode::OK,
            response_content_type: "application/json".into(),
            response_headers: vec![],
            response_body: serde_json::json!({
                "embeddings": [
                    {"values": [1.0, 2.0]},
                    {"values": [3.0, 4.0]}
                ]
            })
            .to_string(),
        },
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let parsed = get_adapter(24)
        .parse_embeddings_response(response.bytes().await.expect("body"), actual_model, 12)
        .expect("parse gemini batch embeddings response");
    assert_eq!(parsed.data.len(), 2);
    assert_eq!(parsed.data[0].embedding, serde_json::json!([1.0, 2.0]));
    assert_eq!(parsed.data[1].embedding, serde_json::json!([3.0, 4.0]));
    assert_eq!(parsed.usage.total_tokens, 12);
}

#[tokio::test]
async fn gemini_embeddings_mock_upstream_provider_failure() {
    let req = sample_mock_embeddings_request();
    let actual_model = "text-embedding-004";
    let (_server, response) = send_mock_embeddings_request(
        24,
        "gem-key",
        &req,
        actual_model,
        MockUpstreamSpec {
            expected_path_and_query: format!("/v1beta/models/{actual_model}:embedContent"),
            expected_header_name: "x-goog-api-key".into(),
            expected_header_value: "gem-key".into(),
            expected_body_substring: Some("\"content\"".into()),
            additional_expected_headers: vec![],
            additional_expected_body_substrings: vec![
                "\"model\":\"models/text-embedding-004\"".into(),
                "\"text\":\"hello\"".into(),
            ],
            response_status: StatusCode::BAD_REQUEST,
            response_content_type: "application/json".into(),
            response_headers: vec![],
            response_body:
                r#"{"error":{"status":"INVALID_ARGUMENT","message":"bad embedding input"}}"#
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
    assert_eq!(failure.error.error.error.message, "bad embedding input");
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn gemini_embeddings_route_normalizes_provider_payload_to_openai_shape() {
    let actual_model = "text-embedding-004";
    let primary = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: format!("/v1beta/models/{actual_model}:embedContent"),
        expected_header_name: "x-goog-api-key".into(),
        expected_header_value: "sk-primary".into(),
        expected_body_substring: Some("\"content\"".into()),
        additional_expected_headers: vec![],
        additional_expected_body_substrings: vec![
            "\"model\":\"models/text-embedding-004\"".into(),
            "\"text\":\"hello\"".into(),
        ],
        response_status: StatusCode::OK,
        response_content_type: "application/json".into(),
        response_headers: vec![(
            "x-request-id".into(),
            "gemini-upstream-embeddings-123".into(),
        )],
        response_body: serde_json::json!({
            "embedding": {
                "values": [1.0, 2.0]
            }
        })
        .to_string(),
    })
    .await;
    let harness =
        TestHarness::gemini_embeddings_affinity_fixture(&primary.base_url, "http://127.0.0.1:9")
            .await;
    let request_id = format!("gemini-embeddings-route-success-{}", harness.model_name);

    let response = harness
        .json_request(
            Method::POST,
            "/v1/embeddings",
            &request_id,
            serde_json::json!({
                "model": harness.model_name,
                "input": "hello"
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let upstream_request_id = response
        .headers()
        .get("x-upstream-request-id")
        .and_then(|value| value.to_str().ok())
        .expect("gemini embeddings upstream request id")
        .to_string();
    let payload = crate::router::test_support::response_json(response).await;

    assert_eq!(payload["object"], "list");
    assert_eq!(
        payload["data"][0]["embedding"],
        serde_json::json!([1.0, 2.0])
    );
    assert_eq!(payload["usage"]["total_tokens"], 2);
    assert_eq!(upstream_request_id, "gemini-upstream-embeddings-123");

    let token = harness.wait_for_token_used_quota(2).await;
    assert_eq!(token.used_quota, 2);

    let log = harness.wait_for_log_by_request_id(&request_id).await;
    assert_eq!(log.endpoint, "embeddings");
    assert_eq!(log.request_format, "openai/embeddings");
    assert_eq!(log.requested_model, harness.model_name);
    assert_eq!(log.upstream_model, actual_model);
    assert_eq!(log.model_name, harness.model_name);
    assert_eq!(log.total_tokens, 2);
    assert_eq!(log.quota, 2);
    assert_eq!(log.status_code, 200);
    assert_eq!(log.upstream_request_id, "gemini-upstream-embeddings-123");
    assert_eq!(log.status, LogStatus::Success);
    assert!(!log.is_stream);

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn gemini_embeddings_route_falls_back_after_primary_invalid_request() {
    let actual_model = "text-embedding-004";
    let primary = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: format!("/v1beta/models/{actual_model}:embedContent"),
        expected_header_name: "x-goog-api-key".into(),
        expected_header_value: "sk-primary".into(),
        expected_body_substring: Some("\"content\"".into()),
        additional_expected_headers: vec![],
        additional_expected_body_substrings: vec![
            "\"model\":\"models/text-embedding-004\"".into(),
            "\"text\":\"hello\"".into(),
        ],
        response_status: StatusCode::BAD_REQUEST,
        response_content_type: "application/json".into(),
        response_headers: vec![],
        response_body: r#"{"error":{"status":"INVALID_ARGUMENT","message":"bad embedding input"}}"#
            .to_string(),
    })
    .await;
    let fallback = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: format!("/v1beta/models/{actual_model}:embedContent"),
        expected_header_name: "x-goog-api-key".into(),
        expected_header_value: "sk-fallback".into(),
        expected_body_substring: Some("\"content\"".into()),
        additional_expected_headers: vec![],
        additional_expected_body_substrings: vec![
            "\"model\":\"models/text-embedding-004\"".into(),
            "\"text\":\"hello\"".into(),
        ],
        response_status: StatusCode::OK,
        response_content_type: "application/json".into(),
        response_headers: vec![],
        response_body: serde_json::json!({
            "embedding": {
                "values": [9.0, 8.0]
            }
        })
        .to_string(),
    })
    .await;
    let harness = TestHarness::gemini_embeddings_fallback_affinity_fixture(
        &primary.base_url,
        &fallback.base_url,
    )
    .await;

    let response = harness
        .json_request(
            Method::POST,
            "/v1/embeddings",
            "gemini-embeddings-route-fallback",
            serde_json::json!({
                "model": harness.model_name,
                "input": "hello"
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let payload = crate::router::test_support::response_json(response).await;

    assert_eq!(
        payload["data"][0]["embedding"],
        serde_json::json!([9.0, 8.0])
    );
    assert_eq!(payload["usage"]["total_tokens"], 2);

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn anthropic_responses_route_quarantines_primary_account_after_auth_failure() {
    let actual_model = "claude-sonnet-4-20250514";
    let primary = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: "/v1/messages".into(),
        expected_header_name: "x-api-key".into(),
        expected_header_value: "sk-primary".into(),
        expected_body_substring: Some(format!("\"model\":\"{actual_model}\"")),
        additional_expected_headers: vec![("anthropic-version".into(), "2023-06-01".into())],
        additional_expected_body_substrings: vec!["\"text\":\"Hello\"".into()],
        response_status: StatusCode::UNAUTHORIZED,
        response_content_type: "application/json".into(),
        response_headers: vec![],
        response_body:
            r#"{"type":"error","error":{"type":"authentication_error","message":"invalid api key"}}"#
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
            "id": "msg_resp_auth_fallback_123",
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
    let harness = TestHarness::anthropic_responses_fallback_affinity_fixture(
        &primary.base_url,
        &fallback.base_url,
    )
    .await;
    let first_request_id = format!("anthropic-responses-auth-first-{}", harness.model_name);
    let second_request_id = format!("anthropic-responses-auth-second-{}", harness.model_name);

    let first_response = harness
        .json_request(
            Method::POST,
            "/v1/responses",
            &first_request_id,
            serde_json::json!({
                "model": harness.model_name,
                "input": "Hello",
                "stream": false
            }),
        )
        .await;
    assert_eq!(first_response.status(), StatusCode::OK);
    let first_payload = crate::router::test_support::response_json(first_response).await;
    assert_eq!(first_payload["id"], "msg_resp_auth_fallback_123");

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
            "/v1/responses",
            &second_request_id,
            serde_json::json!({
                "model": harness.model_name,
                "input": "Hello",
                "stream": false
            }),
        )
        .await;
    assert_eq!(second_response.status(), StatusCode::OK);
    let second_payload = crate::router::test_support::response_json(second_response).await;
    assert_eq!(second_payload["id"], "msg_resp_auth_fallback_123");

    let token = harness.wait_for_token_used_quota(38).await;
    assert_eq!(token.used_quota, 38);

    assert_eq!(primary.hit_count("/v1/messages"), 1);
    assert_eq!(fallback.hit_count("/v1/messages"), 2);

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn gemini_embeddings_route_falls_back_after_primary_rate_limit() {
    let actual_model = "text-embedding-004";
    let primary = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: format!("/v1beta/models/{actual_model}:embedContent"),
        expected_header_name: "x-goog-api-key".into(),
        expected_header_value: "sk-primary".into(),
        expected_body_substring: Some("\"content\"".into()),
        additional_expected_headers: vec![],
        additional_expected_body_substrings: vec![
            "\"model\":\"models/text-embedding-004\"".into(),
            "\"text\":\"hello\"".into(),
        ],
        response_status: StatusCode::TOO_MANY_REQUESTS,
        response_content_type: "application/json".into(),
        response_headers: vec![],
        response_body: r#"{"error":{"status":"RESOURCE_EXHAUSTED","message":"slow down"}}"#
            .to_string(),
    })
    .await;
    let fallback = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: format!("/v1beta/models/{actual_model}:embedContent"),
        expected_header_name: "x-goog-api-key".into(),
        expected_header_value: "sk-fallback".into(),
        expected_body_substring: Some("\"content\"".into()),
        additional_expected_headers: vec![],
        additional_expected_body_substrings: vec![
            "\"model\":\"models/text-embedding-004\"".into(),
            "\"text\":\"hello\"".into(),
        ],
        response_status: StatusCode::OK,
        response_content_type: "application/json".into(),
        response_headers: vec![(
            "x-request-id".into(),
            "gemini-fallback-embeddings-123".into(),
        )],
        response_body: serde_json::json!({
            "embedding": {
                "values": [9.0, 8.0]
            }
        })
        .to_string(),
    })
    .await;
    let harness = TestHarness::gemini_embeddings_fallback_affinity_fixture(
        &primary.base_url,
        &fallback.base_url,
    )
    .await;
    let request_id = format!("gemini-embeddings-route-rate-limit-{}", harness.model_name);

    let response = harness
        .json_request(
            Method::POST,
            "/v1/embeddings",
            &request_id,
            serde_json::json!({
                "model": harness.model_name,
                "input": "hello"
            }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let upstream_request_id = response
        .headers()
        .get("x-upstream-request-id")
        .and_then(|value| value.to_str().ok())
        .expect("gemini embeddings fallback upstream request id")
        .to_string();
    let payload = crate::router::test_support::response_json(response).await;

    assert_eq!(
        payload["data"][0]["embedding"],
        serde_json::json!([9.0, 8.0])
    );
    assert_eq!(payload["usage"]["total_tokens"], 2);
    assert_eq!(upstream_request_id, "gemini-fallback-embeddings-123");

    let token = harness.wait_for_token_used_quota(2).await;
    assert_eq!(token.used_quota, 2);

    let log = harness.wait_for_log_by_request_id(&request_id).await;
    assert_eq!(log.endpoint, "embeddings");
    assert_eq!(log.request_format, "openai/embeddings");
    assert_eq!(log.requested_model, harness.model_name);
    assert_eq!(log.upstream_model, actual_model);
    assert_eq!(log.model_name, harness.model_name);
    assert_eq!(log.total_tokens, 2);
    assert_eq!(log.quota, 2);
    assert_eq!(log.status_code, 200);
    assert_eq!(log.upstream_request_id, "gemini-fallback-embeddings-123");
    assert_eq!(log.status, LogStatus::Success);

    let primary_account = harness.wait_for_primary_account_rate_limited().await;
    assert_eq!(primary_account.failure_streak, 1);
    assert!(primary_account.rate_limited_until.is_some());
    assert!(primary_account.overload_until.is_none());

    let primary_channel = harness.primary_channel_model().await;
    assert_eq!(primary_channel.failure_streak, 1);
    assert_eq!(primary_channel.last_health_status, 3);

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn gemini_embeddings_route_skips_rate_limited_primary_on_next_request() {
    let actual_model = "text-embedding-004";
    let primary = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: format!("/v1beta/models/{actual_model}:embedContent"),
        expected_header_name: "x-goog-api-key".into(),
        expected_header_value: "sk-primary".into(),
        expected_body_substring: Some("\"content\"".into()),
        additional_expected_headers: vec![],
        additional_expected_body_substrings: vec![
            "\"model\":\"models/text-embedding-004\"".into(),
            "\"text\":\"hello\"".into(),
        ],
        response_status: StatusCode::TOO_MANY_REQUESTS,
        response_content_type: "application/json".into(),
        response_headers: vec![],
        response_body: r#"{"error":{"status":"RESOURCE_EXHAUSTED","message":"slow down"}}"#
            .to_string(),
    })
    .await;
    let fallback = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: format!("/v1beta/models/{actual_model}:embedContent"),
        expected_header_name: "x-goog-api-key".into(),
        expected_header_value: "sk-fallback".into(),
        expected_body_substring: Some("\"content\"".into()),
        additional_expected_headers: vec![],
        additional_expected_body_substrings: vec![
            "\"model\":\"models/text-embedding-004\"".into(),
            "\"text\":\"hello\"".into(),
        ],
        response_status: StatusCode::OK,
        response_content_type: "application/json".into(),
        response_headers: vec![],
        response_body: serde_json::json!({
            "embedding": {
                "values": [9.0, 8.0]
            }
        })
        .to_string(),
    })
    .await;
    let harness = TestHarness::gemini_embeddings_fallback_affinity_fixture(
        &primary.base_url,
        &fallback.base_url,
    )
    .await;
    let first_request_id = format!("gemini-embeddings-rate-limit-first-{}", harness.model_name);
    let second_request_id = format!("gemini-embeddings-rate-limit-second-{}", harness.model_name);

    let first_response = harness
        .json_request(
            Method::POST,
            "/v1/embeddings",
            &first_request_id,
            serde_json::json!({
                "model": harness.model_name,
                "input": "hello"
            }),
        )
        .await;
    assert_eq!(first_response.status(), StatusCode::OK);
    let first_payload = crate::router::test_support::response_json(first_response).await;
    assert_eq!(
        first_payload["data"][0]["embedding"],
        serde_json::json!([9.0, 8.0])
    );

    let primary_account = harness.wait_for_primary_account_rate_limited().await;
    assert_eq!(primary_account.failure_streak, 1);
    assert!(primary_account.rate_limited_until.is_some());

    let second_response = harness
        .json_request(
            Method::POST,
            "/v1/embeddings",
            &second_request_id,
            serde_json::json!({
                "model": harness.model_name,
                "input": "hello"
            }),
        )
        .await;
    assert_eq!(second_response.status(), StatusCode::OK);
    let second_payload = crate::router::test_support::response_json(second_response).await;
    assert_eq!(
        second_payload["data"][0]["embedding"],
        serde_json::json!([9.0, 8.0])
    );

    let token = harness.wait_for_token_used_quota(4).await;
    assert_eq!(token.used_quota, 4);

    assert_eq!(
        primary.hit_count(&format!("/v1beta/models/{actual_model}:embedContent")),
        1
    );
    assert_eq!(
        fallback.hit_count(&format!("/v1beta/models/{actual_model}:embedContent")),
        2
    );

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn gemini_embeddings_route_quarantines_primary_account_after_auth_failure() {
    let actual_model = "text-embedding-004";
    let primary = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: format!("/v1beta/models/{actual_model}:embedContent"),
        expected_header_name: "x-goog-api-key".into(),
        expected_header_value: "sk-primary".into(),
        expected_body_substring: Some("\"content\"".into()),
        additional_expected_headers: vec![],
        additional_expected_body_substrings: vec![
            "\"model\":\"models/text-embedding-004\"".into(),
            "\"text\":\"hello\"".into(),
        ],
        response_status: StatusCode::UNAUTHORIZED,
        response_content_type: "application/json".into(),
        response_headers: vec![],
        response_body: r#"{"error":{"status":"UNAUTHENTICATED","message":"invalid api key"}}"#
            .to_string(),
    })
    .await;
    let fallback = spawn_mock_upstream(MockUpstreamSpec {
        expected_path_and_query: format!("/v1beta/models/{actual_model}:embedContent"),
        expected_header_name: "x-goog-api-key".into(),
        expected_header_value: "sk-fallback".into(),
        expected_body_substring: Some("\"content\"".into()),
        additional_expected_headers: vec![],
        additional_expected_body_substrings: vec![
            "\"model\":\"models/text-embedding-004\"".into(),
            "\"text\":\"hello\"".into(),
        ],
        response_status: StatusCode::OK,
        response_content_type: "application/json".into(),
        response_headers: vec![],
        response_body: serde_json::json!({
            "embedding": {
                "values": [9.0, 8.0]
            }
        })
        .to_string(),
    })
    .await;
    let harness = TestHarness::gemini_embeddings_fallback_affinity_fixture(
        &primary.base_url,
        &fallback.base_url,
    )
    .await;
    let first_request_id = format!("gemini-embeddings-auth-first-{}", harness.model_name);
    let second_request_id = format!("gemini-embeddings-auth-second-{}", harness.model_name);

    let first_response = harness
        .json_request(
            Method::POST,
            "/v1/embeddings",
            &first_request_id,
            serde_json::json!({
                "model": harness.model_name,
                "input": "hello"
            }),
        )
        .await;
    assert_eq!(first_response.status(), StatusCode::OK);
    let first_payload = crate::router::test_support::response_json(first_response).await;
    assert_eq!(
        first_payload["data"][0]["embedding"],
        serde_json::json!([9.0, 8.0])
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
            "/v1/embeddings",
            &second_request_id,
            serde_json::json!({
                "model": harness.model_name,
                "input": "hello"
            }),
        )
        .await;
    assert_eq!(second_response.status(), StatusCode::OK);
    let second_payload = crate::router::test_support::response_json(second_response).await;
    assert_eq!(
        second_payload["data"][0]["embedding"],
        serde_json::json!([9.0, 8.0])
    );

    let token = harness.wait_for_token_used_quota(4).await;
    assert_eq!(token.used_quota, 4);

    assert_eq!(
        primary.hit_count(&format!("/v1beta/models/{actual_model}:embedContent")),
        1
    );
    assert_eq!(
        fallback.hit_count(&format!("/v1beta/models/{actual_model}:embedContent")),
        2
    );

    harness.cleanup().await;
}

#[tokio::test]
async fn embeddings_mock_upstream_provider_failure() {
    let req = sample_mock_embeddings_request();
    let actual_model = "text-embedding-3-small";
    let (_server, response) = send_mock_embeddings_request(
        1,
        "sk-openai-test",
        &req,
        actual_model,
        MockUpstreamSpec {
            expected_path_and_query: "/v1/embeddings".into(),
            expected_header_name: "authorization".into(),
            expected_header_value: "Bearer sk-openai-test".into(),
            expected_body_substring: Some(format!("\"model\":\"{actual_model}\"")),
            additional_expected_headers: vec![],
            additional_expected_body_substrings: vec![],
            response_status: StatusCode::BAD_REQUEST,
            response_content_type: "application/json".into(),
            response_headers: vec![],
            response_body: r#"{"error":{"message":"bad embedding input","type":"invalid_request_error","code":"invalid_request_error"}}"#
                .to_string(),
        },
    )
    .await;

    let status = response.status();
    let headers = response.headers().clone();
    let body = response.bytes().await.expect("body");
    let failure = classify_upstream_provider_failure(1, status, &headers, &body);
    assert_eq!(failure.scope, UpstreamFailureScope::Channel);
    assert_eq!(failure.error.status, StatusCode::BAD_REQUEST);
    assert_eq!(failure.error.error.error.r#type, "invalid_request_error");
    assert_eq!(failure.error.error.error.message, "bad embedding input");
}

#[test]
fn classify_anthropic_rate_limit_as_account_failure() {
    let failure = classify_upstream_provider_failure(
        3,
        StatusCode::TOO_MANY_REQUESTS,
        &HeaderMap::new(),
        br#"{"type":"error","error":{"type":"rate_limit_error","message":"slow down"}}"#,
    );

    assert_eq!(failure.scope, UpstreamFailureScope::Account);
    assert_eq!(failure.error.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(failure.error.error.error.r#type, "rate_limit_error");
    assert_eq!(
        failure.error.error.error.code.as_deref(),
        Some("rate_limit_error")
    );
    assert_eq!(failure.error.error.error.message, "slow down");
}

#[test]
fn classify_anthropic_invalid_request_as_channel_failure() {
    let failure = classify_upstream_provider_failure(
        3,
        StatusCode::BAD_REQUEST,
        &HeaderMap::new(),
        br#"{"type":"error","error":{"type":"invalid_request_error","message":"bad claude payload"}}"#,
    );

    assert_eq!(failure.scope, UpstreamFailureScope::Channel);
    assert_eq!(failure.error.status, StatusCode::BAD_REQUEST);
    assert_eq!(failure.error.error.error.r#type, "invalid_request_error");
    assert_eq!(
        failure.error.error.error.code.as_deref(),
        Some("invalid_request_error")
    );
    assert_eq!(failure.error.error.error.message, "bad claude payload");
}

#[test]
fn classify_anthropic_new_api_error_as_account_failure() {
    let failure = classify_upstream_provider_failure(
        3,
        StatusCode::INTERNAL_SERVER_ERROR,
        &HeaderMap::new(),
        br#"{"error":{"type":"new_api_error","message":"invalid claude code request"},"type":"error"}"#,
    );

    assert_eq!(failure.scope, UpstreamFailureScope::Account);
    assert_eq!(failure.error.status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(failure.error.error.error.r#type, "server_error");
    assert_eq!(
        failure.error.error.error.code.as_deref(),
        Some("new_api_error")
    );
    assert_eq!(
        failure.error.error.error.message,
        "invalid claude code request"
    );
}

#[test]
fn classify_gemini_invalid_argument_as_channel_failure() {
    let failure = classify_upstream_provider_failure(
        24,
        StatusCode::BAD_REQUEST,
        &HeaderMap::new(),
        br#"{"error":{"status":"INVALID_ARGUMENT","message":"bad tool schema"}}"#,
    );

    assert_eq!(failure.scope, UpstreamFailureScope::Channel);
    assert_eq!(failure.error.status, StatusCode::BAD_REQUEST);
    assert_eq!(failure.error.error.error.r#type, "invalid_request_error");
    assert_eq!(
        failure.error.error.error.code.as_deref(),
        Some("invalid_argument")
    );
    assert_eq!(failure.error.error.error.message, "bad tool schema");
}

#[test]
fn classify_azure_rate_limit_as_account_failure() {
    let failure = classify_upstream_provider_failure(
        14,
        StatusCode::TOO_MANY_REQUESTS,
        &HeaderMap::new(),
        br#"{"error":{"message":"slow down","type":"rate_limit_error","code":"rate_limit_error"}}"#,
    );

    assert_eq!(failure.scope, UpstreamFailureScope::Account);
    assert_eq!(failure.error.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(failure.error.error.error.r#type, "rate_limit_error");
    assert_eq!(
        failure.error.error.error.code.as_deref(),
        Some("rate_limit_error")
    );
    assert_eq!(failure.error.error.error.message, "slow down");
}

#[test]
fn map_adapter_build_error_uses_unsupported_endpoint_contract() {
    let error = map_adapter_build_error(
        "failed to build upstream responses request",
        anyhow::anyhow!("responses endpoint is not supported"),
    );

    assert_eq!(error.status, StatusCode::BAD_GATEWAY);
    assert_eq!(error.error.error.r#type, "upstream_error");
    assert_eq!(
        error.error.error.code.as_deref(),
        Some("unsupported_endpoint")
    );
    assert_eq!(
        error.error.error.message,
        "responses endpoint is not supported"
    );
}

#[test]
fn map_adapter_build_error_keeps_internal_errors_internal() {
    let error = map_adapter_build_error(
        "failed to build upstream embeddings request",
        anyhow::anyhow!("failed to sign request"),
    );

    assert_eq!(error.status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(error.error.error.r#type, "server_error");
    assert!(
        error
            .error
            .error
            .message
            .contains("failed to build upstream embeddings request")
    );
}

#[test]
fn extract_upstream_request_id_supports_oneapi_header() {
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-oneapi-request-id",
        HeaderValue::from_static("2026032622051868099140Z3FLl6h8"),
    );

    assert_eq!(
        extract_upstream_request_id(&headers),
        "2026032622051868099140Z3FLl6h8"
    );
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn list_models_returns_fixture_models_for_token_group() {
    let harness =
        TestHarness::responses_affinity_fixture("http://127.0.0.1:9", "http://127.0.0.1:10").await;

    let response = harness
        .empty_request(Method::GET, "/v1/models", "list-models")
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let payload = crate::router::test_support::response_json(response).await;

    assert_eq!(payload["object"], "list");
    assert_eq!(payload["data"].as_array().map(Vec::len), Some(1));
    assert_eq!(payload["data"][0]["id"], harness.model_name);

    harness.cleanup().await;
}

#[tokio::test]
#[ignore = "requires local postgres and redis"]
async fn retrieve_model_returns_not_found_for_unknown_fixture_model() {
    let harness =
        TestHarness::responses_affinity_fixture("http://127.0.0.1:9", "http://127.0.0.1:10").await;

    let response = harness
        .empty_request(
            Method::GET,
            "/v1/models/missing-test-model",
            "retrieve-model-missing",
        )
        .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let payload = crate::router::test_support::response_json(response).await;
    assert_eq!(payload["error"]["code"], "not_found");

    harness.cleanup().await;
}
