//! summer-ai-relay
//!
//! LLM 中转运行时：OpenAI / Claude / Gemini 多入口协议 → ChannelRouter → AdapterDispatcher → 上游。
//!
//! # 当前状态
//!
//! P3 walking skeleton——OpenAI 入口走通（硬编码 ServiceTarget），后续 Phase 按 MIGRATION_V2 推进。

pub mod convert;
pub mod error;
pub mod plugin;
pub mod router;
pub mod service;

pub use error::{RelayError, RelayResult};
pub use plugin::SummerAiRelayPlugin;
pub use router::relay_router;
