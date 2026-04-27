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
/// Start ──► [UsageDelta?] ──► TextDelta* ──► [ReasoningDelta* / ToolCallDelta* / ThoughtSignature? 交错] ──► End
/// ```
///
/// `Error` 可在任何时刻 emit，语义是"上游 SSE 流内报错"；收到 `Error` 后下游应
/// 立即终止流并按失败处理（billing refund、tracking failure），不再等 `End`。
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
    /// Claude extended thinking 的 `signature_delta` 透传。
    ///
    /// Multi-turn 场景下一轮客户端要把 signature 回传上游以继承思考状态；
    /// OpenAI / Gemini 协议没有对应字段,ingress 层按需丢弃。
    ThoughtSignature { signature: String },
    /// 流中期的 usage 快照。
    ///
    /// Claude `message_start` 事件里带 `input_tokens + cache_creation_input_tokens +
    /// cache_read_input_tokens`（完整 prompt 侧），但此时 `output_tokens = 0`；
    /// 后续 `message_delta.usage` 只携带累积 `output_tokens`。如果只在 `End`
    /// 事件里抓 usage，prompt_tokens 会整个丢失 —— billing 看到 0 去扣费。
    ///
    /// 这个事件让 adapter 把 message_start 里的 prompt 侧 usage 先 emit 出来，
    /// stream_driver 按字段 merge 到 `final_usage`（非零字段覆盖），最终 billing
    /// 能看到 prompt + cache + completion 完整画像。
    ///
    /// ingress 层一般不需要处理（客户端 wire 协议的 usage 由各自的 End 事件承载）。
    UsageDelta(Usage),
    /// 上游 SSE 流内报错（不同于 HTTP 层错误）。
    ///
    /// Claude 的 `event: error` / OpenAI 中途下发的 `{error: {...}}` chunk 都映射到这里。
    /// 收到该事件后 stream_driver 会终止流并置 outcome 为 Failure。
    Error(StreamError),
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

/// 上游 SSE 流内报错信息。
///
/// `kind` 是上游给的错误类型（Claude 的 `"overloaded_error"` / `"invalid_request_error"` /
/// OpenAI 的 `"server_error"` 等）；不同上游分类不一样，透传给 egress 层再做翻译。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamError {
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}
