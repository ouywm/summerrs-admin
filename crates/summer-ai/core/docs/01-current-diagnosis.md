# 01 — 现状诊断：summer-ai-core 问题分析

## 一、当前架构概览

```
summer-ai-core/src/
├── lib.rs                    # 仅导出 provider + types
├── provider/
│   ├── mod.rs                # ProviderAdapter trait + 所有 provider 元数据 + 错误类型
│   ├── openai.rs             # OpenAI 适配器
│   ├── anthropic.rs          # Anthropic 适配器 (~920 行)
│   ├── azure.rs              # Azure OpenAI 适配器
│   └── gemini.rs             # Gemini 适配器 (~1260 行)
└── types/
    ├── mod.rs                # 子模块导出
    ├── chat.rs               # ChatCompletion 请求/响应
    ├── common.rs             # Message, Usage, Tool 等共用类型
    ├── completion.rs         # (Legacy completions)
    ├── embedding.rs          # Embedding 请求/响应
    ├── error.rs              # OpenAI 兼容错误响应
    ├── responses.rs          # /v1/responses 类型
    ├── sse_parser.rs         # SSE 字节流解析器
    ├── audio.rs              # Audio 类型
    ├── batch.rs              # Batch 类型
    ├── file.rs               # File 类型
    ├── image.rs              # Image 类型
    ├── model.rs              # Model 列表类型
    └── moderation.rs         # Moderation 类型
```

### 依赖关系
```
summer-ai-core
├── anyhow
├── async-stream
├── bytes
├── futures
├── reqwest (json, stream)
├── schemars
├── serde / serde_json
├── summer-common
├── summer-web (可选, feature = "axum")
└── tracing
```

---

## 二、核心问题诊断

### 问题 1: ProviderAdapter trait 是「超级 trait」— 职责过重

当前 `ProviderAdapter` trait 包含 **8 个方法**，涵盖了：
- Chat Completion 请求构建 (`build_request`)
- Chat Completion 响应解析 (`parse_response`)
- Chat Completion 流解析 (`parse_stream`)
- Responses API 请求构建 (`build_responses_request`)
- Responses API 运行时模式 (`responses_runtime_mode`)
- Embeddings 请求构建 (`build_embeddings_request`)
- Embeddings 响应解析 (`parse_embeddings_response`)
- 错误解析 (`parse_error`)

**问题**：
- 每个 provider 都必须实现/关注所有 endpoint，即使它不支持某些
- 新增 endpoint（如 Audio, Image, Rerank）需要修改 trait 和所有 impl
- 违反了接口隔离原则 (ISP)

```rust
// 当前：一个巨型 trait
pub trait ProviderAdapter: Send + Sync {
    fn build_request(...) -> Result<RequestBuilder>;
    fn parse_response(...) -> Result<ChatCompletionResponse>;
    fn parse_stream(...) -> Result<BoxStream<...>>;
    fn build_responses_request(...) -> Result<RequestBuilder> { Err(...) }  // 默认不支持
    fn responses_runtime_mode(...) -> ResponsesRuntimeMode { Native }
    fn build_embeddings_request(...) -> Result<RequestBuilder> { Err(...) }  // 默认不支持
    fn parse_embeddings_response(...) -> Result<EmbeddingResponse> { ... }
    fn parse_error(...) -> ProviderErrorInfo { ... }
}
```

### 问题 2: Provider 元数据和适配器逻辑混在一起

`provider/mod.rs` 有 **830 行**，既包含：
- `ProviderMeta` 结构体和静态注册表 (provider_meta)
- `ProviderAdapter` trait 定义
- `get_adapter()` 全局工厂
- `provider_scope_allowlist()` 能力查询
- 错误类型 (`ProviderErrorKind`, `ProviderErrorInfo`, `ProviderStreamError`)
- 辅助函数 (`responses_request_to_chat_request`, `merge_extra_body_fields`)
- 大量测试

应该分离为：元数据注册、trait 定义、错误类型、适配器工厂。

### 问题 3: 协议转换逻辑分散在各个 adapter 中

Anthropic adapter (~920行) 和 Gemini adapter (~1260行) 包含大量的：
- 请求格式转换代码（OpenAI → 原生格式）
- 响应格式转换代码（原生格式 → OpenAI）
- SSE 流解析和状态机
- 工具调用转换
- 图片格式转换

