# 02 — 目标架构：summer-ai-core 重构方案

## 一、设计原则

1. **接口隔离** — 每个 endpoint 一个 trait，provider 按需实现
2. **元数据驱动** — Provider 能力通过注册表声明，运行时查询
3. **零成本抽象** — 静态分发优先，trait object 仅在必要时使用
4. **协议转换独立** — OpenAI ↔ Native 格式转换是独立模块
5. **SSE 流统一** — 通用 SSE 解析 + per-provider 事件映射

## 二、目标目录结构

```
summer-ai-core/src/
├── lib.rs                          # 公开 API surface
│
├── provider/                       # Provider 适配器体系
│   ├── mod.rs                      # trait 定义 + 工厂函数
│   ├── kind.rs                     # ProviderKind enum (替代 channel_type 魔数)
│   ├── registry.rs                 # Provider 元数据注册表
│   ├── error.rs                    # Provider 错误类型
│   ├── openai/
│   │   ├── mod.rs                  # OpenAI adapter (所有 endpoint)
│   │   └── tests.rs
│   ├── anthropic/
│   │   ├── mod.rs                  # Anthropic adapter
│   │   ├── convert.rs             # Anthropic ↔ OpenAI 协议转换
│   │   ├── stream.rs              # Anthropic SSE 状态机
│   │   └── tests.rs
│   ├── gemini/
│   │   ├── mod.rs                  # Gemini adapter
│   │   ├── convert.rs             # Gemini ↔ OpenAI 协议转换
│   │   ├── stream.rs              # Gemini SSE 状态机
│   │   └── tests.rs
│   └── azure/
│       ├── mod.rs                  # Azure adapter
│       └── tests.rs
│
├── types/                          # 统一类型系统
│   ├── mod.rs
│   ├── chat.rs                     # ChatCompletion 类型
│   ├── common.rs                   # Message, Usage, Tool 等共用类型
│   ├── embedding.rs                # Embedding 类型
│   ├── responses.rs                # Responses API 类型
│   ├── error.rs                    # OpenAI 兼容错误响应
│   ├── audio.rs                    # Audio 类型
│   ├── image.rs                    # Image 类型
│   ├── moderation.rs               # Moderation 类型
│   ├── rerank.rs                   # Rerank 类型 (新增)
│   └── model.rs                    # Model 列表类型
│
├── stream/                         # 统一 SSE 流处理 (新增)
│   ├── mod.rs                      # 公开 API
│   ├── sse_parser.rs               # SSE 字节流 → 事件解析器 (从 types/ 迁移)
│   ├── event_stream.rs             # 通用 SSE 事件流适配器
│   └── chunk_aggregator.rs         # 流式 chunk → 完整响应聚合器 (新增)
│
└── convert/                        # 共享协议转换工具 (新增)
    ├── mod.rs
    ├── content.rs                  # 内容块转换 (text/image/tool_result 通用)
    ├── tool.rs                     # 工具定义/调用转换
    └── message.rs                  # 消息格式转换
```

## 三、核心 trait 重设计

### 3.1 拆分后的 Provider Traits

```rust
// ===== provider/mod.rs =====

/// 所有 provider 都必须实现的基础 trait
pub trait Provider: Send + Sync + 'static {
    /// Provider 唯一标识
    fn kind(&self) -> ProviderKind;

    /// 解析 provider 特定的错误响应
    fn parse_error(&self, status: u16, headers: &HeaderMap, body: &[u8]) -> ProviderErrorInfo;
}

/// Chat Completion 能力
pub trait ChatProvider: Provider {
    fn build_chat_request(
        &self,
        client: &reqwest::Client,
        base_url: &str,
        api_key: &str,
        req: &ChatCompletionRequest,
        actual_model: &str,
    ) -> Result<reqwest::RequestBuilder>;

    fn parse_chat_response(
        &self,
        body: Bytes,
        model: &str,
    ) -> Result<ChatCompletionResponse>;

    fn parse_chat_stream(
        &self,
        response: reqwest::Response,
        model: &str,
    ) -> Result<BoxStream<'static, Result<ChatCompletionChunk>>>;
}

/// Embeddings 能力
pub trait EmbeddingProvider: Provider {
    fn build_embedding_request(
        &self,
        client: &reqwest::Client,
        base_url: &str,
        api_key: &str,
        req: &serde_json::Value,
        actual_model: &str,
    ) -> Result<reqwest::RequestBuilder>;

    fn parse_embedding_response(
        &self,
        body: Bytes,
        model: &str,
        estimated_prompt_tokens: i32,
    ) -> Result<EmbeddingResponse>;
}

/// Responses API 能力 (OpenAI 原生 /v1/responses)
pub trait ResponsesProvider: Provider {
    /// 原生支持 or 需要桥接到 Chat？
    fn runtime_mode(&self) -> ResponsesRuntimeMode;

    fn build_responses_request(
        &self,
        client: &reqwest::Client,
        base_url: &str,
        api_key: &str,
        req: &serde_json::Value,
        actual_model: &str,
    ) -> Result<reqwest::RequestBuilder>;
}

// 未来可以按需添加：
// pub trait AudioProvider: Provider { ... }
// pub trait ImageProvider: Provider { ... }
// pub trait RerankProvider: Provider { ... }
```

### 3.2 ProviderKind Enum

