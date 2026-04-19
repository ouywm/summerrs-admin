//! Ingress / Egress 转换 trait 与状态。
//!
//! 同一份 trait 承担双向转换：
//!
//! - `to_canonical` —— 客户端 wire → canonical（Ingress 方向）
//! - `from_canonical` / `from_canonical_stream_event` —— canonical → 客户端 wire（Egress 方向）
//!
//! 合成一个 trait 是因为两方向共用 `IngressCtx` + state，且同一入口协议一定成对出现。
//!
//! # 添加新入口协议三步
//!
//! 1. [`IngressFormat`] 加一个变体
//! 2. [`StreamConvertState`] 加对应 state 变体（若无状态可复用现有）
//! 3. 新文件 `relay/src/convert/ingress/xxx.rs` 实现 trait

use serde::Serialize;
use serde::de::DeserializeOwned;
use summer_ai_core::{
    AdapterKind, AdapterResult, ChatRequest, ChatResponse, ChatStreamEvent, Usage,
};

pub mod claude;
pub mod gemini;
pub mod openai;

pub use claude::ClaudeIngress;
pub use gemini::GeminiIngress;
pub use openai::OpenAIIngress;

// ---------------------------------------------------------------------------
// IngressFormat
// ---------------------------------------------------------------------------

/// 入口协议识别。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IngressFormat {
    /// `POST /v1/chat/completions`
    OpenAI,
    /// `POST /v1/messages`
    Claude,
    /// `POST /v1beta/models/{model}:generateContent[:streamGenerateContent]`
    Gemini,
    /// `POST /v1/responses`
    OpenAIResponses,
}

impl IngressFormat {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OpenAI => "openai",
            Self::Claude => "claude",
            Self::Gemini => "gemini",
            Self::OpenAIResponses => "openai_responses",
        }
    }
}

// ---------------------------------------------------------------------------
// IngressCtx —— 转换过程中需要知道的上下文
// ---------------------------------------------------------------------------

/// Ingress / Egress 转换时的外部上下文。
///
/// Handler 在组装 `ServiceTarget` 时同时构造 `IngressCtx` 传给 converter。
#[derive(Debug, Clone)]
pub struct IngressCtx {
    /// 上游 adapter 类型。
    ///
    /// 决定方言选项：
    /// - `Claude` / `OpenRouter` (anthropic/*) → 保留 `cache_control`
    /// - 其他 → 丢弃 `cache_control`
    /// - `OpenRouter` 的 `thinking` → 转 `reasoning.max_tokens`
    /// - 其他 → 模型名加 `-thinking` 后缀
    pub channel_kind: AdapterKind,

    /// Channel 厂商代码（仅日志 / 追踪用，可空）。
    pub channel_vendor_code: Option<String>,

    /// 客户端请求里的 model（映射前）。
    pub logical_model: String,

    /// 实际发给上游的 model（映射后）。
    pub actual_model: String,

    /// 上游是否支持 `stream_options`（OpenAI 官方 true，老兼容厂商 false）。
    pub support_stream_options: bool,

    /// 路由时 tokenizer 估算的 prompt token 数。
    ///
    /// 用于 Claude `message_start` 事件的 `usage.input_tokens` 字段——Claude
    /// 流式响应首块需要立刻给 input_tokens，但此时上游还没返 usage，只能估算。
    pub estimated_prompt_tokens: u32,
}

