use std::sync::{Arc, Mutex};

pub(crate) use crate::router::openai::{UpstreamFailureScope, classify_upstream_provider_failure};
use crate::router::tests::support::{MultipartRequestSpec, TestHarness};
pub(crate) use crate::service::openai_http::extract_upstream_request_id;
pub(crate) use crate::service::openai_relay_support::MAX_MULTIPART_FILE_SIZE_BYTES;
pub(crate) use crate::service::openai_tracking::map_adapter_build_error;
pub(crate) use summer_ai_core::provider::{ProviderErrorKind, get_adapter};
pub(crate) use summer_ai_core::types::chat::ChatCompletionRequest;
pub(crate) use summer_ai_core::types::embedding::EmbeddingRequest;
pub(crate) use summer_ai_core::types::responses::ResponsesRequest;
use summer_ai_model::entity::log::LogStatus;
use summer_ai_model::entity::request::RequestStatus;
use summer_ai_model::entity::request_execution::ExecutionStatus;
pub(crate) use summer_web::axum::http::StatusCode;
use summer_web::axum::{
    Router,
    body::{Body, to_bytes},
    extract::{Request, State},
    http::header::CONTENT_TYPE,
    http::{HeaderMap, Method},
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

mod suite_a;
mod suite_b;
mod suite_c;
mod suite_d;
