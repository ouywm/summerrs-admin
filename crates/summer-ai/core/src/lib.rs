//! summer-ai-core
//!
//! LLM relay 的协议无关类型与协议适配抽象。
//!
//! # 模块
//!
//! - [`types`] — canonical 类型（OpenAI 对齐 + 共享 common）
//! - [`resolver`] — 运行时上下文（`AuthData` / `Endpoint` / `ServiceTarget`）
//! - [`adapter`] — 协议适配 trait（ZST + 静态 dispatcher）
//! - [`error`] — adapter 层错误

pub mod adapter;
pub mod error;
pub mod resolver;
pub mod types;

pub use adapter::{
    Adapter, AdapterDispatcher, AdapterKind, AuthStrategy, Capabilities, CostProfile, ServiceType,
    WebRequestData,
    adapters::{ClaudeAdapter, GeminiAdapter, OpenAIAdapter, OpenAICompatAdapter},
};
pub use error::{AdapterError, AdapterResult, AuthResolveError};
pub use resolver::{AuthData, Endpoint, ModelIden, ServiceTarget};
pub use types::{
    AudioOutputOptions, AudioResponse, CacheControl, ChatChoice, ChatMessage, ChatRequest,
    ChatResponse, ChatStreamEvent, CompletionTokensDetails, ContentPart, FinishReason, ImageUrl,
    InputAudio, JsonSchemaFormat, MessageContent, MessageOptions, ModelInfo, ModelList,
    PromptTokensDetails, ResponseFormat, Role, StreamEnd, StreamError, StreamOptions, Tool,
    ToolCall, ToolCallDelta, ToolCallFunction, ToolChoice, ToolFunction, Usage,
};