impl IngressCtx {
    /// 便利构造（默认不带 vendor_code / estimated tokens）。
    pub fn new(
        channel_kind: AdapterKind,
        logical_model: impl Into<String>,
        actual_model: impl Into<String>,
    ) -> Self {
        Self {
            channel_kind,
            channel_vendor_code: None,
            logical_model: logical_model.into(),
            actual_model: actual_model.into(),
            support_stream_options: false,
            estimated_prompt_tokens: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// StreamConvertState —— 每个入口协议一个变体
// ---------------------------------------------------------------------------

/// 流式转换状态机。各入口协议一个变体，变体内部自持所需 state。
///
/// Handler 为每个流请求创建一个 state 实例，每个 canonical stream event 调用
/// `from_canonical_stream_event(event, &mut state, ctx)` 更新。
#[derive(Debug)]
pub enum StreamConvertState {
    /// OpenAI identity 无 state。
    Openai,
    /// Claude 6-event 状态机。
    Claude(ClaudeStreamState),
    /// Gemini 轮询式流（每 chunk 一个完整 response）。
    Gemini(GeminiStreamState),
    /// OpenAI Responses API 流状态。
    Responses(ResponsesStreamState),
}

impl StreamConvertState {
    /// 按 [`IngressFormat`] 构造初始 state。
    pub fn for_format(format: IngressFormat) -> Self {
        match format {
            IngressFormat::OpenAI => Self::Openai,
            IngressFormat::Claude => Self::Claude(ClaudeStreamState::default()),
            IngressFormat::Gemini => Self::Gemini(GeminiStreamState::default()),
            IngressFormat::OpenAIResponses => Self::Responses(ResponsesStreamState::default()),
        }
    }
}

// ----- Claude stream state -----

/// Claude 流转换状态——对应客户端的 6-event SSE 重组。
#[derive(Debug, Default)]
pub struct ClaudeStreamState {
    /// 客户端已收到的事件总数；首块前为 0。
    pub send_response_count: u32,
    /// 当前 block 类型。
    pub last_message_type: ClaudeLastMessageType,
    /// 当前 block 的 content_block index（text/thinking 最多一个；tool 可并行多个）。
    pub index: i32,
    /// tool_use 并发时的起点 index。
    pub tool_call_base_index: i32,
    /// tool_use 并发时的最大 offset。
    pub tool_call_max_index_offset: i32,
    /// 上游 finish_reason（停止时写入）。
    pub finish_reason: String,
    /// 累积的 usage。
    pub usage: Option<Usage>,
    /// 是否已发 message_stop。
    pub done: bool,
}

/// Claude stream 当前正在生成的 content block 种类。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ClaudeLastMessageType {
    #[default]
    None,
    Text,
    Thinking,
    Tools,
}

// ----- Gemini stream state -----

/// Gemini 每个 chunk 一个完整 GeminiChatResponse，state 只要追踪已发 candidate 位置。
#[derive(Debug, Default)]
pub struct GeminiStreamState {
    pub emitted_candidates: i32,
}

// ----- Responses stream state -----

/// OpenAI Responses API 流状态（当前占位）。
#[derive(Debug, Default)]
pub struct ResponsesStreamState {}

// ---------------------------------------------------------------------------
// IngressConverter trait
// ---------------------------------------------------------------------------

/// 客户端请求 ↔ canonical 转换器（双向）。
///
/// 每个入口协议一个 ZST 实现（`OpenAIIngress` / `ClaudeIngress` / `GeminiIngress`）。
pub trait IngressConverter {
    /// 客户端请求 wire 类型（如 `ClaudeMessagesRequest`）。
    type ClientRequest: DeserializeOwned;
    /// 客户端响应 wire 类型（如 `ClaudeResponse`）。
    type ClientResponse: Serialize;
    /// 客户端流事件 wire 类型（如 `ClaudeStreamEvent`）。
    type ClientStreamEvent: Serialize;

    /// 对应 [`IngressFormat`] 变体。
    const FORMAT: IngressFormat;

    /// client wire → canonical。
    fn to_canonical(req: Self::ClientRequest, ctx: &IngressCtx) -> AdapterResult<ChatRequest>;

    /// canonical → client wire（非流式）。
    fn from_canonical(resp: ChatResponse, ctx: &IngressCtx) -> AdapterResult<Self::ClientResponse>;

    /// canonical 流事件 → client 流事件（可能一对多）。
    ///
    /// 返 `Vec` 而不是 `Option`，因为 Claude 的 `TextDelta → TextDelta` 是 1→1，
    /// 但 Claude 的 `Start → [message_start, content_block_start]` 是 1→2；
    /// 所以统一返 `Vec` 给实现自由度。
    fn from_canonical_stream_event(
        event: ChatStreamEvent,
        state: &mut StreamConvertState,
        ctx: &IngressCtx,
    ) -> AdapterResult<Vec<Self::ClientStreamEvent>>;
}
