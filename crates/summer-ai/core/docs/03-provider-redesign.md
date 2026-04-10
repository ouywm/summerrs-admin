# 03 — Provider 适配器体系重设计

## 一、现状 vs 目标对比

### 现状：单一巨型 trait

```
ProviderAdapter
├── build_request()              ─┐
├── parse_response()              │ Chat Completion
├── parse_stream()               ─┘
├── build_responses_request()    ─┐ Responses API
├── responses_runtime_mode()     ─┘
├── build_embeddings_request()   ─┐ Embeddings
├── parse_embeddings_response()  ─┘
└── parse_error()                ── 错误处理
```

**问题**：
- Anthropic 不支持 Embeddings，但被迫"实现"一个返回 `Err` 的默认方法
- 新增 Audio/Image/Rerank endpoint 必须修改 trait + 所有 4 个 impl
- 调用方无法在编译期知道某个 provider 是否支持某个 endpoint

### 目标：分层 trait 体系

```
Provider (base trait)
├── kind() → ProviderKind
└── parse_error() → ProviderErrorInfo

ChatProvider : Provider
├── build_chat_request()
├── parse_chat_response()
└── parse_chat_stream()

EmbeddingProvider : Provider
├── build_embedding_request()
└── parse_embedding_response()

ResponsesProvider : Provider
├── runtime_mode() → ResponsesRuntimeMode
└── build_responses_request()
```

## 二、各 Provider 能力矩阵

| Provider | Chat | Embedding | Responses | Audio | Image | Rerank |
|----------|------|-----------|-----------|-------|-------|--------|
| OpenAI | ✅ | ✅ | ✅ (Native) | ✅ | ✅ | ❌ |
| Anthropic | ✅ | ❌ | ✅ (ChatBridge) | ❌ | ❌ | ❌ |
| Azure OpenAI | ✅ | ✅ | ✅ (Native/Legacy) | ✅ | ✅ | ❌ |
| Gemini | ✅ | ✅ | ✅ (ChatBridge) | ❌ | ❌ | ❌ |
| DeepSeek | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ |
| Groq | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ |
| Ollama | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ |
| Cohere | ✅ | ✅ | ✅ | ❌ | ❌ | ✅ |
| 其他 OAI 兼容 | ✅ | ✅ | ✅ | ? | ? | ? |

## 三、实现细节

### 3.1 OpenAI Adapter — 实现所有 trait

```rust
pub struct OpenAiAdapter;

impl Provider for OpenAiAdapter {
    fn kind(&self) -> ProviderKind { ProviderKind::OpenAi }
    fn parse_error(&self, status: u16, _headers: &HeaderMap, body: &[u8]) -> ProviderErrorInfo {
        parse_openai_compatible_error(status, body)
    }
}

impl ChatProvider for OpenAiAdapter {
    fn build_chat_request(...) -> Result<RequestBuilder> { /* 现有逻辑 */ }
    fn parse_chat_response(...) -> Result<ChatCompletionResponse> { /* 现有逻辑 */ }
    fn parse_chat_stream(...) -> Result<BoxStream<...>> { /* 现有逻辑 */ }
}

impl EmbeddingProvider for OpenAiAdapter {
    fn build_embedding_request(...) -> Result<RequestBuilder> { /* 现有逻辑 */ }
    fn parse_embedding_response(...) -> Result<EmbeddingResponse> { /* 直接反序列化 */ }
}

impl ResponsesProvider for OpenAiAdapter {
    fn runtime_mode(&self) -> ResponsesRuntimeMode { ResponsesRuntimeMode::Native }
    fn build_responses_request(...) -> Result<RequestBuilder> { /* 现有逻辑 */ }
}
```

### 3.2 Anthropic Adapter — 只实现支持的 trait

```rust
pub struct AnthropicAdapter;

impl Provider for AnthropicAdapter { ... }

impl ChatProvider for AnthropicAdapter {
    // 现有的完整 Anthropic ↔ OpenAI 转换逻辑
    // 但内部使用 convert 模块的共享函数
}

impl ResponsesProvider for AnthropicAdapter {
    fn runtime_mode(&self) -> ResponsesRuntimeMode { ResponsesRuntimeMode::ChatBridge }
    fn build_responses_request(...) -> Result<RequestBuilder> {
        // 将 responses 请求转为 chat 请求，再调用 self.build_chat_request
    }
}

// ❌ 不实现 EmbeddingProvider — 编译期就知道 Anthropic 不支持 embeddings
```

### 3.3 Gemini Adapter — Chat + Embedding + Responses

```rust
pub struct GeminiAdapter;

impl Provider for GeminiAdapter { ... }
impl ChatProvider for GeminiAdapter { ... }
impl EmbeddingProvider for GeminiAdapter { ... }

impl ResponsesProvider for GeminiAdapter {
    fn runtime_mode(&self) -> ResponsesRuntimeMode { ResponsesRuntimeMode::ChatBridge }
    fn build_responses_request(...) -> Result<RequestBuilder> {
        // responses → chat 桥接
    }
}
```

