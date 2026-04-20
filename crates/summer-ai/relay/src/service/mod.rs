//! relay 业务层。
//!
//! - [`chat`] — `/v1/chat/completions` 的同步/流式调用
//! - [`key_picker`] — account 内选 API Key 的策略（random / 未来 rendezvous）
//!
//! 后续 Phase 会加：`embeddings` / `audio` / `images` / `responses` / `rerank`。

pub mod channel_store;
pub mod chat;
pub mod key_picker;
pub mod stream_driver;
pub mod tracking;
