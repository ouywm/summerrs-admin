//! summer-ai-relay
//!
//! LLM 中转运行时：OpenAI / Claude / Gemini 多入口协议 → ChannelRouter → AdapterDispatcher → 上游。
//!
//! # 当前状态
//!
//! 走路骨架——OpenAI 入口走通（硬编码 ServiceTarget），后续按节奏扩展。

pub mod auth;
pub mod context;
pub mod convert;
pub mod error;
pub mod extract;
pub mod panic_guard;
pub mod pipeline;
pub mod plugin;
pub mod router;
pub mod service;

pub use auth::{AiToken, AiTokenContext, AiTokenStore, ApiKeyStrategy};
pub use context::{AccountSnapshot, ChannelSnapshot, RelayContext};
pub use error::{
    ClaudeError, ClaudeResult, ErrorFlavor, GeminiError, GeminiResult, OpenAIError, OpenAIResult,
    RelayError, RelayResult,
};
pub use extract::RelayRequestMeta;
pub use panic_guard::{claude_panic_guard, gemini_panic_guard, openai_panic_guard};
pub use plugin::SummerAiRelayPlugin;
pub use router::router_with_layers;

/// relay 域整体标识（日志 / 文档 / 边界检测用）。
///
/// **注意**：路由层不再按这个 group 收集 handler。relay 内部按入口协议拆成三个
/// 子 group（[`relay_openai_group`] / [`relay_claude_group`] / [`relay_gemini_group`]），
/// 各自挂自己 [`ErrorFlavor`](crate::error::ErrorFlavor) 硬绑的 `ApiKeyStrategy`
/// + `panic_guard`，flavor 由路由静态决定，不再运行时从 path 推断。
pub fn relay_group() -> &'static str {
    env!("CARGO_PKG_NAME")
}

/// OpenAI 入口子 group：`/v1/chat/completions`、`/v1/responses`、`/v1/models`。
///
/// **必须与 `#[post(..., group = "summer-ai-relay::openai")]` 字面量保持一致**——
/// 宏只接 `LitStr`，无法 `concat!` 这边的常量函数。
pub fn relay_openai_group() -> &'static str {
    "summer-ai-relay::openai"
}

/// Claude 入口子 group：`/v1/messages`。
///
/// **必须与 `#[post(..., group = "summer-ai-relay::claude")]` 字面量保持一致**。
pub fn relay_claude_group() -> &'static str {
    "summer-ai-relay::claude"
}

/// Gemini 入口子 group：`/v1beta/models/{target}`。
///
/// **必须与 `#[post(..., group = "summer-ai-relay::gemini")]` 字面量保持一致**。
pub fn relay_gemini_group() -> &'static str {
    "summer-ai-relay::gemini"
}
