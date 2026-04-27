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
pub mod endpoint_scope;
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
pub use endpoint_scope::{EndpointScope, UnknownEndpointScope, parse_json_scopes};
pub use kind::AdapterKind;

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
    /// 非流式 chat（OpenAI `/v1/chat/completions` / Claude `/v1/messages` /
    /// Gemini `:generateContent`）
    Chat,
    /// 流式 chat（SSE）
    ChatStream,
    /// 非流式 Responses（OpenAI `/v1/responses`）
    Responses,
    /// 流式 Responses（SSE）
    ResponsesStream,
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

/// 一家上游协议的 **协议转换器**
pub trait Adapter {
    // ─────────────── 协议元数据（const） ───────────────

    /// 对应的 [`AdapterKind`] 枚举变体。
    const KIND: AdapterKind;

    // ─────────────── 元数据（可覆盖） ───────────────

    /// 协议默认鉴权方式。生产链路从 `channel_account.credentials` 拿 key 注入；
    /// 这里仅给单元测试或独立调用 Adapter 时一个 fallback。
    fn default_auth() -> AuthData {
        AuthData::None
    }

    /// 协议默认端点。返 `None` 表示无事实标准（如 OpenAICompat / Azure / 自建 Ollama）。
    fn default_endpoint() -> Option<Endpoint> {
        None
    }

    /// 协议鉴权策略（给 relay 层构造 headers 用）。
    fn auth_strategy() -> AuthStrategy;

    /// 协议级计费系数（Claude cache write 1.25x / OpenAI cache read 0.5x 等）。
    ///
    /// 默认：`CostProfile::default()`（不支持 prompt cache，两个倍率都是 1.0）。
    fn cost_profile() -> CostProfile {
        CostProfile::default()
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
