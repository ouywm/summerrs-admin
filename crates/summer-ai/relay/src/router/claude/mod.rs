//! Claude Messages API 入口路由（`POST /v1/messages`）。
//!
//! Handler 通过 `#[post("/v1/messages", group = "summer-ai-relay")]` 自动注册。

pub mod messages;
