//! Adapter 层：**ZST + 静态分派**。
//!
//! 每个 provider 对应一个 **零大小 struct**（如 `OpenAIAdapter`），实现 [`Adapter`]
//! trait。Trait 方法全是 **associated functions**（无 `&self`）——调用方**永不
//! 实例化** Adapter，而是通过 [`AdapterDispatcher`] 根据运行时 [`AdapterKind`]
//! 静态 `match` 分派。
//!
//! # 为什么这样设计
//!
//! - **零运行时开销**：ZST + associated fn = 纯函数调用
//! - **编译期穷尽**：加新 adapter 时 `match kind` 的分支如果漏写，编译器会拒绝
//! - **无状态**：adapter 不持有 state，每次请求从 [`ServiceTarget`] 取
//!
//! # 新增 adapter 三步走
//!
//! 1. [`AdapterKind`] 加一个变体 + 编码值
//! 2. 在 `adapters/xxx.rs` 写 `pub struct XxxAdapter;` + `impl Adapter`
//! 3. [`AdapterDispatcher`] 的每个 `match kind` 加一行
//!
//! 借助 Rust 的 exhaustive match 检查，忘记任一步都会编译失败。

pub mod adapters;
pub mod dispatcher;
pub mod kind;

use bytes::Bytes;
use reqwest::header::HeaderMap;
use rust_decimal::Decimal;
use serde_json::Value;
use std::future::Future;

use crate::error::{AdapterError, AdapterResult};
use crate::resolver::{AuthData, Endpoint, ServiceTarget};
use crate::types::{ChatRequest, ChatResponse, ChatStreamEvent};

pub use dispatcher::AdapterDispatcher;
pub use kind::{AdapterKind, InvalidAdapterKind};

// ---------------------------------------------------------------------------
// Capabilities —— 协议能力声明
// ---------------------------------------------------------------------------

/// 协议能力声明。
///
/// Adapter 用 [`Adapter::capabilities()`] 暴露协议默认能力；
/// Channel 可通过 `ai.channel.capabilities` JSONB + [`ServiceTarget::capabilities_override`]
/// **收窄**（例如 DeepSeek 走 OpenAI 协议但暂不支持 `vision`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Capabilities {
    /// 支持 SSE 流式
    pub streaming: bool,
    /// 支持 function/tool calling
    pub tools: bool,
    /// 支持 `tool_choice` 字段
    pub tool_choice: bool,
    /// 支持多模态输入（image/audio/file content parts）
    pub multimodal_input: bool,
    /// 支持 reasoning（含 `reasoning_effort` 或 `thinking.budget_tokens`）
    pub reasoning: bool,
    /// 支持 `response_format` （json_object / json_schema）
    pub response_format: bool,
    /// 支持 `n > 1`
    pub multi_choice: bool,
    /// 支持 prompt caching（Claude cache_control / OpenAI 自动 cache）
    pub prompt_caching: bool,
    /// 支持 parallel_tool_calls
    pub parallel_tool_calls: bool,
}

impl Capabilities {
    /// 主流 OpenAI-compat 的默认能力集合（兜底给没特别声明的 adapter）。
    pub const fn openai_like() -> Self {
        Self {
            streaming: true,
            tools: true,
            tool_choice: true,
            multimodal_input: true,
            reasoning: false,
            response_format: true,
            multi_choice: true,
            prompt_caching: true,
            parallel_tool_calls: true,
        }
    }

    /// Ollama 风格（保守：无 tool_choice / 无多模态 / 无 reasoning）。
    pub const fn ollama_like() -> Self {
        Self {
            streaming: true,
            tools: true,
            tool_choice: false,
            multimodal_input: false,
            reasoning: false,
            response_format: true,
            multi_choice: false,
            prompt_caching: false,
            parallel_tool_calls: false,
        }
    }
}

// ---------------------------------------------------------------------------
// CostProfile —— 协议级计费系数
// ---------------------------------------------------------------------------

/// 协议级计费系数。**不存单价**——单价放 `ai.channel_model_price` 表。
///
/// 这里只是协议维度的"乘数"，例如：
///
/// - Claude：`cache_write_multiplier=1.25`（5m ephemeral）、`cache_read_discount=0.1`
/// - OpenAI：`cache_write_multiplier=1.0`、`cache_read_discount=0.5`
/// - 不支持 prompt cache 的厂商：两者都 `1.0` + `supports_prompt_cache=false`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CostProfile {
    /// 写入 prompt cache 的加价倍率（Claude 5m = 1.25；OpenAI = 1.0；不支持 = 1.0）。
    pub cache_write_multiplier: Decimal,
    /// 命中 prompt cache 时 prompt tokens 的折扣（Claude ≈ 0.1；OpenAI ≈ 0.5；不支持 = 1.0）。
    pub cache_read_discount: Decimal,
    /// 协议是否支持 prompt cache（客户端可据此决定是否带 `cache_control`）。
    pub supports_prompt_cache: bool,
}

