//! `ApiKeyStrategy` —— AI relay 域的 Bearer API Key 鉴权策略。
//!
//! 对应原 [`super::layer::AiAuthLayer`] 的语义：
//!
//! - `Authorization: Bearer sk-xxx` 取 token
//! - `AiTokenStore::lookup(token)` 查 Redis + DB
//! - 命中 → 注入 [`AiTokenContext`] 到 `Request::extensions`；继续
//! - 未命中 / 失败 → 按路径推断的 [`ErrorFlavor`]（OpenAI / Claude / Gemini）返对应格式
//!
//! 同时把推断的 `ErrorFlavor` 塞进 extensions，下游 handler / extractor 复用同一份。

use axum_extra::headers::authorization::Bearer;
use axum_extra::headers::{Authorization, HeaderMapExt};
use summer_auth::GroupAuthStrategy;
use summer_web::axum::body::Body;
use summer_web::axum::extract::Request;
use summer_web::axum::response::Response;

use super::context::AiTokenContext;
use super::store::AiTokenStore;
use crate::error::{ErrorFlavor, RelayError};

/// relay 域鉴权策略。由 [`super::layer::AiAuthLayer`] 之外的统一抽象
/// [`summer_auth::GroupAuthLayer`] 承载，挂到 `"summer-ai-relay"` 组。
#[derive(Clone)]
pub struct ApiKeyStrategy {
    store: AiTokenStore,
    group: &'static str,
}

impl ApiKeyStrategy {
    pub fn new(store: AiTokenStore, group: &'static str) -> Self {
        Self { store, group }
    }
}

#[async_trait::async_trait]
impl GroupAuthStrategy for ApiKeyStrategy {
    fn group(&self) -> &'static str {
        self.group
    }

    async fn authenticate(&self, req: &mut Request<Body>) -> Result<(), Response<Body>> {
        let flavor = ErrorFlavor::from_path(req.uri().path());
        req.extensions_mut().insert(flavor);

        let Some(raw_token) = extract_bearer(req) else {
            return Err(RelayError::Unauthenticated(
                "missing or malformed Authorization: Bearer <token>",
            )
            .into_response_with(flavor));
        };

        match self.store.lookup(&raw_token).await {
            Ok(Some(model)) => {
                let ctx = AiTokenContext::from_model(&model);
                tracing::debug!(
                    token_id = ctx.token_id,
                    user_id = ctx.user_id,
                    prefix = %ctx.key_prefix,
                    "ai auth ok"
                );
                req.extensions_mut().insert(ctx);
                Ok(())
            }
            Ok(None) => {
                tracing::info!("ai auth reject: token not found or disabled");
                Err(RelayError::Unauthenticated("invalid api token").into_response_with(flavor))
            }
            Err(e) => {
                tracing::info!(%e, "ai auth reject");
                Err(e.into_response_with(flavor))
            }
        }
    }
}

/// 从请求头提取 `Authorization: Bearer <token>`。只认标准 `Authorization` 头；
/// 后续如需支持 `x-api-key` 等变体，在此扩展。
pub(crate) fn extract_bearer(req: &Request) -> Option<String> {
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
