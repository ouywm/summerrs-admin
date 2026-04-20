//! relay 业务层。
//!
//! - [`chat`] — `/v1/chat/completions` 的同步/流式调用
//! - [`key_picker`] — account 内选 API Key 的策略（random / 未来 rendezvous）
//! - [`model_service`] — `ai.model_config` 的查询门面（`/v1/models` 列表、后续计费）
//!
//! 后续 Phase 会加：`embeddings` / `audio` / `images` / `responses` / `rerank`。

pub mod channel_store;
pub mod chat;
pub mod key_picker;
pub mod model_service;
pub mod stream_driver;
pub mod tracking;
