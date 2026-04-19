//! 流式事件（语义级，**不是** OpenAI `ChatCompletionChunk` 的透传）。
//!
//! Adapter 层把上游原生 SSE chunk 归一到这套事件，调用方不用关心 OpenAI delta /
//! Claude `content_block_delta` / Gemini `candidate` 等差异。
//!
//! 接口骨架阶段本文件只定义类型；实际生成由 adapter 层的 stream parser 产出。

use serde::{Deserialize, Serialize};

use super::usage::{FinishReason, Usage};

/// 流事件。
///
/// 常规发射顺序：
///
/// ```text
/// Start ──► TextDelta* ──► [ReasoningDelta* / ToolCallDelta* 交错] ──► End
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatStreamEvent {
    /// 流开始。`adapter` 是协议名（`openai` / `anthropic` / `gemini` ...），
    /// `model` 是上游实际模型名。
    Start { adapter: String, model: String },
    /// 文本增量（assistant content 追加）。
    TextDelta { text: String },
    /// 推理内容增量（o1 / Claude extended thinking / DeepSeek reasoning）。
    ReasoningDelta { text: String },
    /// 工具调用增量。
    ToolCallDelta(ToolCallDelta),
    /// 流结束。
    End(StreamEnd),
}

/// 工具调用 delta。上游通常按 index 分组投递：
///
/// - 首条 delta 带 `id` + `name`；
/// - 后续只追加 `arguments_delta`。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallDelta {
    pub index: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments_delta: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StreamEnd {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<FinishReason>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
}
