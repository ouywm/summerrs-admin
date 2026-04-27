//! OpenAI 协议 wire 类型。
//!
//! 字段**严格对齐** OpenAI 官方 API；共享类型（`ChatMessage` / `Usage` / `Tool` /
//! `FinishReason` / `ChatStreamEvent`）从 [`crate::types::common`] 引入。
//!
//! 将来加 Claude native 入口（`POST /v1/messages`）时，在同级新增
//! `types/anthropic/` 目录。

pub mod chat;
pub mod model;

pub use chat::{
    AudioOutputOptions, ChatChoice, ChatRequest, ChatResponse, JsonSchemaFormat, ResponseFormat,
    StreamOptions,
};
pub use model::{ModelInfo, ModelList};