impl Default for CostProfile {
    fn default() -> Self {
        Self {
            cache_write_multiplier: Decimal::ONE,
            cache_read_discount: Decimal::ONE,
            supports_prompt_cache: false,
        }
    }
}

impl CostProfile {
    /// Claude 风格（5m ephemeral cache）。
    pub fn anthropic_like() -> Self {
        Self {
            cache_write_multiplier: Decimal::new(125, 2), // 1.25
            cache_read_discount: Decimal::new(1, 1),      // 0.1
            supports_prompt_cache: true,
        }
    }

    /// OpenAI 风格（自动 prompt cache，写入不加价）。
    pub fn openai_like() -> Self {
        Self {
            cache_write_multiplier: Decimal::ONE,
            cache_read_discount: Decimal::new(5, 1), // 0.5
            supports_prompt_cache: true,
        }
    }
}

// ---------------------------------------------------------------------------
// AuthStrategy —— 声明式鉴权
// ---------------------------------------------------------------------------

/// 上游协议使用的鉴权方式。
///
/// 借鉴 llm-connector：把 auth 拆成**声明**（这个协议怎么鉴权）+ **实现**
/// （用 `ServiceTarget.auth` 取出 key，按 strategy 塞进 HTTP 请求），避免
/// 每个 adapter 手写 headers 拼接。
#[derive(Debug, Clone)]
pub enum AuthStrategy {
    /// 不鉴权（本地 Ollama / 自建服务）
    None,

    /// `Authorization: Bearer <key>`（OpenAI / DeepSeek / Groq / 大多数）
    Bearer,

    /// `x-api-key: <key>`（Claude）
    XApiKey,

    /// `api-key: <key>`（Azure OpenAI）
    AzureApiKey,

    /// URL query param `?key=<key>`（Gemini 公开接口）
    QueryParam(&'static str),

    /// 自定义 header 名
    Header(&'static str),
}

// ---------------------------------------------------------------------------
// ServiceType —— 本次调用的服务类别
// ---------------------------------------------------------------------------

/// Adapter 当前能处理的服务类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceType {
    /// 非流式 chat
    Chat,
    /// 流式 chat（SSE）
    ChatStream,
}

// ---------------------------------------------------------------------------
// WebRequestData —— Adapter 产出的 HTTP 请求数据
// ---------------------------------------------------------------------------

/// 构造好的 HTTP 请求数据。由 [`Adapter::build_chat_request`] 产出，
/// 由 relay 层的 upstream service 发出（不在 Adapter 里发 HTTP）。
#[derive(Debug, Clone)]
pub struct WebRequestData {
    pub url: String,
    pub headers: HeaderMap,
    pub payload: Value,
}

// ---------------------------------------------------------------------------
// Adapter trait
// ---------------------------------------------------------------------------

/// 一家上游协议的 **协议转换器**。
///
/// # 设计约束
///
/// - 实现类型必须是 **ZST**（零大小 struct），调用方永不实例化
/// - 所有方法都是 associated fn（无 `&self`），通过 [`AdapterDispatcher`] 静态分派
/// - 使用 Rust 2024 edition 的 `async fn in trait`（不需要 `async_trait` macro）
pub trait Adapter {
    // ─────────────── 协议元数据（const） ───────────────

    /// 对应的 [`AdapterKind`] 枚举变体。
    const KIND: AdapterKind;

    /// 默认的 API Key 环境变量名。生产从 DB 读，这是 dev 环境 fallback。
    const DEFAULT_API_KEY_ENV_NAME: Option<&'static str>;

    // ─────────────── 元数据（可覆盖） ───────────────

    /// 协议默认鉴权方式。
    ///
    /// 默认：有 `DEFAULT_API_KEY_ENV_NAME` → `AuthData::from_env(env)`；否则 `AuthData::None`。
    fn default_auth() -> AuthData {
        match Self::DEFAULT_API_KEY_ENV_NAME {
            Some(env) => AuthData::from_env(env),
            None => AuthData::None,
        }
    }

    /// 协议默认端点。返 `None` 表示无事实标准（如 OpenAICompat / Azure / 自建 Ollama）。
    fn default_endpoint() -> Option<Endpoint> {
        None
    }

    /// 协议能力声明。
    fn capabilities() -> Capabilities;

    /// 协议鉴权策略（给 relay 层构造 headers 用）。
    fn auth_strategy() -> AuthStrategy;

    /// 协议级计费系数（Claude cache write 1.25x / OpenAI cache read 0.5x 等）。
    ///
    /// 默认：`CostProfile::default()`（不支持 prompt cache，两个倍率都是 1.0）。
    fn cost_profile() -> CostProfile {
        CostProfile::default()
    }

    // ─────────────── 请求验证（前置能力校验） ───────────────

