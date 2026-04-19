//! 所有具体 Adapter 实现的入口。

pub mod openai;

pub use openai::{OpenAIAdapter, OpenAICompatAdapter};
