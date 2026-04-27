//! 所有具体 Adapter 实现的入口。

pub mod claude;
pub mod gemini;
pub mod openai;
pub mod openai_resp;
pub mod shared;

pub use claude::ClaudeAdapter;
pub use gemini::GeminiAdapter;
pub use openai::{OpenAIAdapter, OpenAICompatAdapter};
pub use openai_resp::OpenAIRespAdapter;
