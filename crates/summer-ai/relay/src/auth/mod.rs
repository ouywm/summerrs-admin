//! AI API Token 鉴权子模块。
//!
//! - [`context::AiTokenContext`] —— 鉴权通过后注入 `Request::extensions` 的上下文
//! - [`store::AiTokenStore`] —— Redis + DB 双层查 `ai.token.key_hash`
//! - [`layer::AiAuthLayer`] —— Tower Layer，挂在 relay Router 上，把 Bearer → 上下文
//! - [`extractor::AiToken`] —— handler 提取 `AiTokenContext` 的 extractor

pub mod context;
pub mod extractor;
pub mod layer;
pub mod store;

pub use context::AiTokenContext;
pub use extractor::AiToken;
pub use layer::{AiAuthLayer, AiAuthMiddleware};
pub use store::{AiTokenStore, sha256_hex};
