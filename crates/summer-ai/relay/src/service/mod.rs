//! relay 业务层。
//!
//! - [`chat`] — `/v1/chat/completions` 的同步/流式调用
//!
//! 后续 Phase 会加：`embeddings` / `audio` / `images` / `responses` / `rerank`。

pub mod chat;
