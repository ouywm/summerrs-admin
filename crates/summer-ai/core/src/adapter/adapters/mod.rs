//! 所有具体 Adapter 实现的入口。

pub mod anthropic;
pub mod gemini;
pub mod openai;

pub use anthropic::AnthropicAdapter;
pub use gemini::GeminiAdapter;
pub use openai::{OpenAIAdapter, OpenAICompatAdapter};
