use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum_client_ip::ClientIpSource;
use sea_orm::prelude::BigDecimal;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Database, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    Set,
};
use summer::App;
use summer::plugin::MutableComponentRegistry;
use summer_ai_model::entity::{
    ability, channel, channel_account, log, model_config, request, request_execution, token,
};
use summer_redis::redis::AsyncCommands;
use summer_web::axum::Extension;
use summer_web::axum::Router as AxumRouter;
use summer_web::axum::body::{Body, to_bytes};
use summer_web::axum::extract::{Request, State};
use summer_web::axum::http::{Method, StatusCode, header};
use summer_web::axum::response::{IntoResponse, Response};
use summer_web::handler::auto_router;
use summer_web::{AppState, Router};
use tower::ServiceExt;

use crate::auth::middleware::AiAuthLayer;
use crate::relay::http_client::UpstreamHttpClient;
use crate::service::log_batch::AiLogBatchQueue;

const DEFAULT_DATABASE_URL: &str =
    "postgres://admin:123456@localhost/summerrs-admin?options=-c%20TimeZone%3DAsia%2FShanghai";
const DEFAULT_REDIS_URL: &str = "redis://127.0.0.1/";

#[derive(Clone)]
pub(crate) struct MockRoute {
    method: Method,
    path_and_query: String,
    expected_authorization: Option<String>,
    expected_body_substring: Option<String>,
    response_status: StatusCode,
    response_content_type: String,
    response_headers: Vec<(String, String)>,
    response_body: String,
}

impl MockRoute {
    pub(crate) fn json(
        method: Method,
        path_and_query: &str,
        expected_authorization: Option<&str>,
        expected_body_substring: Option<&str>,
        response_status: StatusCode,
        response_body: serde_json::Value,
    ) -> Self {
        Self {
            method,
            path_and_query: path_and_query.to_string(),
            expected_authorization: expected_authorization.map(ToOwned::to_owned),
            expected_body_substring: expected_body_substring.map(ToOwned::to_owned),
            response_status,
            response_content_type: "application/json".to_string(),
            response_headers: Vec::new(),
            response_body: response_body.to_string(),
        }
    }

    pub(crate) fn raw(
        method: Method,
        path_and_query: &str,
        expected_authorization: Option<&str>,
        expected_body_substring: Option<&str>,
        response_status: StatusCode,
        response_content_type: &str,
        response_body: impl Into<String>,
    ) -> Self {
        Self {
            method,
            path_and_query: path_and_query.to_string(),
            expected_authorization: expected_authorization.map(ToOwned::to_owned),
            expected_body_substring: expected_body_substring.map(ToOwned::to_owned),
            response_status,
            response_content_type: response_content_type.to_string(),
            response_headers: Vec::new(),
            response_body: response_body.into(),
        }
    }

    pub(crate) fn with_response_headers(mut self, headers: Vec<(&str, &str)>) -> Self {
        self.response_headers = headers
            .into_iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect();
        self
    }
}

struct MockUpstreamState {
    routes: Mutex<Vec<MockRoute>>,
    hits: Mutex<Vec<String>>,
}

pub(crate) struct MockUpstreamServer {
    pub(crate) base_url: String,
    state: Arc<MockUpstreamState>,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
    _task: tokio::task::JoinHandle<()>,
}

impl MockUpstreamServer {
    pub(crate) async fn spawn(routes: Vec<MockRoute>) -> Self {
        let state = Arc::new(MockUpstreamState {
            routes: Mutex::new(routes),
            hits: Mutex::new(Vec::new()),
        });
        let router = AxumRouter::new()
            .fallback(mock_upstream_handler)
            .with_state(state.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock upstream");
        let addr = listener.local_addr().expect("mock upstream addr");
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            let _ = summer_web::axum::serve(listener, router.into_make_service())
                .with_graceful_shutdown(async move {
                    let _ = shutdown_rx.await;
                })
                .await;
        });

        Self {
            base_url: format!("http://{addr}"),
            state,
            shutdown_tx: Some(shutdown_tx),
            _task: task,
        }
    }

    pub(crate) fn hit_count(&self, path_and_query: &str) -> usize {
        self.state
            .hits
            .lock()
            .expect("mock hits mutex")
            .iter()
            .filter(|hit| hit.as_str() == path_and_query)
            .count()
    }

    pub(crate) fn total_hits(&self) -> usize {
        self.state.hits.lock().expect("mock hits mutex").len()
    }

    pub(crate) fn replace_placeholder(&self, from: &str, to: &str) {
        let mut routes = self.state.routes.lock().expect("mock routes mutex");
        for route in routes.iter_mut() {
            route.response_body = route.response_body.replace(from, to);
            route.expected_body_substring = route
                .expected_body_substring
                .clone()
                .map(|value: String| value.replace(from, to));
        }
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
) -> Response {
    let path_and_query = req
        .uri()
        .path_and_query()
        .map(|value| value.as_str().to_string())
        .unwrap_or_else(|| req.uri().path().to_string());
    state
        .hits
        .lock()
        .expect("mock hits mutex")
        .push(path_and_query.clone());

    let route = match state
        .routes
        .lock()
        .expect("mock routes mutex")
        .iter()
        .find(|route| route.method == req.method() && route.path_and_query == path_and_query)
    {
        Some(route) => route.clone(),
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!(
                    "unexpected upstream route {} {}",
                    req.method(),
                    path_and_query
                ),
            )
                .into_response();
        }
    };

    if let Some(expected_authorization) = route.expected_authorization.as_ref() {
        let actual_authorization = req
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        if actual_authorization != expected_authorization {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("unexpected authorization for {path_and_query}: {actual_authorization}"),
            )
                .into_response();
        }
    }

    if let Some(expected_body_substring) = route.expected_body_substring.as_ref() {
        let body = to_bytes(req.into_body(), usize::MAX)
            .await
            .expect("mock request body");
        let body = String::from_utf8_lossy(&body);
        if !body.contains(expected_body_substring) {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("unexpected body for {path_and_query}: {body}"),
            )
                .into_response();
        }
    }

    let mut response = summer_web::axum::http::Response::builder()
        .status(route.response_status)
        .header(header::CONTENT_TYPE, route.response_content_type)
        .body(Body::from(route.response_body))
        .expect("mock response");
    for (name, value) in &route.response_headers {
        let header_name = header::HeaderName::try_from(name.as_str()).expect("mock header name");
        let header_value = header::HeaderValue::from_str(value).expect("mock header value");
        response.headers_mut().insert(header_name, header_value);
    }
    response
}

