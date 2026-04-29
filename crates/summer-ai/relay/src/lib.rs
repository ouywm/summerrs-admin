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
pub use plugin::SummerAiRelayPlugin;

pub fn relay_group() -> &'static str {
    env!("CARGO_PKG_NAME")
}