```rust
// ===== provider/kind.rs =====

/// 类型安全的 provider 标识，替代 channel_type: i16 魔数
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i16)]
pub enum ProviderKind {
    OpenAi = 1,
    Anthropic = 3,
    AzureOpenAi = 14,
    Baidu = 15,
    Ali = 17,
    Gemini = 24,
    Ollama = 28,
    DeepSeek = 30,
    Groq = 31,
    Mistral = 32,
    SiliconFlow = 33,
    Vllm = 34,
    Fireworks = 35,
    Together = 36,
    OpenRouter = 37,
    Moonshot = 38,
    Lingyi = 39,
    Cohere = 40,
}

impl ProviderKind {
    /// 从 channel_type 数字转换，保持向后兼容
    pub fn from_channel_type(channel_type: i16) -> Option<Self> { ... }

    /// 是否 OpenAI 兼容格式
    pub fn is_openai_compatible(&self) -> bool { ... }

    /// 获取默认 base_url
    pub fn default_base_url(&self) -> &'static str { ... }

    /// 人类可读名称
    pub fn display_name(&self) -> &'static str { ... }
}
```

### 3.3 Provider 注册表

```rust
// ===== provider/registry.rs =====

/// 运行时 provider 能力查询
pub struct ProviderRegistry;

impl ProviderRegistry {
    /// 获取 provider 实例 (静态分发)
    pub fn get(kind: ProviderKind) -> &'static dyn Provider { ... }

    /// 获取 Chat 能力 (如果支持)
    pub fn chat(kind: ProviderKind) -> Option<&'static dyn ChatProvider> { ... }

    /// 获取 Embedding 能力 (如果支持)
    pub fn embedding(kind: ProviderKind) -> Option<&'static dyn EmbeddingProvider> { ... }

    /// 获取 Responses 能力 (如果支持)
    pub fn responses(kind: ProviderKind) -> Option<&'static dyn ResponsesProvider> { ... }

    /// 查询 provider 支持的 endpoint scopes
    pub fn supported_scopes(kind: ProviderKind) -> &'static [&'static str] { ... }
}
```

## 四、SSE 流处理统一

```rust
// ===== stream/event_stream.rs =====

/// 通用 SSE 字节流 → 事件流 转换器
///
/// 提取所有 provider 共享的 SSE 解析逻辑：
/// 1. 字节流 → SseParser → 原始事件文本
/// 2. 原始事件文本 → (event_name, data) 对
///
/// 各 provider 只需实现事件映射：data → ChatCompletionChunk
pub fn sse_event_stream(
    response: reqwest::Response,
) -> BoxStream<'static, Result<SseEvent>> { ... }

pub struct SseEvent {
    pub event: Option<String>,
    pub data: String,
}

/// Provider 实现这个 trait 来定义事件映射
pub trait StreamEventMapper: Send + Sync {
    type State: Default + Send;

    /// 将一个 SSE 事件映射为零个或多个 ChatCompletionChunk
    fn map_event(
        &self,
        state: &mut Self::State,
        event: SseEvent,
    ) -> Vec<Result<ChatCompletionChunk>>;
}

/// 组合 SSE 事件流 + 事件映射器 → chunk 流
pub fn mapped_chunk_stream<M: StreamEventMapper + 'static>(
    response: reqwest::Response,
    mapper: M,
) -> BoxStream<'static, Result<ChatCompletionChunk>> { ... }
```

## 五、共享转换工具

```rust
// ===== convert/content.rs =====

/// 从 OpenAI content 数组中提取文本
pub fn extract_text_segments(content: &serde_json::Value) -> Option<String> { ... }

/// 合并文本数组为单个值
pub fn joined_text_value(texts: Vec<String>) -> serde_json::Value { ... }

/// 解析 data: URI (base64 图片)
pub fn parse_data_url(url: &str) -> Option<(&str, &str)> { ... }

// ===== convert/tool.rs =====

/// 解析工具调用参数 JSON
pub fn parse_function_arguments(arguments: &str) -> serde_json::Value { ... }

/// 序列化工具调用参数为 JSON 字符串
pub fn serialize_arguments(arguments: serde_json::Value) -> String { ... }
```

## 六、与现有代码的兼容策略

### 渐进式迁移，不破坏 hub

```rust
// Phase 1: 新增 trait，保留旧 trait
pub trait ProviderAdapter: Send + Sync { ... }  // 旧的，保留

pub trait Provider: Send + Sync { ... }          // 新的
pub trait ChatProvider: Provider { ... }         // 新的

// Phase 2: 为旧 adapter 实现新 trait (blanket impl 或手动)
// Phase 3: hub 逐步迁移到新 trait
// Phase 4: 删除旧 trait
```

### 重导出保持 API 稳定

```rust
// lib.rs — 保持现有公开 API
pub mod provider;
pub mod types;
pub mod stream;    // 新增
pub mod convert;   // 新增

// 重导出常用类型
pub use provider::{ProviderKind, ProviderRegistry};
pub use provider::{ChatProvider, EmbeddingProvider, ResponsesProvider};
```

## 七、设计决策记录

| 决策 | 选择 | 替代方案 | 原因 |
|------|------|---------|------|
| trait 拆分粒度 | 按 endpoint 拆 (Chat/Embedding/Responses) | 按能力拆 (Build/Parse/Stream) | endpoint 对齐 OpenAI API 直觉更清晰 |
| Provider 实例生命周期 | 全局静态 (`&'static`) | Arc 共享 | 当前 adapter 无状态，静态实例零开销 |
| ProviderKind 底层类型 | `#[repr(i16)]` enum | newtype `struct ProviderKind(i16)` | enum 可穷举，编译器帮检查 |
| SSE 流抽象 | event stream + mapper trait | 完全共享 parse_stream | 不同 provider 的 SSE 事件格式差异大，mapper 更灵活 |
| responses 桥接位置 | 保留在 core（ChatBridge 模式标记） | 完全移到 hub | core 标记模式让 hub 知道需要桥接，但桥接逻辑本身可以在 hub |
| 类型系统 | 保持 OpenAI-compatible 为主类型 | 自建中间类型 | 实际业务就是 OpenAI 兼容网关，中间类型是过度抽象 |
