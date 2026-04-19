//! `AiAuthLayer` —— 把 `Authorization: Bearer sk-xxx` 翻译成 [`AiTokenContext`] 注入
//! `Request::extensions`。**所有 relay 路由都必须过这一层**，没过的 handler 无从
//! 拿 `AiTokenContext`（`AiToken` extractor 会 401）。
//!
//! 不做 IP / 限流 / 模型白名单——这些独立 middleware 自己加。

use axum_extra::headers::authorization::Bearer;
use axum_extra::headers::{Authorization, HeaderMapExt};
use std::pin::Pin;
use summer_web::axum::body::Body;
use summer_web::axum::extract::Request;
use summer_web::axum::response::{IntoResponse, Response};
use tower_layer::Layer;
use tower_service::Service;

use super::context::AiTokenContext;
use super::store::AiTokenStore;
use crate::error::RelayError;

/// Tower Layer，挂在 relay 的 Router 上。
#[derive(Clone)]
pub struct AiAuthLayer {
    store: AiTokenStore,
}

impl AiAuthLayer {
    pub fn new(store: AiTokenStore) -> Self {
        Self { store }
    }
}

impl<S: Clone> Layer<S> for AiAuthLayer {
    type Service = AiAuthMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        AiAuthMiddleware {
            inner,
            store: self.store.clone(),
        }
    }
}

/// 实际执行鉴权的 Service。
#[derive(Clone)]
pub struct AiAuthMiddleware<S> {
    inner: S,
    store: AiTokenStore,
}

impl<S> Service<Request> for AiAuthMiddleware<S>
where
    S: Service<Request, Response = Response<Body>> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = Response<Body>;
    type Error = S::Error;
    type Future =
        Pin<Box<dyn std::future::Future<Output = Result<Self::Response, S::Error>> + Send>>;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: Request) -> Self::Future {
        let store = self.store.clone();
        let mut inner = self.inner.clone();

        Box::pin(async move {
            let Some(raw_token) = extract_bearer(&req) else {
                return Ok(RelayError::Unauthenticated(
                    "missing or malformed Authorization: Bearer <token>",
                )
                .into_response());
            };

            match store.lookup(&raw_token).await {
                Ok(Some(model)) => {
                    let ctx = AiTokenContext::from_model(&model);
                    tracing::debug!(
                        token_id = ctx.token_id,
                        user_id = ctx.user_id,
                        prefix = %ctx.key_prefix,
                        "ai auth ok"
                    );
                    req.extensions_mut().insert(ctx);
                    inner.call(req).await
                }
                Ok(None) => {
                    tracing::info!("ai auth reject: token not found or disabled");
                    Ok(RelayError::Unauthenticated("invalid api token").into_response())
                }
                Err(e) => {
                    tracing::info!(%e, "ai auth reject");
                    Ok(e.into_response())
                }
            }
        })
    }
}

/// 从请求头提取 `Authorization: Bearer <token>`。
///
/// 只认标准 `Authorization` 头；后续如需支持 `x-api-key` 等变体，在此扩展。
fn extract_bearer(req: &Request) -> Option<String> {
    req.headers()
        .typed_get::<Authorization<Bearer>>()
        .map(|Authorization(bearer)| bearer.token().to_string())
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use summer_web::axum::http::{HeaderValue, Method, Request as HttpRequest};

    fn make_req(authz: Option<&str>) -> Request {
        let mut builder = HttpRequest::builder().method(Method::POST).uri("/v1/x");
        if let Some(v) = authz {
            builder = builder.header("authorization", HeaderValue::from_str(v).unwrap());
        }
        builder.body(Body::empty()).unwrap()
    }

    #[test]
    fn extract_bearer_parses_standard_header() {
        let req = make_req(Some("Bearer sk-abc"));
        assert_eq!(extract_bearer(&req).as_deref(), Some("sk-abc"));
    }

    #[test]
    fn extract_bearer_missing_header_returns_none() {
        let req = make_req(None);
        assert!(extract_bearer(&req).is_none());
    }

    #[test]
    fn extract_bearer_non_bearer_returns_none() {
        let req = make_req(Some("Basic Zm9vOmJhcg=="));
        assert!(extract_bearer(&req).is_none());
    }

    #[test]
    fn extract_bearer_empty_token_returns_none() {
        let req = make_req(Some("Bearer "));
        assert!(extract_bearer(&req).is_none());
    }
}
