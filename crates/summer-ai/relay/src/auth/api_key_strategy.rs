//! `ApiKeyStrategy` —— AI relay 域的 Bearer API Key 鉴权策略。
//!
//! 对应原 [`super::layer::AiAuthLayer`] 的语义:
//!
//! - `Authorization: Bearer sk-xxx` 取 token
//! - `AiTokenStore::lookup(token)` 查 Redis + DB
//! - 命中 → 注入 [`AiTokenContext`] 到 `Request::extensions`;继续
//! - 未命中 / 失败 → 用构造时绑定的 [`ErrorFlavor`] 返对应格式
//!
//! # 设计:flavor 是路由静态决定的
//!
//! relay 内部按入口协议拆三个子 group(`::openai` / `::claude` / `::gemini`),
//! 每个子 router 独立挂一个 `ApiKeyStrategy`,构造时**直接绑死**对应 flavor。
//! 不再运行时按 path 推断、不再读写 `extensions::<ErrorFlavor>()`。
//!
//! 这样:
//! - 错误格式由路由结构静态保证,新增协议入口只在 relay crate 内改
//! - 每条请求只过一个 strategy(根据匹配的子 router),没有 layer 共享导致的"flavor
//!   传染"问题

use super::context::AiTokenContext;
use super::store::AiTokenStore;
use crate::error::{ErrorFlavor, RelayError};
use axum_extra::headers::authorization::Bearer;
use axum_extra::headers::{Authorization, HeaderMapExt};
use summer_auth::GroupAuthStrategy;
use summer_auth::path_auth::PathAuthConfig;
use summer_web::axum::body::Body;
use summer_web::axum::extract::Request;
use summer_web::axum::response::Response;
use summer_web::extractor::RequestPartsExt;

#[derive(Clone)]
pub struct ApiKeyStrategy {
    group: &'static str,
    flavor: ErrorFlavor,
    path_config: PathAuthConfig,
}

impl ApiKeyStrategy {
    pub fn new(group: &'static str, flavor: ErrorFlavor, path_config: PathAuthConfig) -> Self {
        Self {
            group,
            flavor,
            path_config,
        }
    }

    /// 默认配置:`include = "/**"`,`exclude` 取自该 group 下 `#[public]` / `#[no_auth]`
    /// 编译期注册的公共路由。
    pub fn for_group(group: &'static str, flavor: ErrorFlavor) -> Self {
        Self::for_group_with(group, flavor, PathAuthConfig::new().include("/**"))
    }

    /// 在调用方提供的 [`PathAuthConfig`] 之上,自动并入该 group 下 inventory 注册的
    /// public routes,再绑定到指定 group + flavor。
    pub fn for_group_with(group: &'static str, flavor: ErrorFlavor, cfg: PathAuthConfig) -> Self {
        Self::new(group, flavor, cfg.extend_excludes_from_public_routes(group))
    }
}

#[async_trait::async_trait]
impl GroupAuthStrategy for ApiKeyStrategy {
    fn group(&self) -> &'static str {
        self.group
    }

    fn path_config(&self) -> &PathAuthConfig {
        &self.path_config
    }

    async fn authenticate(&self, req: &mut Request<Body>) -> Result<(), Response<Body>> {
        let method = req.method().clone();
        let path = req.uri().path().to_string();

        let requires_auth = self.path_config.requires_auth(&method, &path);

        if !requires_auth {
            return Ok(());
        }

        let flavor = self.flavor;

        // 从 AppState 获取 AiTokenStore
        let (parts, body) = std::mem::take(req).into_parts();
        let store = parts
            .get_component::<AiTokenStore>()
            .expect("AiTokenStore not found in AppState");
        *req = Request::from_parts(parts, body);

        let Some(raw_token) = extract_bearer(req) else {
            return Err(RelayError::Unauthenticated(
                "missing or malformed Authorization: Bearer <token>",
            )
            .into_response_with(flavor));
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
