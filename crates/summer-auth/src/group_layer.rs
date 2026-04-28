//! `GroupAuthLayer` —— 把 [`GroupAuthStrategy`] 包成 tower Layer 的通用实现。
//!
//! 使用流程：
//!
//! ```ignore
//! let group = "summer-system";
//! let strategy = JwtStrategy::new(manager, path_config, group);
//! app.add_group_layer(group, move |router| {
//!     router.layer(GroupAuthLayer::new(strategy.clone()))
//! });
//! ```
//!
//! 运行期一次请求的处理：
//! 1. 永远调 `strategy.authenticate(req)`，由 strategy 自决是否需要鉴权、
//!    是否注入上下文、是否短路返错；
//! 2. `Ok(())` → 透传到下游 handler；
//! 3. `Err(resp)` → layer 直接把这个 Response 作为最终响应返给客户端。
//!
//! 刻意不做前置 `path_config` 过滤——strategy 内部可自行调 `path_config().requires_auth()`
//! 来区分强鉴权 / 软鉴权路径（如 JWT 的"无 token 且非鉴权路径放行"语义）。

use std::sync::Arc;

use summer_web::axum::body::Body;
use summer_web::axum::extract::Request;
use summer_web::axum::response::Response;
use tower_layer::Layer;

use crate::strategy::GroupAuthStrategy;

pub struct GroupAuthLayer<S: GroupAuthStrategy> {
    strategy: Arc<S>,
}

impl<S: GroupAuthStrategy> GroupAuthLayer<S> {
    pub fn new(strategy: S) -> Self {
        Self {
            strategy: Arc::new(strategy),
        }
    }
}

impl<S: GroupAuthStrategy> Clone for GroupAuthLayer<S> {
    fn clone(&self) -> Self {
        Self {
            strategy: self.strategy.clone(),
        }
    }
}

impl<Inner, S> Layer<Inner> for GroupAuthLayer<S>
where
    Inner: Clone,
    S: GroupAuthStrategy,
{
    type Service = GroupAuthMiddleware<Inner, S>;

    fn layer(&self, inner: Inner) -> Self::Service {
        GroupAuthMiddleware {
            inner,
            strategy: self.strategy.clone(),
        }
    }
}

pub struct GroupAuthMiddleware<Inner, S: GroupAuthStrategy> {
    inner: Inner,
    strategy: Arc<S>,
}

impl<Inner: Clone, S: GroupAuthStrategy> Clone for GroupAuthMiddleware<Inner, S> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            strategy: self.strategy.clone(),
        }
    }
}

impl<Inner, S> tower_service::Service<Request<Body>> for GroupAuthMiddleware<Inner, S>
where
    Inner:
        tower_service::Service<Request<Body>, Response = Response<Body>> + Clone + Send + 'static,
    Inner::Future: Send + 'static,
    S: GroupAuthStrategy,
{
    type Response = Response<Body>;
    type Error = Inner::Error;
    type Future = std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>,
    >;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: Request<Body>) -> Self::Future {
        let strategy = self.strategy.clone();
        // Clone 一份干净的 inner 交给异步块，避免 `call` 复用过程中出现
        // "still in use" 的借用问题（`tower::buffer::Buffer` / `ServiceExt::ready` 惯例）。
        let mut inner = self.inner.clone();
        std::mem::swap(&mut inner, &mut self.inner);

        Box::pin(async move {
            if let Err(resp) = strategy.authenticate(&mut req).await {
                return Ok(resp);
            }

            inner.call(req).await
        })
    }
}
