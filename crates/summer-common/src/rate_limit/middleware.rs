//! 限流响应头自动注入（IETF draft "RateLimit Header Fields for HTTP"）
//!
//! 自动给所有响应添加：
//!
//! - `RateLimit-Limit`: 桶/窗口容量
//! - `RateLimit-Remaining`: 剩余可立即使用的配额
//! - `RateLimit-Reset`: 桶/窗口完全恢复所需秒数
//! - `Retry-After`: 仅 429 时，建议客户端重试等待秒数
//!
//! 这是 IETF 正在标准化的限流头格式，OpenAI / Anthropic / Cloudflare 等都已采用。
//! 客户端 SDK（OpenAI Python SDK、Anthropic SDK）会自动读取这些头做 backoff。
//!
//! ## 用法
//!
//! ```ignore
//! use summer_common::rate_limit::middleware::rate_limit_headers_middleware;
//! use summer_web::axum::{Router, middleware};
//!
//! let app = Router::new()
//!     .route("/api/foo", get(handler))
//!     .layer(middleware::from_fn(rate_limit_headers_middleware));
//! ```
//!
//! 工作原理：[`super::RateLimitContext`] 在 extractor 阶段把
//! `Arc<RateLimitMetadataHolder>` 注入到 `Parts::extensions`；本 middleware
//! 在响应阶段从同一个 holder 读取最严格的 metadata 写入响应头。

use std::sync::Arc;

use summer_web::axum::extract::Request;
use summer_web::axum::http::{HeaderName, HeaderValue};
use summer_web::axum::middleware::Next;
use summer_web::axum::response::Response;

use super::RateLimitMetadataHolder;

static HEADER_LIMIT: HeaderName = HeaderName::from_static("ratelimit-limit");
static HEADER_REMAINING: HeaderName = HeaderName::from_static("ratelimit-remaining");
static HEADER_RESET: HeaderName = HeaderName::from_static("ratelimit-reset");
static HEADER_RETRY_AFTER: HeaderName = HeaderName::from_static("retry-after");

/// axum middleware：响应阶段自动注入 `RateLimit-*` 头。
///
/// 用 `axum::middleware::from_fn(rate_limit_headers_middleware)` 挂载。
///
/// 关键实现细节：middleware **主动创建** [`RateLimitMetadataHolder`] 并 insert
/// 到 request extensions——这样 handler 内的 [`crate::rate_limit::RateLimitContext`]
/// extractor 会复用同一个 holder（见 [`crate::rate_limit::RateLimitContext::from_request_parts`]
/// 的 fallback 逻辑），handler 写入 metadata，middleware 在响应阶段读出。
pub async fn rate_limit_headers_middleware(mut req: Request, next: Next) -> Response {
    // 主动创建 holder，handler 的 extractor 会通过 extensions 拿到同一个
    let holder = Arc::new(RateLimitMetadataHolder::default());
    req.extensions_mut().insert(holder.clone());

    let mut response = next.run(req).await;

    let Some(meta) = holder.snapshot() else {
        return response;
    };

    // unlimited（allowlist 命中、未限流场景）的 metadata 是 u32::MAX 占位值，
    // 写出 `RateLimit-Limit: 4294967295` 没有意义反而会让客户端 SDK 误解析；
    // 这种情况直接跳过。
    if meta.is_unlimited() {
        return response;
    }

    let headers = response.headers_mut();

    if let Ok(value) = HeaderValue::try_from(meta.limit.to_string()) {
        headers.insert(HEADER_LIMIT.clone(), value);
    }
    if let Ok(value) = HeaderValue::try_from(meta.remaining.to_string()) {
        headers.insert(HEADER_REMAINING.clone(), value);
    }
    if let Ok(value) = HeaderValue::try_from(meta.reset_after.as_secs().to_string()) {
        headers.insert(HEADER_RESET.clone(), value);
    }
    if let Some(retry_after) = meta.retry_after
        && let Ok(value) = HeaderValue::try_from(retry_after.as_secs().to_string())
    {
        headers.insert(HEADER_RETRY_AFTER.clone(), value);
    }

    response
}
