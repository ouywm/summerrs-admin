//! `panic_guard` —— relay 域专用的 panic 兜底中间件。
//!
//! # 与 `tower_http::catch_panic::CatchPanicLayer` 的差别
//!
//! CatchPanicLayer 的 handler 拿不到 `Request` / 路由上下文,无法决定输出哪种家族
//! ([`ErrorFlavor`]) 的错误 JSON。relay 必须返回与上游 OpenAI / Claude / Gemini 一致
//! 风格的错误,所以这里换成 axum middleware:`Future` 套 `catch_unwind`,panic 时用
//! 中间件**构造时硬绑**的 flavor 输出 [`RelayError::Internal`]。
//!
//! # flavor 静态化
//!
//! 三个入口 ([`openai_panic_guard`] / [`claude_panic_guard`] / [`gemini_panic_guard`])
//! 各自挂在对应协议子 router 上,flavor 由路由结构静态决定。不再运行时按 path 推断、
//! 不再写 `extensions::<ErrorFlavor>()`。新增协议入口时复制一个新函数即可。
//!
//! # 挂载位置
//!
//! 必须挂在子 router 的最外层(在 `GroupAuthLayer` / `RequestId` 之外)。`AssertUnwindSafe`
//! 这里语义安全:panic 后我们不再使用 `next` / `request`,也不向其他线程共享内部状态。

use std::any::Any;
use std::panic::AssertUnwindSafe;

use futures::future::FutureExt;
use summer_web::axum::extract::Request;
use summer_web::axum::middleware::Next;
use summer_web::axum::response::Response;

use crate::error::{ErrorFlavor, RelayError};

/// OpenAI 子 router (`/v1/chat/completions` / `/v1/responses` / `/v1/models`) 专用 guard。
pub async fn openai_panic_guard(request: Request, next: Next) -> Response {
    panic_guard_inner(request, next, ErrorFlavor::OpenAI).await
}

/// Claude 子 router (`/v1/messages`) 专用 guard。
pub async fn claude_panic_guard(request: Request, next: Next) -> Response {
    panic_guard_inner(request, next, ErrorFlavor::Claude).await
}

/// Gemini 子 router (`/v1beta/...`) 专用 guard。
pub async fn gemini_panic_guard(request: Request, next: Next) -> Response {
    panic_guard_inner(request, next, ErrorFlavor::Gemini).await
}

async fn panic_guard_inner(request: Request, next: Next, flavor: ErrorFlavor) -> Response {
    let path = request.uri().path().to_string();

    match AssertUnwindSafe(next.run(request)).catch_unwind().await {
        Ok(response) => response,
        Err(payload) => {
            let detail = downcast_panic_message(payload);
            tracing::error!(path = %path, ?flavor, detail = %detail, "relay handler panicked");
            RelayError::Internal(detail).into_response_with(flavor)
        }
    }
}

fn downcast_panic_message(payload: Box<dyn Any + Send + 'static>) -> String {
    if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else if let Some(s) = payload.downcast_ref::<&'static str>() {
        s.to_string()
    } else {
        "unknown panic payload".to_string()
    }
}
