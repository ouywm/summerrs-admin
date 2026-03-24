use summer_ai_core::types::error::OpenAiErrorResponse;
use summer_web::axum::body::Body;
use summer_web::axum::extract::Request;
use summer_web::axum::http::header::AUTHORIZATION;
use summer_web::axum::http::request::Parts;
use summer_web::axum::response::{IntoResponse, Response};
use summer_web::extractor::RequestPartsExt;
use tower_layer::Layer;

use crate::service::token::TokenService;

/// AiAuthLayer - Axum Layer
#[derive(Clone, Copy, Default)]
pub struct AiAuthLayer;

impl AiAuthLayer {
    pub const fn new() -> Self {
        Self
    }
}

impl<S: Clone> Layer<S> for AiAuthLayer {
    type Service = AiAuthMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        AiAuthMiddleware { inner }
    }
}

/// AiAuthMiddleware - actual middleware service
#[derive(Clone)]
pub struct AiAuthMiddleware<S> {
    inner: S,
}

impl<S> tower_service::Service<Request> for AiAuthMiddleware<S>
where
    S: tower_service::Service<Request, Response = Response<Body>> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = Response<Body>;
    type Error = S::Error;
    type Future = std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>,
    >;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let mut inner = self.inner.clone();

        Box::pin(async move {
            let path = req.uri().path().to_string();
            if !requires_ai_auth(&path) {
                return inner.call(req).await;
            }

            let (mut parts, body) = req.into_parts();

            let Some(raw_key) = extract_bearer_token(&parts) else {
                return Ok(invalid_api_key_response("missing Authorization header"));
            };

            let token_service = match parts.get_component::<TokenService>() {
                Ok(service) => service,
                Err(error) => {
                    return Ok(OpenAiErrorResponse::internal_with(
                        "failed to get token service",
                        error,
                    )
                    .into_response());
                }
            };

            match token_service.validate(&raw_key).await {
                Ok(token_info) => {
                    parts.extensions.insert(token_info);
                    let req = Request::from_parts(parts, body);
                    inner.call(req).await
                }
                Err(error) => Ok(OpenAiErrorResponse::from_api_error(&error).into_response()),
            }
        })
    }
}

fn requires_ai_auth(path: &str) -> bool {
    matches!(
        path,
        "/v1/models"
            | "/v1/chat/completions"
            | "/v1/responses"
            | "/api/v1/models"
            | "/api/v1/chat/completions"
            | "/api/v1/responses"
    )
}

fn extract_bearer_token(parts: &Parts) -> Option<String> {
    parts
        .headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|header| {
            header
                .strip_prefix("Bearer ")
                .or_else(|| header.strip_prefix("bearer "))
        })
        .map(ToOwned::to_owned)
}

fn invalid_api_key_response(message: &str) -> Response<Body> {
    OpenAiErrorResponse::invalid_api_key(message).into_response()
}
