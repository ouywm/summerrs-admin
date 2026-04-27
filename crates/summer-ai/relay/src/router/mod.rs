//! summer-ai-relay HTTP 路由模块。
//!
//! 按**入口协议**分子目录：
//!
//! - `openai/` — `/v1/chat/completions` / `/v1/models` / `/v1/responses`
//! - `claude/` — `/v1/messages`
//! - `gemini/` — `/v1beta/models/*/generateContent`
//!
//! 所有 handler 通过 `#[post("/...", group = "summer-ai-relay")]` 宏在编译期注册到
//! `inventory`。运行时 `summer_web::handler::auto_grouped_routers()` 按 group 分桶收集，
//! 而 [`crate::plugin::SummerAiRelayPlugin`] 用 `add_group_layer("summer-ai-relay", ..)`
//! 给这组路由套 `AiAuthLayer`——不会影响其它 crate 的 handler。

pub mod claude;
pub mod gemini;
pub mod openai;
