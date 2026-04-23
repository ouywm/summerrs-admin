//! 跨协议共享的 canonical 类型。
//!
//! 这里的每个类型都应该**任何一种上游协议都能用**：
//!
//! - [`message`] — 消息 / 角色 / 多模态内容（文本 + 图片 + 音频）
//! - [`tool`] — function calling
//! - [`usage`] — token 用量 / finish_reason
//! - [`stream_event`] — 语义化流事件
//!
//! 若一个字段只对某家 provider 有意义（如 OpenAI 的 `system_fingerprint`），应该
//! 放到 `types/openai/`（或对应 provider 目录），不放这里。

pub mod message;
pub mod options;
pub mod responses_extras;
pub mod stream_event;
pub mod tool;
pub mod usage;

pub use message::{
    AudioResponse, CacheControl, ChatMessage, ContentPart, ImageUrl, InputAudio, MessageContent,
    MessageOptions, Role,
};
pub use options::{
    ReasoningEffort, ServiceTier, Verbosity, WebSearchContextSize, WebSearchOptions,
};
pub use responses_extras::ResponsesExtras;
pub use stream_event::{ChatStreamEvent, StreamEnd, StreamError, ToolCallDelta};
pub use tool::{Tool, ToolCall, ToolCallFunction, ToolChoice, ToolFunction};
pub use usage::{CompletionTokensDetails, FinishReason, PromptTokensDetails, Usage};