## 四、Anthropic Adapter 内部重构

当前 `anthropic.rs` 有 920 行，需要拆分：

### 4.1 anthropic/convert.rs — 协议转换

```rust
/// OpenAI messages → Anthropic messages
pub fn convert_messages(messages: &[Message]) -> Vec<AnthropicMessage> { ... }

/// 提取 system prompt
pub fn collect_system_prompt(messages: &[Message]) -> Option<String> { ... }

/// OpenAI tools → Anthropic tools
pub fn convert_tools(tools: Option<&Vec<Tool>>) -> Option<Vec<AnthropicTool>> { ... }

/// OpenAI tool_choice → Anthropic tool_choice
pub fn convert_tool_choice(tool_choice: Option<&Value>) -> Option<Value> { ... }

/// Anthropic response → OpenAI ChatCompletionResponse
pub fn convert_response(response: AnthropicResponse) -> ChatCompletionResponse { ... }

/// Anthropic usage → OpenAI Usage
pub fn usage_from_anthropic(usage: AnthropicUsage) -> Usage { ... }
```

### 4.2 anthropic/stream.rs — SSE 状态机

```rust
/// Anthropic SSE 事件映射器
pub struct AnthropicStreamMapper;

impl StreamEventMapper for AnthropicStreamMapper {
    type State = AnthropicStreamState;

    fn map_event(
        &self,
        state: &mut Self::State,
        event: SseEvent,
    ) -> Vec<Result<ChatCompletionChunk>> {
        // 现有的 message_start, content_block_start, content_block_delta,
        // message_delta, error, message_stop 处理逻辑
    }
}
```

### 4.3 anthropic/mod.rs — 简洁的 adapter 入口

```rust
mod convert;
mod stream;

pub struct AnthropicAdapter;

impl ChatProvider for AnthropicAdapter {
    fn build_chat_request(...) -> Result<RequestBuilder> {
        let body = convert::build_anthropic_body(req, actual_model)?;
        Ok(client.post(convert::build_url(base_url))
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body))
    }

    fn parse_chat_response(&self, body: Bytes, model: &str) -> Result<ChatCompletionResponse> {
        let response: AnthropicResponse = serde_json::from_slice(&body)?;
        Ok(convert::to_openai_response(response))
    }

    fn parse_chat_stream(&self, response: Response, model: &str) -> Result<BoxStream<...>> {
        Ok(mapped_chunk_stream(response, stream::AnthropicStreamMapper))
    }
}
```

**效果**：adapter 入口文件从 920 行缩减到 ~100 行。

## 五、OpenAI 兼容 Provider 的复用

大量 provider（DeepSeek, Groq, Mistral, SiliconFlow, vLLM, 等等）共享 OpenAI 格式。
当前用 `get_adapter()` 的 `_ => &OPENAI` 实现，重构后更优雅：

```rust
/// OpenAI 兼容 provider 只需要不同的 ProviderKind
pub fn get_chat_provider(kind: ProviderKind) -> &'static dyn ChatProvider {
    static ANTHROPIC: AnthropicAdapter = AnthropicAdapter;
    static AZURE: AzureOpenAiAdapter = AzureOpenAiAdapter;
    static GEMINI: GeminiAdapter = GeminiAdapter;
    static OPENAI: OpenAiAdapter = OpenAiAdapter;

    match kind {
        ProviderKind::Anthropic => &ANTHROPIC,
        ProviderKind::AzureOpenAi => &AZURE,
        ProviderKind::Gemini => &GEMINI,
        // 所有 OpenAI 兼容 provider 共享同一个 adapter
        _ => &OPENAI,
    }
}

pub fn get_embedding_provider(kind: ProviderKind) -> Option<&'static dyn EmbeddingProvider> {
    static GEMINI: GeminiAdapter = GeminiAdapter;
    static OPENAI: OpenAiAdapter = OpenAiAdapter;

    match kind {
        ProviderKind::Anthropic => None,  // Anthropic 不支持 embeddings
        ProviderKind::Gemini => Some(&GEMINI),
        _ => Some(&OPENAI),
    }
}
```

## 六、迁移路径

| 阶段 | 动作 | 影响 |
|------|------|------|
| Phase 1 | 新增 `ProviderKind` enum + `kind.rs` | 无破坏性变更 |
| Phase 2 | 新增 `Provider`/`ChatProvider`/`EmbeddingProvider`/`ResponsesProvider` trait | 无破坏性变更 |
| Phase 3 | 为 4 个 adapter 实现新 trait | 无破坏性变更 |
| Phase 4 | 提取 `convert/` 和 `stream/` 共享模块 | adapter 内部重构 |
| Phase 5 | hub 迁移到新 trait | 需要 hub 配合 |
| Phase 6 | 删除旧 `ProviderAdapter` trait | 破坏性变更（仅内部） |
