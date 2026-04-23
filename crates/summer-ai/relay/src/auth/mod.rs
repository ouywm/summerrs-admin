//! AI API Token 鉴权子模块。
//!
//! - [`context::AiTokenContext`] —— 鉴权通过后注入 `Request::extensions` 的上下文
//! - [`store::AiTokenStore`] —— Redis + DB 双层查 `ai.token.key_hash`
//! - [`api_key_strategy::ApiKeyStrategy`] —— 统一的 `GroupAuthStrategy` 实现，由
//!   `summer_auth::GroupAuthLayer` 承载，挂到 `"summer-ai-relay"` 组
//! - [`extractor::AiToken`] —— handler 提取 `AiTokenContext` 的 extractor

pub mod api_key_strategy;
pub mod context;
pub mod extractor;
pub mod store;

pub use api_key_strategy::ApiKeyStrategy;
pub use context::{AiTokenContext, ensure_endpoint_scope_allowed};
pub use extractor::AiToken;
pub use store::{AiTokenStore, sha256_hex};