pub(crate) struct TestHarness {
    pub(crate) model_name: String,
    raw_api_key: String,
    router: Router,
    db: summer_sea_orm::DbConn,
    redis: summer_redis::Redis,
    cleanup_ids: CleanupIds,
}

pub(crate) struct MultipartRequestSpec<'a> {
    pub(crate) uri: &'a str,
    pub(crate) request_id: &'a str,
    pub(crate) text_fields: &'a [(&'a str, &'a str)],
    pub(crate) file_field_name: &'a str,
    pub(crate) file_name: &'a str,
    pub(crate) file_content_type: &'a str,
    pub(crate) file_bytes: &'a [u8],
}

#[derive(Clone)]
struct CleanupIds {
    token_id: i64,
    primary_channel_id: i64,
    fallback_channel_id: i64,
    primary_account_id: i64,
    fallback_account_id: i64,
    model_config_id: i64,
    ability_ids: Vec<i64>,
}

impl TestHarness {
    async fn scoped_affinity_fixture(
        fixture_name: &str,
        primary_base_url: &str,
        fallback_base_url: &str,
        endpoint_scopes: Vec<&'static str>,
        ability_scopes: Vec<&'static str>,
        include_fallback_abilities: bool,
    ) -> Self {
        Self::scoped_affinity_fixture_with_provider(
            fixture_name,
            primary_base_url,
            fallback_base_url,
            endpoint_scopes,
            ability_scopes,
            include_fallback_abilities,
            channel::ChannelType::OpenAi,
            "openai",
            model_config::ModelType::Chat,
            None,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn scoped_affinity_fixture_with_provider(
        fixture_name: &str,
        primary_base_url: &str,
        fallback_base_url: &str,
        endpoint_scopes: Vec<&'static str>,
        ability_scopes: Vec<&'static str>,
        include_fallback_abilities: bool,
        channel_type: channel::ChannelType,
        vendor_code: &'static str,
        model_type: model_config::ModelType,
        mapped_upstream_model: Option<&'static str>,
    ) -> Self {
        let base = unique_base_id();
        let model_name = format!("{fixture_name}-model-{base}");
        let group = format!("{fixture_name}-group-{base}");
        let raw_api_key = format!("sk-{fixture_name}-{base}");
        let model_mapping = mapped_upstream_model
            .map(|actual_model| serde_json::json!({model_name.clone(): actual_model}))
            .unwrap_or_else(|| serde_json::json!({}));
        let db = shared_test_db().await;
        let redis = shared_test_redis().await;

        let cleanup_ids = seed_fixture(
            &db,
            FixtureSeed {
                base,
                model_name: model_name.clone(),
                group: group.clone(),
                raw_api_key: raw_api_key.clone(),
                primary_base_url: primary_base_url.to_string(),
                fallback_base_url: fallback_base_url.to_string(),
                endpoint_scopes,
                ability_scopes,
                include_fallback_abilities,
                channel_type,
                vendor_code: vendor_code.to_string(),
                model_type,
                model_mapping,
            },
        )
        .await;

        let router = build_test_router(db.clone(), redis.clone()).await;
        Self {
            model_name,
            raw_api_key,
            router,
            db,
            redis,
            cleanup_ids,
        }
    }

    pub(crate) async fn responses_affinity_fixture(
        primary_base_url: &str,
        fallback_base_url: &str,
    ) -> Self {
        Self::scoped_affinity_fixture(
            "responses-affinity",
            primary_base_url,
            fallback_base_url,
            vec!["responses"],
            vec!["responses"],
            false,
        )
        .await
    }

    pub(crate) async fn embeddings_affinity_fixture(
        primary_base_url: &str,
        fallback_base_url: &str,
    ) -> Self {
        Self::scoped_affinity_fixture_with_provider(
            "embeddings-affinity",
            primary_base_url,
            fallback_base_url,
            vec!["embeddings"],
            vec!["embeddings"],
            false,
            channel::ChannelType::OpenAi,
            "openai",
            model_config::ModelType::Embedding,
            None,
        )
        .await
    }

    pub(crate) async fn moderations_affinity_fixture(
        primary_base_url: &str,
        fallback_base_url: &str,
    ) -> Self {
        Self::scoped_affinity_fixture(
            "moderations-affinity",
            primary_base_url,
            fallback_base_url,
            vec!["moderations"],
            vec!["moderations"],
            false,
        )
        .await
    }

    pub(crate) async fn rerank_affinity_fixture(
        primary_base_url: &str,
        fallback_base_url: &str,
    ) -> Self {
        Self::scoped_affinity_fixture(
            "rerank-affinity",
            primary_base_url,
            fallback_base_url,
            vec!["rerank"],
            vec!["rerank"],
            false,
        )
        .await
    }

    pub(crate) async fn files_affinity_fixture(
        primary_base_url: &str,
        fallback_base_url: &str,
    ) -> Self {
        Self::scoped_affinity_fixture_with_provider(
            "files-affinity",
            primary_base_url,
            fallback_base_url,
            vec!["files"],
            vec!["files"],
            false,
            channel::ChannelType::OpenAi,
            "openai",
            model_config::ModelType::Chat,
            None,
        )
        .await
    }

    pub(crate) async fn anthropic_files_affinity_fixture(
        primary_base_url: &str,
        fallback_base_url: &str,
    ) -> Self {
        Self::scoped_affinity_fixture_with_provider(
            "anthropic-files-affinity",
            primary_base_url,
            fallback_base_url,
            vec!["files"],
            vec!["files"],
            false,
            channel::ChannelType::Anthropic,
            "anthropic",
            model_config::ModelType::Chat,
            Some("claude-sonnet-4-20250514"),
        )
        .await
    }

    pub(crate) async fn anthropic_images_affinity_fixture(
        primary_base_url: &str,
        fallback_base_url: &str,
    ) -> Self {
        Self::scoped_affinity_fixture_with_provider(
            "anthropic-images-affinity",
            primary_base_url,
            fallback_base_url,
            vec!["images"],
            vec!["images"],
            false,
            channel::ChannelType::Anthropic,
            "anthropic",
            model_config::ModelType::Image,
            Some("claude-sonnet-4-20250514"),
        )
        .await
    }

    pub(crate) async fn audio_affinity_fixture(
        primary_base_url: &str,
        fallback_base_url: &str,
    ) -> Self {
        Self::scoped_affinity_fixture_with_provider(
            "audio-affinity",
            primary_base_url,
            fallback_base_url,
            vec!["audio"],
            vec!["audio"],
            false,
            channel::ChannelType::OpenAi,
            "openai",
            model_config::ModelType::Audio,
            None,
        )
        .await
    }

    pub(crate) async fn anthropic_audio_affinity_fixture(
        primary_base_url: &str,
        fallback_base_url: &str,
    ) -> Self {
        Self::scoped_affinity_fixture_with_provider(
            "anthropic-audio-affinity",
            primary_base_url,
            fallback_base_url,
            vec!["audio"],
            vec!["audio"],
            false,
            channel::ChannelType::Anthropic,
            "anthropic",
            model_config::ModelType::Audio,
            Some("claude-sonnet-4-20250514"),
        )
        .await
    }

    pub(crate) async fn assistants_threads_affinity_fixture(
        primary_base_url: &str,
        fallback_base_url: &str,
    ) -> Self {
        Self::scoped_affinity_fixture(
            "assistants-threads",
            primary_base_url,
            fallback_base_url,
            vec!["assistants", "threads"],
            vec!["assistants", "threads"],
            false,
        )
        .await
    }

    pub(crate) async fn assistants_threads_fallback_affinity_fixture(
        primary_base_url: &str,
        fallback_base_url: &str,
    ) -> Self {
        Self::scoped_affinity_fixture(
            "assistants-threads-fallback",
            primary_base_url,
            fallback_base_url,
            vec!["assistants", "threads"],
            vec!["assistants", "threads"],
            true,
        )
        .await
    }

    pub(crate) async fn files_vector_stores_affinity_fixture(
        primary_base_url: &str,
        fallback_base_url: &str,
    ) -> Self {
        Self::scoped_affinity_fixture(
            "files-vector-stores",
            primary_base_url,
            fallback_base_url,
            vec!["files", "vector_stores"],
            vec!["files", "vector_stores"],
            true,
        )
        .await
    }

    pub(crate) async fn batches_affinity_fixture(
        primary_base_url: &str,
        fallback_base_url: &str,
    ) -> Self {
        Self::scoped_affinity_fixture(
            "batches-affinity",
            primary_base_url,
            fallback_base_url,
            vec!["batches"],
            vec!["batches"],
            true,
        )
        .await
    }

    pub(crate) async fn uploads_affinity_fixture(
        primary_base_url: &str,
        fallback_base_url: &str,
    ) -> Self {
        Self::scoped_affinity_fixture(
            "uploads-affinity",
            primary_base_url,
            fallback_base_url,
            vec!["uploads"],
            vec!["uploads"],
            true,
        )
        .await
    }

    pub(crate) async fn uploads_files_affinity_fixture(
        primary_base_url: &str,
        fallback_base_url: &str,
    ) -> Self {
        Self::scoped_affinity_fixture(
            "uploads-files-affinity",
            primary_base_url,
            fallback_base_url,
            vec!["uploads", "files"],
            vec!["uploads", "files"],
            true,
        )
        .await
    }

    pub(crate) async fn fine_tuning_affinity_fixture(
        primary_base_url: &str,
        fallback_base_url: &str,
    ) -> Self {
        Self::scoped_affinity_fixture(
            "fine-tuning-affinity",
            primary_base_url,
            fallback_base_url,
            vec!["fine_tuning"],
            vec!["fine_tuning"],
            true,
        )
        .await
    }

    pub(crate) async fn gemini_embeddings_affinity_fixture(
        primary_base_url: &str,
        fallback_base_url: &str,
    ) -> Self {
        Self::scoped_affinity_fixture_with_provider(
            "gemini-embeddings-affinity",
            primary_base_url,
            fallback_base_url,
            vec!["embeddings"],
            vec!["embeddings"],
            false,
            channel::ChannelType::Gemini,
            "gemini",
            model_config::ModelType::Embedding,
            Some("text-embedding-004"),
        )
        .await
    }

    pub(crate) async fn gemini_embeddings_fallback_affinity_fixture(
        primary_base_url: &str,
        fallback_base_url: &str,
    ) -> Self {
        Self::scoped_affinity_fixture_with_provider(
            "gemini-embeddings-fallback-affinity",
            primary_base_url,
            fallback_base_url,
            vec!["embeddings"],
            vec!["embeddings"],
            true,
            channel::ChannelType::Gemini,
            "gemini",
            model_config::ModelType::Embedding,
            Some("text-embedding-004"),
        )
        .await
    }

    pub(crate) async fn anthropic_responses_affinity_fixture(
        primary_base_url: &str,
        fallback_base_url: &str,
    ) -> Self {
        Self::scoped_affinity_fixture_with_provider(
            "anthropic-responses-affinity",
            primary_base_url,
            fallback_base_url,
            vec!["responses"],
            vec!["responses"],
            false,
            channel::ChannelType::Anthropic,
            "anthropic",
            model_config::ModelType::Chat,
            Some("claude-sonnet-4-20250514"),
        )
        .await
    }

    pub(crate) async fn anthropic_completions_affinity_fixture(
        primary_base_url: &str,
        fallback_base_url: &str,
    ) -> Self {
        Self::scoped_affinity_fixture_with_provider(
            "anthropic-completions-affinity",
            primary_base_url,
            fallback_base_url,
            vec!["completions"],
            vec!["completions"],
            false,
            channel::ChannelType::Anthropic,
            "anthropic",
            model_config::ModelType::Chat,
            Some("claude-sonnet-4-20250514"),
        )
        .await
    }

    pub(crate) async fn anthropic_moderations_affinity_fixture(
        primary_base_url: &str,
        fallback_base_url: &str,
    ) -> Self {
        Self::scoped_affinity_fixture_with_provider(
            "anthropic-moderations-affinity",
            primary_base_url,
            fallback_base_url,
            vec!["moderations"],
            vec!["moderations"],
            false,
            channel::ChannelType::Anthropic,
            "anthropic",
            model_config::ModelType::Chat,
            Some("claude-sonnet-4-20250514"),
        )
        .await
    }

    pub(crate) async fn anthropic_rerank_affinity_fixture(
        primary_base_url: &str,
        fallback_base_url: &str,
    ) -> Self {
        Self::scoped_affinity_fixture_with_provider(
            "anthropic-rerank-affinity",
            primary_base_url,
            fallback_base_url,
            vec!["rerank"],
            vec!["rerank"],
            false,
            channel::ChannelType::Anthropic,
            "anthropic",
            model_config::ModelType::Chat,
            Some("claude-sonnet-4-20250514"),
        )
        .await
    }

    pub(crate) async fn anthropic_chat_fallback_affinity_fixture(
        primary_base_url: &str,
        fallback_base_url: &str,
    ) -> Self {
        Self::scoped_affinity_fixture_with_provider(
            "anthropic-chat-fallback-affinity",
            primary_base_url,
            fallback_base_url,
            vec!["chat"],
            vec!["chat"],
            true,
            channel::ChannelType::Anthropic,
            "anthropic",
            model_config::ModelType::Chat,
            Some("claude-sonnet-4-20250514"),
        )
        .await
    }

    pub(crate) async fn anthropic_responses_fallback_affinity_fixture(
        primary_base_url: &str,
        fallback_base_url: &str,
    ) -> Self {
        Self::scoped_affinity_fixture_with_provider(
            "anthropic-responses-fallback-affinity",
            primary_base_url,
            fallback_base_url,
            vec!["responses"],
            vec!["responses"],
            true,
            channel::ChannelType::Anthropic,
            "anthropic",
            model_config::ModelType::Chat,
            Some("claude-sonnet-4-20250514"),
        )
        .await
    }

    pub(crate) async fn gemini_chat_fallback_affinity_fixture(
        primary_base_url: &str,
        fallback_base_url: &str,
    ) -> Self {
        Self::scoped_affinity_fixture_with_provider(
            "gemini-chat-fallback-affinity",
            primary_base_url,
            fallback_base_url,
            vec!["chat"],
            vec!["chat"],
            true,
            channel::ChannelType::Gemini,
            "gemini",
            model_config::ModelType::Chat,
            Some("gemini-2.5-pro"),
        )
        .await
    }

    pub(crate) async fn gemini_responses_affinity_fixture(
        primary_base_url: &str,
        fallback_base_url: &str,
    ) -> Self {
        Self::scoped_affinity_fixture_with_provider(
            "gemini-responses-affinity",
            primary_base_url,
            fallback_base_url,
            vec!["responses"],
            vec!["responses"],
            false,
            channel::ChannelType::Gemini,
            "gemini",
            model_config::ModelType::Chat,
            Some("gemini-2.5-pro"),
        )
        .await
    }

    pub(crate) async fn gemini_responses_fallback_affinity_fixture(
        primary_base_url: &str,
        fallback_base_url: &str,
    ) -> Self {
        Self::scoped_affinity_fixture_with_provider(
            "gemini-responses-fallback-affinity",
            primary_base_url,
            fallback_base_url,
            vec!["responses"],
            vec!["responses"],
            true,
            channel::ChannelType::Gemini,
            "gemini",
            model_config::ModelType::Chat,
            Some("gemini-2.5-pro"),
        )
        .await
    }

    pub(crate) async fn model_passthrough_affinity_fixture(
        primary_base_url: &str,
        fallback_base_url: &str,
    ) -> Self {
        Self::scoped_affinity_fixture(
            "model-passthrough-affinity",
            primary_base_url,
            fallback_base_url,
            vec!["completions", "images", "audio", "moderations", "rerank"],
            vec!["completions", "images", "audio", "moderations", "rerank"],
            true,
        )
        .await
    }

    pub(crate) async fn json_request(
        &self,
        method: Method,
        uri: &str,
        request_id: &str,
        body: serde_json::Value,
    ) -> Response {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header(
                header::AUTHORIZATION,
                format!("Bearer {}", self.raw_api_key),
            )
            .header("x-request-id", request_id)
            .header("x-forwarded-for", "127.0.0.1")
            .header(header::USER_AGENT, "summer-ai-hub-test")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .expect("json request");

        self.router
            .clone()
            .oneshot(request)
            .await
            .expect("router response")
    }

    pub(crate) async fn empty_request(
        &self,
        method: Method,
        uri: &str,
        request_id: &str,
    ) -> Response {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header(
                header::AUTHORIZATION,
                format!("Bearer {}", self.raw_api_key),
            )
            .header("x-request-id", request_id)
            .header("x-forwarded-for", "127.0.0.1")
            .header(header::USER_AGENT, "summer-ai-hub-test")
            .body(Body::empty())
            .expect("empty request");

        self.router
            .clone()
            .oneshot(request)
            .await
            .expect("router response")
    }

    pub(crate) async fn multipart_request(&self, spec: MultipartRequestSpec<'_>) -> Response {
        let boundary = format!("----summer-ai-hub-test-{}", spec.request_id);
        let mut body = Vec::new();
        for (name, value) in spec.text_fields {
            body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
            body.extend_from_slice(
                format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n").as_bytes(),
            );
            body.extend_from_slice(value.as_bytes());
            body.extend_from_slice(b"\r\n");
        }
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(
            format!(
                "Content-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\n",
                spec.file_field_name, spec.file_name
            )
            .as_bytes(),
        );
        body.extend_from_slice(
            format!("Content-Type: {}\r\n\r\n", spec.file_content_type).as_bytes(),
        );
        body.extend_from_slice(spec.file_bytes);
        body.extend_from_slice(b"\r\n");
        body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());

        let request = Request::builder()
            .method(Method::POST)
            .uri(spec.uri)
            .header(
                header::AUTHORIZATION,
                format!("Bearer {}", self.raw_api_key),
            )
            .header("x-request-id", spec.request_id)
            .header("x-forwarded-for", "127.0.0.1")
            .header(header::USER_AGENT, "summer-ai-hub-test")
            .header(
                header::CONTENT_TYPE,
                format!("multipart/form-data; boundary={boundary}"),
            )
            .body(Body::from(body))
            .expect("multipart request");

        self.router
            .clone()
            .oneshot(request)
            .await
            .expect("router response")
    }

    pub(crate) async fn promote_fallback_for_scopes(&self, scopes: &[&str]) {
        for scope in scopes {
            let primary_ability_id = self
                .cleanup_ids
                .ability_ids
                .iter()
                .copied()
                .find(|id| *id == ability_id_for_scope(self.cleanup_ids.primary_channel_id, scope))
                .expect("primary ability id");
            let fallback_ability_id = self
                .cleanup_ids
                .ability_ids
                .iter()
                .copied()
                .find(|id| *id == ability_id_for_scope(self.cleanup_ids.fallback_channel_id, scope))
                .expect("fallback ability id");

            let primary_model = ability::Entity::find_by_id(primary_ability_id)
                .one(&self.db)
                .await
                .expect("load primary ability")
                .expect("primary ability exists");
            let fallback_model = ability::Entity::find_by_id(fallback_ability_id)
                .one(&self.db)
                .await
                .expect("load fallback ability")
                .expect("fallback ability exists");

            let mut primary_active: ability::ActiveModel = primary_model.into();
            primary_active.priority = Set(1);
            primary_active.weight = Set(1);
            primary_active
                .update(&self.db)
                .await
                .expect("update primary ability");

            let mut fallback_active: ability::ActiveModel = fallback_model.into();
            fallback_active.priority = Set(100);
            fallback_active.weight = Set(100);
            fallback_active
                .update(&self.db)
                .await
                .expect("update fallback ability");
        }

        let mut redis = self.redis.clone();
        let _: i64 = redis
            .incr(
                crate::relay::channel_router::route_cache_version_key(),
                1_i64,
            )
            .await
            .expect("bump route cache version");
    }

    pub(crate) async fn token_model(&self) -> token::Model {
        token::Entity::find_by_id(self.cleanup_ids.token_id)
            .one(&self.db)
            .await
            .expect("load token")
            .expect("token exists")
    }

    pub(crate) async fn primary_channel_model(&self) -> channel::Model {
        channel::Entity::find_by_id(self.cleanup_ids.primary_channel_id)
            .one(&self.db)
            .await
            .expect("load primary channel")
            .expect("primary channel exists")
    }

    pub(crate) async fn primary_account_model(&self) -> channel_account::Model {
        channel_account::Entity::find_by_id(self.cleanup_ids.primary_account_id)
            .one(&self.db)
            .await
            .expect("load primary account")
            .expect("primary account exists")
    }

    pub(crate) async fn reset_primary_persistent_route_state(&self) {
        let primary_channel = self.primary_channel_model().await;
        let mut primary_channel_active: channel::ActiveModel = primary_channel.into();
        primary_channel_active.status = Set(channel::ChannelStatus::Enabled);
        primary_channel_active.failure_streak = Set(0);
        primary_channel_active.last_health_status = Set(1);
        primary_channel_active.last_error_at = Set(None);
        primary_channel_active.last_error_code = Set(String::new());
        primary_channel_active.last_error_message = Set(None);
        primary_channel_active
            .update(&self.db)
            .await
            .expect("reset primary channel route state");

        let primary_account = self.primary_account_model().await;
        let mut primary_account_active: channel_account::ActiveModel = primary_account.into();
        primary_account_active.status = Set(channel_account::AccountStatus::Enabled);
        primary_account_active.schedulable = Set(true);
        primary_account_active.failure_streak = Set(0);
        primary_account_active.rate_limited_until = Set(None);
        primary_account_active.overload_until = Set(None);
        primary_account_active.last_error_at = Set(None);
        primary_account_active.last_error_code = Set(String::new());
        primary_account_active.last_error_message = Set(None);
        primary_account_active
            .update(&self.db)
            .await
            .expect("reset primary account route state");

        let mut redis = self.redis.clone();
        let _: i64 = redis
            .incr(
                crate::relay::channel_router::route_cache_version_key(),
                1_i64,
            )
            .await
            .expect("bump route cache version");
    }

    pub(crate) async fn wait_for_token_used_quota(&self, expected_used_quota: i64) -> token::Model {
        for _ in 0..50 {
            let model = self.token_model().await;
            if model.used_quota == expected_used_quota {
                return model;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        panic!("timed out waiting for token used_quota={expected_used_quota}");
    }

    pub(crate) async fn wait_for_log_by_request_id(&self, request_id: &str) -> log::Model {
        for _ in 0..50 {
            if let Some(model) = log::Entity::find()
                .filter(log::Column::RequestId.eq(request_id))
                .one(&self.db)
                .await
                .expect("query log by request id")
            {
                return model;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        panic!("timed out waiting for log request_id={request_id}");
    }

    pub(crate) async fn wait_for_request_by_request_id(&self, request_id: &str) -> request::Model {
        for _ in 0..50 {
            if let Some(model) = request::Entity::find()
                .filter(request::Column::RequestId.eq(request_id))
                .one(&self.db)
                .await
                .expect("query request by request id")
            {
                return model;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        panic!("timed out waiting for request request_id={request_id}");
    }

    pub(crate) async fn wait_for_request_executions_by_request_id(
        &self,
        request_id: &str,
    ) -> Vec<request_execution::Model> {
        for _ in 0..50 {
            let models = request_execution::Entity::find()
                .filter(request_execution::Column::RequestId.eq(request_id))
                .order_by_asc(request_execution::Column::AttemptNo)
                .all(&self.db)
                .await
                .expect("query request executions by request id");
            if !models.is_empty() {
                return models;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        panic!("timed out waiting for request executions request_id={request_id}");
    }

    pub(crate) async fn insert_log(&self, active: log::ActiveModel) -> log::Model {
        let desired_create_time = match &active.create_time {
            sea_orm::ActiveValue::Set(value) | sea_orm::ActiveValue::Unchanged(value) => {
                Some(*value)
            }
            sea_orm::ActiveValue::NotSet => None,
        };

        let model = active.insert(&self.db).await.expect("insert log");
        if let Some(create_time) = desired_create_time {
            let mut active: log::ActiveModel = model.into();
            active.create_time = Set(create_time);
            return active
                .update(&self.db)
                .await
                .expect("update log create_time");
        }

        model
    }

    pub(crate) async fn assert_no_log_by_request_id(&self, request_id: &str) {
        tokio::time::sleep(Duration::from_millis(150)).await;

        let model = log::Entity::find()
            .filter(log::Column::RequestId.eq(request_id))
            .one(&self.db)
            .await
            .expect("query log by request id");
        assert!(
            model.is_none(),
            "expected no log for request_id={request_id}, but one was persisted"
        );
    }

    pub(crate) async fn wait_for_primary_account_rate_limited(&self) -> channel_account::Model {
        for _ in 0..250 {
            let model = self.primary_account_model().await;
            if model.rate_limited_until.is_some() {
                return model;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        panic!("timed out waiting for primary account to become rate limited");
    }

    pub(crate) async fn wait_for_primary_account_overloaded(&self) -> channel_account::Model {
        for _ in 0..250 {
            let model = self.primary_account_model().await;
            if model.overload_until.is_some() {
                return model;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        panic!("timed out waiting for primary account to become overloaded");
    }

    pub(crate) async fn wait_for_primary_account_disabled(&self) -> channel_account::Model {
        for _ in 0..250 {
            let model = self.primary_account_model().await;
            if model.status == channel_account::AccountStatus::Disabled || !model.schedulable {
                return model;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        panic!("timed out waiting for primary account to become disabled");
    }

    pub(crate) async fn count_failed_logs_in_window(
        &self,
        start_time: chrono::DateTime<chrono::FixedOffset>,
        end_time: chrono::DateTime<chrono::FixedOffset>,
    ) -> u64 {
        log::Entity::find()
            .filter(log::Column::TokenId.eq(self.cleanup_ids.token_id))
            .filter(log::Column::Status.eq(log::LogStatus::Failed))
            .filter(log::Column::CreateTime.gte(start_time))
            .filter(log::Column::CreateTime.lte(end_time))
            .count(&self.db)
            .await
            .expect("count failed logs in window")
    }

    pub(crate) async fn delete_logs_by_request_id(&self, request_id: &str) {
        let _ = log::Entity::delete_many()
            .filter(log::Column::RequestId.eq(request_id))
            .exec(&self.db)
            .await;
    }

    pub(crate) async fn cleanup(self) {
        tokio::time::sleep(Duration::from_millis(150)).await;

        let request_ids = request::Entity::find()
            .filter(request::Column::TokenId.eq(self.cleanup_ids.token_id))
            .all(&self.db)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|model| model.id)
            .collect::<Vec<_>>();
        if !request_ids.is_empty() {
            let _ = request_execution::Entity::delete_many()
                .filter(request_execution::Column::AiRequestId.is_in(request_ids.clone()))
                .exec(&self.db)
                .await;
            let _ = request::Entity::delete_many()
                .filter(request::Column::Id.is_in(request_ids))
                .exec(&self.db)
                .await;
        }

        let _ = log::Entity::delete_many()
            .filter(log::Column::TokenId.eq(self.cleanup_ids.token_id))
            .exec(&self.db)
            .await;
        let _ = ability::Entity::delete_many()
            .filter(ability::Column::Id.is_in(self.cleanup_ids.ability_ids.clone()))
            .exec(&self.db)
            .await;
        let _ = channel_account::Entity::delete_many()
            .filter(channel_account::Column::Id.is_in([
                self.cleanup_ids.primary_account_id,
                self.cleanup_ids.fallback_account_id,
            ]))
            .exec(&self.db)
            .await;
        let _ = channel::Entity::delete_many()
            .filter(channel::Column::Id.is_in([
                self.cleanup_ids.primary_channel_id,
                self.cleanup_ids.fallback_channel_id,
            ]))
            .exec(&self.db)
            .await;
        let _ = model_config::Entity::delete_many()
            .filter(model_config::Column::Id.eq(self.cleanup_ids.model_config_id))
            .exec(&self.db)
            .await;
        let _ = token::Entity::delete_many()
            .filter(token::Column::Id.eq(self.cleanup_ids.token_id))
            .exec(&self.db)
            .await;
    }
}

async fn shared_test_db() -> summer_sea_orm::DbConn {
    Database::connect(default_database_url())
        .await
        .expect("connect test db")
}

async fn shared_test_redis() -> summer_redis::Redis {
    summer_redis::redis::Client::open(default_redis_url())
        .expect("create redis client")
        .get_connection_manager()
        .await
        .expect("connect redis")
}

pub(crate) async fn response_json(response: Response) -> serde_json::Value {
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body");
    serde_json::from_slice(&body).expect("json response body")
}

pub(crate) async fn response_text(response: Response) -> String {
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body");
    String::from_utf8(body.to_vec()).expect("utf8 response body")
}

async fn build_test_router(db: summer_sea_orm::DbConn, redis: summer_redis::Redis) -> Router {
    let mut app = App::new();
    app.add_component(db.clone());
    app.add_component(redis);
    app.add_component(UpstreamHttpClient::build().expect("build upstream http client"));
    app.add_component(AiLogBatchQueue::immediate(db));

    let app = app.build().await.expect("build test app");
    auto_router()
        .route_layer(AiAuthLayer::new())
        .layer(ClientIpSource::RightmostXForwardedFor.into_extension())
        .layer(Extension(AppState { app }))
}

struct FixtureSeed {
    base: i64,
    model_name: String,
    group: String,
    raw_api_key: String,
    primary_base_url: String,
    fallback_base_url: String,
    endpoint_scopes: Vec<&'static str>,
    ability_scopes: Vec<&'static str>,
    include_fallback_abilities: bool,
    channel_type: channel::ChannelType,
    vendor_code: String,
    model_type: model_config::ModelType,
    model_mapping: serde_json::Value,
}

async fn seed_fixture(db: &summer_sea_orm::DbConn, seed: FixtureSeed) -> CleanupIds {
    let now = chrono::Utc::now().fixed_offset();
    let primary_channel_id = seed.base + 11;
    let fallback_channel_id = seed.base + 12;
    let primary_account_id = seed.base + 21;
    let fallback_account_id = seed.base + 22;
    let token_id = seed.base + 31;
    let model_config_id = seed.base + 41;
    let mut ability_ids = Vec::new();

    model_config::ActiveModel {
        id: Set(model_config_id),
        model_name: Set(seed.model_name.clone()),
        display_name: Set(seed.model_name.clone()),
        model_type: Set(seed.model_type),
        vendor_code: Set(seed.vendor_code.clone()),
        supported_endpoints: Set(serde_json::json!(seed.endpoint_scopes)),
        input_ratio: Set(BigDecimal::from(1)),
        output_ratio: Set(BigDecimal::from(1)),
        cached_input_ratio: Set(BigDecimal::from(1)),
        reasoning_ratio: Set(BigDecimal::from(1)),
        capabilities: Set(serde_json::json!([])),
        max_context: Set(128_000),
        currency: Set("USD".to_string()),
        effective_from: Set(None),
        metadata: Set(serde_json::json!({})),
        enabled: Set(true),
        remark: Set(String::new()),
        create_by: Set("test".to_string()),
        create_time: Set(now),
        update_by: Set("test".to_string()),
        update_time: Set(now),
    }
    .insert(db)
    .await
    .expect("insert model config");

    channel::ActiveModel {
        id: Set(primary_channel_id),
        name: Set(format!("primary-channel-{}", seed.base)),
        channel_type: Set(seed.channel_type),
        vendor_code: Set(seed.vendor_code.clone()),
        base_url: Set(seed.primary_base_url),
        status: Set(channel::ChannelStatus::Enabled),
        models: Set(serde_json::json!([seed.model_name])),
        model_mapping: Set(seed.model_mapping.clone()),
        channel_group: Set(seed.group.clone()),
        endpoint_scopes: Set(serde_json::json!(seed.endpoint_scopes)),
        capabilities: Set(serde_json::json!([])),
        weight: Set(1),
        priority: Set(1),
        config: Set(serde_json::json!({})),
        auto_ban: Set(false),
        test_model: Set(String::new()),
        used_quota: Set(0),
        balance: Set(BigDecimal::from(0)),
        balance_updated_at: Set(None),
        response_time: Set(0),
        success_rate: Set(BigDecimal::from(1)),
        failure_streak: Set(0),
        last_used_at: Set(None),
        last_error_at: Set(None),
        last_error_code: Set(String::new()),
        last_error_message: Set(None),
        last_health_status: Set(1),
        deleted_at: Set(None),
        remark: Set(String::new()),
        create_by: Set("test".to_string()),
        create_time: Set(now),
        update_by: Set("test".to_string()),
        update_time: Set(now),
    }
    .insert(db)
    .await
    .expect("insert primary channel");

    channel::ActiveModel {
        id: Set(fallback_channel_id),
        name: Set(format!("fallback-channel-{}", seed.base)),
        channel_type: Set(seed.channel_type),
        vendor_code: Set(seed.vendor_code.clone()),
        base_url: Set(seed.fallback_base_url),
        status: Set(channel::ChannelStatus::Enabled),
        models: Set(serde_json::json!([seed.model_name])),
        model_mapping: Set(seed.model_mapping),
        channel_group: Set(seed.group.clone()),
        endpoint_scopes: Set(serde_json::json!(seed.endpoint_scopes)),
        capabilities: Set(serde_json::json!([])),
        weight: Set(1),
        priority: Set(10),
        config: Set(serde_json::json!({})),
        auto_ban: Set(false),
        test_model: Set(String::new()),
        used_quota: Set(0),
        balance: Set(BigDecimal::from(0)),
        balance_updated_at: Set(None),
        response_time: Set(0),
        success_rate: Set(BigDecimal::from(1)),
        failure_streak: Set(0),
        last_used_at: Set(None),
        last_error_at: Set(None),
        last_error_code: Set(String::new()),
        last_error_message: Set(None),
        last_health_status: Set(1),
        deleted_at: Set(None),
        remark: Set(String::new()),
        create_by: Set("test".to_string()),
        create_time: Set(now),
        update_by: Set("test".to_string()),
        update_time: Set(now),
    }
    .insert(db)
    .await
    .expect("insert fallback channel");

    channel_account::ActiveModel {
        id: Set(primary_account_id),
        channel_id: Set(primary_channel_id),
        name: Set(format!("primary-account-{}", seed.base)),
        credential_type: Set("api_key".to_string()),
        credentials: Set(serde_json::json!({"api_key": "sk-primary"})),
        secret_ref: Set(String::new()),
        status: Set(channel_account::AccountStatus::Enabled),
        schedulable: Set(true),
        priority: Set(1),
        weight: Set(1),
        rate_multiplier: Set(BigDecimal::from(1)),
        concurrency_limit: Set(0),
        quota_limit: Set(BigDecimal::from(0)),
        quota_used: Set(BigDecimal::from(0)),
        balance: Set(BigDecimal::from(0)),
        balance_updated_at: Set(None),
        response_time: Set(0),
        failure_streak: Set(0),
        last_used_at: Set(None),
        last_error_at: Set(None),
        last_error_code: Set(String::new()),
        last_error_message: Set(None),
        rate_limited_until: Set(None),
        overload_until: Set(None),
        expires_at: Set(None),
        test_model: Set(String::new()),
        test_time: Set(None),
        extra: Set(serde_json::json!({})),
        deleted_at: Set(None),
        remark: Set(String::new()),
        create_by: Set("test".to_string()),
        create_time: Set(now),
        update_by: Set("test".to_string()),
        update_time: Set(now),
    }
    .insert(db)
    .await
    .expect("insert primary account");

    channel_account::ActiveModel {
        id: Set(fallback_account_id),
        channel_id: Set(fallback_channel_id),
        name: Set(format!("fallback-account-{}", seed.base)),
        credential_type: Set("api_key".to_string()),
        credentials: Set(serde_json::json!({"api_key": "sk-fallback"})),
        secret_ref: Set(String::new()),
        status: Set(channel_account::AccountStatus::Enabled),
        schedulable: Set(true),
        priority: Set(1),
        weight: Set(1),
        rate_multiplier: Set(BigDecimal::from(1)),
        concurrency_limit: Set(0),
        quota_limit: Set(BigDecimal::from(0)),
        quota_used: Set(BigDecimal::from(0)),
        balance: Set(BigDecimal::from(0)),
        balance_updated_at: Set(None),
        response_time: Set(0),
        failure_streak: Set(0),
        last_used_at: Set(None),
        last_error_at: Set(None),
        last_error_code: Set(String::new()),
        last_error_message: Set(None),
        rate_limited_until: Set(None),
        overload_until: Set(None),
        expires_at: Set(None),
        test_model: Set(String::new()),
        test_time: Set(None),
        extra: Set(serde_json::json!({})),
        deleted_at: Set(None),
        remark: Set(String::new()),
        create_by: Set("test".to_string()),
        create_time: Set(now),
        update_by: Set("test".to_string()),
        update_time: Set(now),
    }
    .insert(db)
    .await
    .expect("insert fallback account");

    for scope in &seed.ability_scopes {
        let primary_ability_id = ability_id_for_scope(primary_channel_id, scope);
        ability::ActiveModel {
            id: Set(primary_ability_id),
            channel_group: Set(seed.group.clone()),
            endpoint_scope: Set((*scope).to_string()),
            model: Set(seed.model_name.clone()),
            channel_id: Set(primary_channel_id),
            enabled: Set(true),
            priority: Set(10),
            weight: Set(10),
            route_config: Set(serde_json::json!({})),
            create_time: Set(now),
            update_time: Set(now),
        }
        .insert(db)
        .await
        .expect("insert primary ability");
        ability_ids.push(primary_ability_id);

        if seed.include_fallback_abilities {
            let fallback_ability_id = ability_id_for_scope(fallback_channel_id, scope);
            ability::ActiveModel {
                id: Set(fallback_ability_id),
                channel_group: Set(seed.group.clone()),
                endpoint_scope: Set((*scope).to_string()),
                model: Set(seed.model_name.clone()),
                channel_id: Set(fallback_channel_id),
                enabled: Set(true),
                priority: Set(1),
                weight: Set(1),
                route_config: Set(serde_json::json!({})),
                create_time: Set(now),
                update_time: Set(now),
            }
            .insert(db)
            .await
            .expect("insert fallback ability");
            ability_ids.push(fallback_ability_id);
        }
    }

    token::ActiveModel {
        id: Set(token_id),
        user_id: Set(seed.base),
        service_account_id: Set(0),
        project_id: Set(0),
        name: Set(format!("test-token-{}", seed.base)),
        key_hash: Set(hash_api_key(&seed.raw_api_key)),
        key_prefix: Set(seed.raw_api_key.chars().take(8).collect()),
        status: Set(token::TokenStatus::Enabled),
        remain_quota: Set(1_000_000),
        used_quota: Set(0),
        unlimited_quota: Set(true),
        models: Set(serde_json::json!([seed.model_name])),
        endpoint_scopes: Set(serde_json::json!(seed.endpoint_scopes)),
        ip_whitelist: Set(serde_json::json!([])),
        ip_blacklist: Set(serde_json::json!([])),
        group_code_override: Set(seed.group),
        rpm_limit: Set(0),
        tpm_limit: Set(0),
        concurrency_limit: Set(0),
        daily_quota_limit: Set(0),
        monthly_quota_limit: Set(0),
        daily_used_quota: Set(0),
        monthly_used_quota: Set(0),
        daily_window_start: Set(None),
        monthly_window_start: Set(None),
        expire_time: Set(None),
        access_time: Set(None),
        last_used_ip: Set(String::new()),
        last_user_agent: Set(String::new()),
        remark: Set(String::new()),
        create_by: Set("test".to_string()),
        create_time: Set(now),
        update_by: Set("test".to_string()),
        update_time: Set(now),
    }
    .insert(db)
    .await
    .expect("insert token");

    CleanupIds {
        token_id,
        primary_channel_id,
        fallback_channel_id,
        primary_account_id,
        fallback_account_id,
        model_config_id,
        ability_ids,
    }
}

fn unique_base_id() -> i64 {
    let now = chrono::Utc::now().timestamp_millis().abs();
    now * 1_000 + i64::from(rand::random::<u16>())
}

fn hash_api_key(raw_api_key: &str) -> String {
    use sha2::{Digest, Sha256};

    hex::encode(Sha256::digest(raw_api_key.as_bytes()))
}

fn ability_id_for_scope(channel_id: i64, scope: &str) -> i64 {
    let scope_offset = match scope {
        "responses" => 1,
        "assistants" => 2,
        "threads" => 3,
        "files" => 4,
        "vector_stores" => 5,
        other => {
            let mut hash = 0_i64;
            for byte in other.as_bytes() {
                hash += i64::from(*byte);
            }
            100 + hash
        }
    };
    channel_id * 10 + scope_offset
}

fn default_database_url() -> String {
    std::env::var("DATABASE_URL").unwrap_or_else(|_| DEFAULT_DATABASE_URL.to_string())
}

fn default_redis_url() -> String {
    std::env::var("REDIS_URL").unwrap_or_else(|_| DEFAULT_REDIS_URL.to_string())
}
