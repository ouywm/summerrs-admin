//! Canonical 类型目录。
//!
//! 分两层：
//!
//! - [`common`] — 跨协议共享（message / tool / usage / stream_event）
//! - [`openai`] — OpenAI 协议特有 wire 结构（chat / model）
//!
//! `lib.rs` 会把两者都 re-export 到 crate 顶层，外部调用方可以直接
//! `use summer_ai_core::{ChatRequest, ChatMessage, Usage}` 不用关心子目录。

pub mod common;
pub mod openai;

// 共享类型
pub use common::{
    AudioResponse, ChatMessage, ChatStreamEvent, CompletionTokensDetails, ContentPart,
    FinishReason, ImageUrl, InputAudio, MessageContent, PromptTokensDetails, Role, StreamEnd, Tool,
    ToolCall, ToolCallDelta, ToolCallFunction, ToolChoice, ToolFunction, Usage,
};

// OpenAI 协议 wire 类型
pub use openai::{
    AudioOutputOptions, ChatChoice, ChatRequest, ChatResponse, JsonSchemaFormat, ModelInfo,
    ModelList, ResponseFormat, StreamOptions,
};