    /// 请求进 Adapter 前的能力校验。
    ///
    /// 基于 [`Self::capabilities()`] 检查请求是否含有协议不支持的字段，
    /// 不支持则返 [`AdapterError::Unsupported`]——给客户端清晰错误，避免调上游才失败。
    ///
    /// 默认实现：通用规则（见 [`validate_with_capabilities`]）。特殊协议可覆盖。
    fn validate_chat_request(req: &ChatRequest) -> AdapterResult<()> {
        validate_with_capabilities(Self::KIND.as_str(), &Self::capabilities(), req)
    }

    // ─────────────── Chat 核心三件事 ───────────────

    /// 把 canonical [`ChatRequest`] + [`ServiceTarget`] 组装成 HTTP 请求数据。
    fn build_chat_request(
        target: &ServiceTarget,
        service: ServiceType,
        req: &ChatRequest,
    ) -> AdapterResult<WebRequestData>;

    /// 把上游非流式响应 body 解析成 canonical [`ChatResponse`]。
    fn parse_chat_response(target: &ServiceTarget, body: Bytes) -> AdapterResult<ChatResponse>;

    /// 解析上游 SSE 的**单个**原始事件（已去 `data: ` 前缀的 JSON 行）。
    ///
    /// 返回 `Vec<ChatStreamEvent>`——**一个上游 chunk 可对应多个 canonical 事件**：
    /// - 空 Vec：应被忽略（例如 `: keep-alive` 注释、`[DONE]` 终止标记）。
    /// - 多个：上游把 content + finish_reason 打包在同一 chunk（Ollama/Mistral 风格），
    ///   或并行 tool_calls 在同一 chunk 同时发出多个时，要 emit 多个事件。
    fn parse_chat_stream_event(
        target: &ServiceTarget,
        raw: &str,
    ) -> AdapterResult<Vec<ChatStreamEvent>>;

    // ─────────────── 运维 / 管理面 ───────────────

    /// 向上游拉取可用的 model 列表（`/v1/models` 端点 / admin 测试连通性用）。
    ///
    /// 默认返 `Unsupported`。子类按需实现。
    ///
    /// 使用 Rust 2024 edition 的 RPIT（Return Position Impl Trait），显式声明 `+ Send`
    /// 避免 auto-trait 不确定的风险（relay 要在 tokio multi-thread runtime 跨线程执行）。
    fn fetch_model_names(
        _target: &ServiceTarget,
        _http: &reqwest::Client,
    ) -> impl Future<Output = AdapterResult<Vec<String>>> + Send {
        async {
            Err(AdapterError::Unsupported {
                adapter: Self::KIND.as_str(),
                feature: "fetch_model_names",
            })
        }
    }

    /// HTTP 非 2xx 错误映射。默认包成 `UpstreamStatus`。
    fn map_error(status: u16, body: &[u8]) -> AdapterError {
        AdapterError::UpstreamStatus {
            status,
            message: String::from_utf8_lossy(body).to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// validate_with_capabilities —— 基于 Capabilities 的请求预校验
// ---------------------------------------------------------------------------

/// 按 [`Capabilities`] 规则校验 [`ChatRequest`]。
///
/// Adapter 默认 [`Adapter::validate_chat_request`] 调用此函数。检查项：
///
/// - `tool_choice` 字段与 `tool_choice` 能力
/// - `n > 1` 与 `multi_choice` 能力
/// - 多模态 content parts 与 `multimodal_input` 能力
/// - `response_format` 与 `response_format` 能力
/// - `reasoning_effort` 与 `reasoning` 能力
/// - `parallel_tool_calls` 与 `parallel_tool_calls` 能力
pub fn validate_with_capabilities(
    adapter: &'static str,
    caps: &Capabilities,
    req: &ChatRequest,
) -> AdapterResult<()> {
    if req.tool_choice.is_some() && !caps.tool_choice {
        return Err(AdapterError::Unsupported {
            adapter,
            feature: "tool_choice",
        });
    }
    if matches!(req.n, Some(n) if n > 1) && !caps.multi_choice {
        return Err(AdapterError::Unsupported {
            adapter,
            feature: "n>1",
        });
    }
    if req.response_format.is_some() && !caps.response_format {
        return Err(AdapterError::Unsupported {
            adapter,
            feature: "response_format",
        });
    }
    if req.parallel_tool_calls.is_some() && !caps.parallel_tool_calls {
        return Err(AdapterError::Unsupported {
            adapter,
            feature: "parallel_tool_calls",
        });
    }
    if req.reasoning_effort.is_some() && !caps.reasoning {
        return Err(AdapterError::Unsupported {
            adapter,
            feature: "reasoning_effort",
        });
    }
    // 多模态 content parts 的检查留给 adapter 按需实现
    // （canonical 扁平 ChatRequest 的 content 是 MessageContent::{Text, Parts}）
    Ok(())
}
