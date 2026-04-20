//! OpenAI 协议入口（`/v1/*` 端点）。
//!
//! 所有 handler 通过 `#[post("/v1/...", group = "summer-ai-relay")]` 宏在编译期
//! 注册到 `inventory`，运行时由 `auto_grouped_routers()` 收集——无需手动聚合。

pub mod chat;
pub mod models;
pub mod responses;
