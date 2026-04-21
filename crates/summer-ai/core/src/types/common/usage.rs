//! Token 用量与 finish_reason（跨协议共享）。
//!
//! 字段结构严格对齐 [OpenAI Usage 对象](https://platform.openai.com/docs/api-reference/chat/object#chat/object-usage)。
//! 非 OpenAI 家（Claude / Gemini）的 adapter 需要把自家 usage 映射到这套字段。

use serde::{Deserialize, Serialize};

/// Token 用量。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Usage {
    #[serde(default)]
    pub prompt_tokens: i64,
    #[serde(default)]
    pub completion_tokens: i64,
    #[serde(default)]
    pub total_tokens: i64,
    /// 请求侧 token 细分（官方是嵌套 object，不平铺）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_tokens_details: Option<PromptTokensDetails>,
    /// 响应侧 token 细分。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion_tokens_details: Option<CompletionTokensDetails>,
}

/// `prompt_tokens` 的细分。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PromptTokensDetails {
    /// Prompt cache 命中的 token 数（读，Claude 计费 0.1x / OpenAI 默认 0.5x）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached_tokens: Option<i64>,
    /// Prompt cache 写入的 token 数（Claude 计费 1.25x，OpenAI 协议无此概念）。
    ///
    /// Anthropic 的 `usage.cache_creation_input_tokens` 映射到这里；细分 5m/1h
    /// TTL 还会同时出现在上游 wire 的 `cache_creation` 对象，但 canonical 层只
    /// 暴露总量，细分透传到各 ingress 的 extra 字段。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_creation_tokens: Option<i64>,
    /// 音频输入 token 数（多模态）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_tokens: Option<i64>,
}

/// `completion_tokens` 的细分。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CompletionTokensDetails {
    /// 推理 / thinking token 数（o1 / Claude extended thinking）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<i64>,
    /// 音频输出 token 数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_tokens: Option<i64>,
    /// Predicted outputs 命中 token 数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accepted_prediction_tokens: Option<i64>,
    /// Predicted outputs 未命中丢弃的 token 数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rejected_prediction_tokens: Option<i64>,
}

/// 归一化的 finish_reason 枚举。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    Stop,
    Length,
    ContentFilter,
    ToolCalls,
    FunctionCall,
}
