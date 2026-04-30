//! summer-ai-relay HTTP 路由组装。
//!
//! relay 内部按入口协议拆三个子 group(`::openai` / `::claude` / `::gemini`),每家
//! 协议各自挂上 [`ApiKeyStrategy`](crate::auth::ApiKeyStrategy) 与 [`panic_guard`]
//! 中间件,`flavor` 由路由结构静态决定;共享的 `RequestId` 层挂在三家 merge 完之后
//! 的最外层。
//!
//! app crate 直接调 [`router_with_layers`] 拿到组装好的 `Router`,不需要
//! 知道 relay 内部的中间件细节。
//!
//! [`panic_guard`]: crate::panic_guard

use summer_auth::GroupAuthLayer;
use summer_web::Router;
use summer_web::axum::middleware;
use summer_web::handler::grouped_router;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};

use crate::auth::ApiKeyStrategy;
use crate::error::ErrorFlavor;
use crate::panic_guard::{claude_panic_guard, gemini_panic_guard, openai_panic_guard};
use crate::{relay_claude_group, relay_gemini_group, relay_openai_group};

pub mod claude;
pub mod gemini;
pub mod openai;

/// 组装 relay 域完整的 axum [`Router`],含全部中间件。
///
/// 中间件栈(自外向内):
///
/// 1. `RequestId` —— 三家共享的请求 ID 注入与传播
/// 2. 子 router(三选一,axum 按 path 匹配)
///    - 各自的 [`ApiKeyStrategy`](crate::auth::ApiKeyStrategy):flavor 硬绑
///    - 各自的 panic guard:flavor 硬绑
///    - inventory 注册的对应子 group 路由
///
/// 顺序保证:`panic_guard` 必须在 `GroupAuthLayer` 外层,这样鉴权阶段的 panic
/// 也能被抓到并按对应 flavor 输出。
pub fn router_with_layers() -> Router {
    let openai = grouped_router(relay_openai_group())
        .layer(GroupAuthLayer::new(ApiKeyStrategy::for_group(
            relay_openai_group(),
            ErrorFlavor::OpenAI,
        )))
        .layer(middleware::from_fn(openai_panic_guard));

    let claude = grouped_router(relay_claude_group())
        .layer(GroupAuthLayer::new(ApiKeyStrategy::for_group(
            relay_claude_group(),
            ErrorFlavor::Claude,
        )))
        .layer(middleware::from_fn(claude_panic_guard));

    let gemini = grouped_router(relay_gemini_group())
        .layer(GroupAuthLayer::new(ApiKeyStrategy::for_group(
            relay_gemini_group(),
            ErrorFlavor::Gemini,
        )))
        .layer(middleware::from_fn(gemini_panic_guard));

    Router::new()
        .merge(openai)
        .merge(claude)
        .merge(gemini)
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
}