这些转换逻辑：
- **与 adapter 的构建/解析职责不同**（转换 ≠ 适配）
- 每个 adapter 内部大量重复（extract_text_segments, parse_function_arguments, serialize_arguments 在 anthropic 和 gemini 中各写了一份）
- 测试文件巨大但难以复用

### 问题 4: channel_type 魔数贯穿全部代码

```rust
match channel_type {
    1 => 0,    // OpenAI
    3 => 1,    // Anthropic
    14 => 2,   // Azure
    // ...
    _ => return None,
}
```

数字 `1`, `3`, `14`, `24` 没有语义，分散在多处使用，容易出错。应该用 enum 或常量。

### 问题 5: types 模块是 OpenAI 类型的 1:1 映射

`types/` 下的结构体完全是 OpenAI API 的镜像：
- `ChatCompletionRequest` / `ChatCompletionResponse` — OpenAI 格式
- `ResponsesRequest` / `ResponsesResponse` — OpenAI 格式
- `EmbeddingRequest` / `EmbeddingResponse` — OpenAI 格式

**问题**：
- 如果我们要做真正的多 provider 统一层，应该有**自己的标准化类型**
- 当前 Anthropic/Gemini adapter 的工作是：自己的 internal 类型 → OpenAI 类型
- 没有「summer-ai 标准类型」这一层抽象

### 问题 6: SSE 流解析每个 provider 重复实现

OpenAI, Anthropic, Gemini 三个 adapter 都有各自的 SSE 解析逻辑：
```rust
// 三份几乎相同的样板代码：
while let Some(chunk_result) = byte_stream.next().await {
    let chunk = match chunk_result { ... };
    let events = match parser.feed(&chunk) { ... };
    for event_text in events { ... }
}
```

SSE 字节流 → 事件流的转换是通用的，应该提取为共享基础设施。

### 问题 7: responses_request_to_chat_request 是跨层泄漏

这个函数把 Responses API 请求转成 Chat Completion 请求，让不支持 /v1/responses 的 provider 也能处理。但它：
- 定义在 `provider/mod.rs`（core 层）
- 包含了业务决策（如何桥接 responses → chat）
- 应该属于 hub 的应用层逻辑，不是 core 的职责

### 问题 8: 没有中间件/管道概念

当前 adapter 是单一的「输入→输出」转换，没有插入横切关注点的能力：
- 无法在 core 层做 retry
- 无法做请求/响应日志
- 无法做 token 计数
- 无法做 fallback
- 无法做 circuit breaking

这些都在 hub 层实现，但部分（如 retry、token 计数）是通用逻辑。

---

## 三、质量评估

| 维度 | 评分 | 说明 |
|------|------|------|
| 功能完整性 | ★★★★☆ | 支持 4 个 provider、5 种 endpoint，实用 |
| 测试覆盖 | ★★★★★ | 测试非常充分，每个 adapter 有完整的单元测试 |
| 代码质量 | ★★★☆☆ | 单个文件质量高，但模块间职责划分不清 |
| 可扩展性 | ★★☆☆☆ | 新增 provider 需要实现巨型 trait，新增 endpoint 需要修改所有 provider |
| 可维护性 | ★★★☆☆ | 大文件难以维护（gemini.rs 1260行），转换逻辑重复 |
| 架构设计 | ★★☆☆☆ | 缺少分层、缺少抽象层次、职责边界模糊 |

---

## 四、总结：需要解决的核心问题

1. **ProviderAdapter trait 拆分** — 按 endpoint 拆为多个 trait
2. **Provider 元数据独立管理** — 与适配器逻辑分离
3. **协议转换逻辑复用** — 提取 anthropic/gemini 的共用转换代码
4. **channel_type 类型化** — 引入 `ProviderKind` enum
5. **标准化类型系统** — 建立 summer-ai 自有类型，而非直接用 OpenAI 类型
6. **SSE 流解析统一** — 共享 SSE → 事件流的基础设施
7. **responses 桥接上移** — 将业务桥接逻辑移到 hub 层
8. **中间件管道** — 为 core 增加可组合的处理管道
