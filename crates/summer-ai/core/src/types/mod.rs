//! Canonical 类型目录。
//!
//! 分三层：
//!
//! - [`common`] — 跨协议共享（message / tool / usage / stream_event）
//! - [`openai`] — OpenAI 协议特有 wire 结构（chat / model）
//! - [`ingress_wire`] — 客户端入口协议的 wire 类型（Claude / Gemini / …）
//!
//! `lib.rs` 会把 `common` 和 `openai` 两层的常用类型 re-export 到 crate 顶层；
//! `ingress_wire` 不在顶层 re-export（命名空间会太挤），按需 `use`。

pub mod common;
pub mod ingress_wire;
pub mod openai;

// 共享类型
pub use common::{
    AudioResponse, ChatMessage, ChatStreamEvent, CompletionTokensDetails, ContentPart,
    FinishReason, ImageUrl, InputAudio, MessageContent, PromptTokensDetails, Role, StreamEnd,
    StreamError, Tool, ToolCall, ToolCallDelta, ToolCallFunction, ToolChoice, ToolFunction, Usage,
};

// OpenAI 协议 wire 类型
pub use openai::{
    AudioOutputOptions, ChatChoice, ChatRequest, ChatResponse, JsonSchemaFormat, ModelInfo,
    ModelList, ResponseFormat, StreamOptions,
};
