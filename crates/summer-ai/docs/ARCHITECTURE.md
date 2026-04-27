# summer-ai 架构设计（契约）

> 本文档定义 **架构层契约**：crate 切分、trait 抽象、数据流、扩展点。
> 一旦定稿，后续所有代码提交必须遵守。改动需提案 + 变更日志。

更新日期：2026-04-19

---

## 1. 定位

**summer-ai 是 [NewAPI](https://github.com/QuantumNous/new-api) 的 Rust 重写，吸收多家参考项目精髓**。

- **对标 NewAPI**：AI 网关/代理，40+ 上游 provider 聚合、多入口协议（OpenAI/Claude/Gemini）、用户体系、API Token 池、三阶段计费、限流、admin 后台、异步任务（Midjourney/Suno/视频）
- **Rust 重写的原因**：
  - 性能：零 GC、静态派发、更低内存占用
  - 类型安全：NewAPI 的 `Adaptor` interface 大量用 `any`，类型断言到处都是；Rust generic + trait 让请求/响应类型编译期可控
  - Trait 拆分：NewAPI 一个胖 interface 13 方法（Gemini Adapter 也要硬写 `ConvertAudioRequest`）；Rust 用子 trait（`Adapter` + `EmbedAdapter` + `ImageAdapter` + `TaskAdapter`），可选能力编译期约束
- **吸收的精髓**：
  - **NewAPI**：整体架构（多入口协议、channel/key pool、三阶段计费、TaskAdapter、嵌套复用）
  - **genai**：ZST Adapter + Dispatcher + 19 家 wire-format 转换算法
  - **ironclaw**：retry/failover/breaker/cache 保护层
  - **llm-connector**：Protocol 纯转换 + Provider 带 HTTP 双层、auth_strategy 声明化、能力前置校验

**不做**（NewAPI 有但不 v1）：
- 前端 React 管理界面（v1 用现有 summer-admin 自带的管理页，后续再做专属 AI 管理页）
- WebAuthn/Passkey 登录（复用 summer-auth 的基础登录）
- OAuth 第三方登录的 AI 部分（先 API Key，后期再补）

---

## 2. Crate 切分

```
summer-ai/                    workspace root
├── Cargo.toml                [workspace] members 聚合
├── src/lib.rs                SummerAiPlugin（聚合 5 个 sub-plugin）
│
├── core/                     【协议层】与框架无关，纯 Rust
│   ├── types/                canonical 类型（OpenAI 兼容）
│   ├── resolver/             运行时上下文（Endpoint / AuthData / ServiceTarget）
│   ├── adapter/              Adapter trait + Dispatcher + 19 实现
│   ├── webc/                 HTTP + SSE 解析工具
│   ├── cost/                 价格计算（来自 ironclaw costs.rs）
│   └── error.rs
│
├── model/                    【数据层】SeaORM Entity，按域分子目录
│   └── src/entity/
│       ├── channels/         channel / channel_account / price / probe / routing
│       ├── requests/         log / request / execution / retry / trace
│       ├── platform/         api_token / session / rbac / vendor / config
│       ├── billing/          topup / transaction / quota / group_ratio / ...
│       ├── alerts/           alert_rule / event / silence / daily_stats
│       ├── guardrails/       guardrail_rule / violation / config
│       ├── tenancy/          organization / team / project / sso (后期)
│       └── ...
│
├── relay/                    【运行时】Handler + Service + 鉴权 + Job
│   ├── auth/                 API Token 鉴权 middleware
│   ├── router/               按 **入口协议** 分子目录（多入口支持）
│   │   ├── openai/           /v1/chat, /v1/embeddings, /v1/images, /v1/audio, /v1/responses, /v1/rerank, /v1/models
│   │   ├── claude/           /v1/messages (Anthropic 原生入口)
│   │   └── gemini/           /v1beta/models/*/generateContent (Gemini 原生入口)
│   ├── convert/              **入口/出口格式转换层**（§4 多入口协议）
│   │   ├── ingress/          client wire → canonical
│   │   │   ├── openai.rs     identity（我们的 canonical 就是 OpenAI-flat）
│   │   │   ├── claude.rs     ClaudeMessagesRequest → canonical ChatRequest
│   │   │   └── gemini.rs     GeminiGenerateContentRequest → canonical ChatRequest
│   │   └── egress/           canonical → client wire
│   │       ├── openai.rs     identity
│   │       ├── claude.rs     canonical ChatResponse / StreamEvent → Claude wire
│   │       └── gemini.rs     canonical ChatResponse / StreamEvent → Gemini wire
│   ├── service/              chat / embeddings / responses / log / tracking
│   └── job/                  daily_stats / alert_scan
│
├── billing/                  【计费】三阶段原子扣费（Reserve → Settle → Refund）
│   ├── service/engine/       扣费引擎
│   └── ...
│
└── admin/                    【后台】CRUD（复用 summer-admin）
    ├── router/               channel / price / vendor / request / token / ...
    └── service/
```

**为什么这样切**：

| 切分 | 理由 |
|---|---|
| `core` 独立且零框架依赖 | 可以单测，可以给其他 crate（甚至非 summer 项目）复用 |
| `model` 独立 | Entity 纯 SeaORM，`admin/relay/billing` 都依赖它，不允许反依赖 |
| `relay` 和 `admin` 并列 | 前者处理 `/v1/*`（对外 AI API），后者处理 `/admin/*`（后台），职责互斥 |
| `billing` 独立 | 扣费/退款有事务一致性要求，独立测试 |
| **不要 hub** | 分支的 hub 是 DDD 尝试，已标记清理。我们跳过它 |

**依赖图**：

```
                   ┌──────────┐
                   │   app    │
                   └─────┬────┘
                         │
              ┌──────────▼──────────┐
              │   summer-ai (lib)   │── SummerAiPlugin
              └───┬────┬────┬────┬──┘
                  │    │    │    │
         ┌────────┘    │    │    └─────────┐
         │       ┌─────┘    └─────┐        │
         ▼       ▼                ▼        ▼
      relay    admin           billing    model
         │       │                │        │
         └───────┴────────┬───────┘        │
                          ▼                │
                        core  ◀────────────┘
                                           (entity → adapter)
```

**规则**：
- `core` 不依赖任何 summer-ai 子 crate
- `model` 不依赖任何 summer-ai 子 crate（除 sea-orm）
- `relay/admin/billing` 依赖 `core` + `model`，彼此不依赖
- 主 `summer-ai` lib.rs 依赖全部子 crate，负责 Plugin 聚合

---

## 3. 核心 trait 契约

### 3.1 Adapter trait（**贴合业务**，不是"为精简而精简"）

**评判标准**：每个方法保留与否，取决于 **relay 业务是否需要**——不是"trait 看起来多干净"。genai 是 SDK（静态单上游），我们是 relay（动态多上游），两边业务不同，trait 要裁出不同形状。

```rust
/// 一家上游协议的转换器。实现类型必须是 ZST。
pub trait Adapter {
    // ─────── 协议元数据（const + 默认实现） ───────

    /// 对应的 AdapterKind 枚举变体（编译期常量）
    const KIND: AdapterKind;

    /// 默认的 API Key 环境变量名。开发/测试 fallback；生产从 DB 读。
    const DEFAULT_API_KEY_ENV_NAME: Option<&'static str>;

    /// 协议默认鉴权方式。当 ServiceTarget.auth == AuthData::None 时可选用。
    fn default_auth() -> AuthData {
        match Self::DEFAULT_API_KEY_ENV_NAME {
            Some(env) => AuthData::from_env(env),
            None => AuthData::None,
        }
    }

    /// 协议默认端点（如 OpenAI `https://api.openai.com/v1/`）。
    /// 返 `None` 表示此协议没有"事实标准"地址（如 OpenAICompat / Azure）。
    fn default_endpoint() -> Option<Endpoint> { None }

    /// 协议能力声明（channel 可通过 `ServiceTarget.capabilities_override` 收窄）。
    fn capabilities() -> Capabilities;

    /// 协议级计费系数（Anthropic cache write 1.25x, OpenAI cache read 0.5x 等）。
    fn cost_profile() -> CostProfile { CostProfile::default() }

    // ─────── Chat 核心三件事 ───────

    /// 把 canonical ChatRequest + ServiceTarget 组装成 HTTP 请求数据。
    fn build_chat_request(
        target: &ServiceTarget,
        service: ServiceType,
        req: &ChatRequest,
    ) -> AdapterResult<WebRequestData>;

    /// 把上游非流式响应 body 解析成 canonical ChatResponse。
    fn parse_chat_response(
        target: &ServiceTarget,
        body: Bytes,
    ) -> AdapterResult<ChatResponse>;

    /// 解析上游 SSE 的**单个**原始事件（已去 `data: ` 前缀的 JSON 行）。
    /// 返回 `Ok(None)` 表示这个事件应被忽略（如 `: keep-alive` 注释）。
    ///
    /// **为什么不是返整条 Stream（genai 风格）**：relay 的 stream 中间件
    /// （billing 累积 usage、log 记录首字延迟、guardrail 过滤内容）需要
    /// 看到每个 event 流经——用 Stream 封装会把这层可观测性盖住。
    fn parse_chat_stream_event(
        target: &ServiceTarget,
        raw: &str,
    ) -> AdapterResult<Option<ChatStreamEvent>>;

    // ─────── 运维 / 管理面（异步） ───────

    /// 向上游拉取可用的 model 列表。
    ///
    /// **为什么保留**：
    /// - `/v1/models` 端点需要返回实时 model 清单（对 Ollama 这种本地
    ///   动态加载模型的协议特别重要）
    /// - admin 后台"测试 channel 连通性 + 自动发现 models"要用
    /// - channel 新增时辅助填 `models: JSONB` 字段
    ///
    /// 不支持此能力的协议可保持默认实现（返 `Unsupported` 错误）。
    async fn fetch_model_names(
        _target: &ServiceTarget,
        _http: &reqwest::Client,
    ) -> AdapterResult<Vec<String>> {
        Err(AdapterError::Unsupported {
            adapter: std::any::type_name::<Self>(),
            feature: "fetch_model_names",
        })
    }
}

/// Embedding 适配能力（独立 trait，不强制每个 Adapter 实现）。
pub trait EmbedAdapter: Adapter {
    fn build_embed_request(
        target: &ServiceTarget,
        req: &EmbedRequest,
    ) -> AdapterResult<WebRequestData>;

    fn parse_embed_response(
        target: &ServiceTarget,
        body: Bytes,
    ) -> AdapterResult<EmbedResponse>;
}
```

### 3.1.1 设计对比：genai → summer-ai 的每一处改动**都有业务理由**

| genai 做法 | summer-ai | 理由（**业务驱动**） |
|---|---|---|
| `ChatOptionsSet<'_, '_>` 视图参数 | 直接 `&ChatRequest` 扁平字段 | relay 入口 `ChatRequest` 已扁平（对齐 OpenAI wire），不需要再套一层视图 |
| `ModelIden { kind, name }` 结构体 | `ServiceTarget.actual_model: String` | `ServiceTarget.kind` 已有 kind 信息，`ModelIden` 包装是重复 |
| `Headers`（自研 HashMap 封装） | `reqwest::HeaderMap` 直传 | relay 最后要送进 reqwest，自研 Headers 还要转换一次 |
| `to_chat_stream` 返 `Stream<Item = ChatStreamEvent>` | `parse_chat_stream_event(raw) -> Option<Event>` | **关键业务理由**：stream 中途要做 billing/log/guardrail，adapter 封装 Stream 会盖住中间态 |
| `to_embed_*` 放在主 trait | 拆到 `EmbedAdapter` 子 trait | 不强制不支持 embed 的 adapter 写空方法 |
| `get_service_url` 单独方法 | 并入 `build_chat_request` | URL 拼接是 adapter 内部细节，无对外暴露必要 |
| `from_model("gpt-4o")` 推断 AdapterKind | 不要 | relay 的 adapter 由 DB `channel.channel_type` 决定，不从 model 名推 |
| `Client::exec_chat` SDK 入口 | 不要 | relay 不是 SDK，handler 直接调 `AdapterDispatcher` + reqwest |

### 3.1.2 照抄 genai 的地方（**不改**）

这些 genai 做得对，对 relay 业务也适用，**一行不改**搬过来：

- **canonical 类型**：`ChatMessage` / `MessageContent` / `ContentPart` / `Tool` / `ToolCall` / `Usage` / `FinishReason` / `ChatStreamEvent` — 拷贝即可
- **每个 adapter 的 wire-format 转换算法**：`adapter_impl.rs` / `adapter_shared.rs` 里的 JSON 组装 + 响应解析 + SSE 行解析逻辑 — 拷贝+改 trait 签名即可，核心算法不动
- **`AuthData` / `Endpoint` 数据结构** — 拷贝
- **SSE 行解析器**（`event_source_stream.rs`）— 拷贝

### 3.2 AdapterKind 枚举

**21 个变体，连续编码 1-21**：

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AdapterKind {
    // ─── 1-4: OpenAI 家族 ───
    OpenAI,         // 1 — api.openai.com /chat/completions
    OpenAIResp,     // 2 — api.openai.com /responses API
    OpenAICompat,   // 3 — 所有 OpenAI 兼容第三方（兜底变体，厂商无 native 适配时用）
    Azure,          // 4 — Azure OpenAI Service

    // ─── 5-8: Native 协议 ───
    Anthropic,      // 5
    Gemini,         // 6
    Cohere,         // 7
    Ollama,         // 8

    // ─── 9-21: OpenAI-compat 变种（有 native 细节差异） ───
    OllamaCloud,    // 9
    Groq,           // 10
    DeepSeek,       // 11
    Xai,            // 12
    Fireworks,      // 13
    Together,       // 14
    Nebius,         // 15
    Mimo,           // 16
    Zai,            // 17
    BigModel,       // 18
    Aliyun,         // 19
    Vertex,         // 20
    GithubCopilot,  // 21
}
```

### 3.2.1 AdapterKind ↔ ChannelType 关系

两个 enum 是**同一个概念的两种体现**，必须 **1:1 严格对应**：

| 类型 | 位置 | 职责 |
|---|---|---|
| `AdapterKind` | `core/src/adapter/kind.rs` | 协议维度（代码侧） |
| `ChannelType` | `model/src/entity/channels/channel.rs` | DB 列（`SMALLINT`） |

**为什么要分开**：`core` 零框架依赖（不能挂 `sea_orm::DeriveActiveEnum`），`model` 需要 SeaORM derive。

**映射实现**：放 `model` crate 里（`model → core` 单向依赖）：

```rust
impl From<ChannelType> for summer_ai_core::AdapterKind { ... }
impl From<summer_ai_core::AdapterKind> for ChannelType { ... }
```

**强制约束**：
- **变体名、顺序、编码值 100% 一致**
- **单测 round-trip**：每个变体 `ChannelType → AdapterKind → ChannelType` 恒等
- **编码值一旦上生产禁止变更**（DB 已存的 `channel_type` 列不可改语义）
- **新增变体流程**：ChannelType 加一行 + AdapterKind 加一行 + map 两边 + Dispatcher match 一行，exhaustive match 保证不漏

### 3.3 ServiceTarget（贴 DB 的运行时上下文）

```rust
pub struct ServiceTarget {
    // ─── 协议层字段（发请求用） ───
    pub endpoint: Endpoint,
    pub auth: AuthData,
    pub actual_model: String,           // 发给上游的 model 字符串
    pub extra_headers: BTreeMap<String, String>,

    // ─── 业务层字段（日志/计费/追踪） ───
    pub logical_model: String,          // 用户请求里的 model（映射前）
    pub channel_id: i64,                // 选中的渠道
    pub channel_account_id: i64,        // 选中的账号（密钥池）

    // ─── 运行时覆盖 ───
    pub capabilities_override: Option<Capabilities>,
}
```

**来源**：完全由 `ChannelRouter::resolve(logical_model, token)` 构造。Adapter 自己不负责解析 DB。

### 3.4 WebRequestData

```rust
pub struct WebRequestData {
    pub url: String,
    pub headers: reqwest::header::HeaderMap,   // 直接用 reqwest 的，避免二次转换
    pub payload: serde_json::Value,
}
```

**为什么不用 genai 的 `Headers`（HashMap 封装）**：relay 最后要送进 reqwest，genai 的 Headers 还要再转换一次，浪费。

### 3.5 Capabilities

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Capabilities {
    pub streaming: bool,
    pub tools: bool,
    pub vision: bool,
    pub reasoning: bool,
    pub response_format_json: bool,
    pub multi_choice: bool,          // n > 1
    pub prompt_caching: bool,
}
```

**谁声明**：`Adapter::capabilities()` 返协议默认；`ServiceTarget.capabilities_override` channel 级覆盖（用于关闭部分能力，比如 DeepSeek 走 OpenAI 协议但暂不支持 `vision`）。

### 3.6 CostProfile

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CostProfile {
    pub cache_write_multiplier: Decimal,   // Anthropic 5m=1.25, 1h=2.0; 其他=1.0
    pub cache_read_discount: Decimal,      // Anthropic=0.1, OpenAI=0.5, 其他=1.0
    pub supports_prompt_cache: bool,
}
```

**不存单价**——单价放 `ai.channel_model_price` 表。这里只是协议级系数。

---

## 4. 多入口协议（Ingress/Egress Converter）

### 4.1 背景：NewAPI 的"多入口协议"灵魂

NewAPI 的核心特色之一是：**客户端可以用任意协议格式请求，NewAPI 翻译后转发给任意上游**：

```
客户端（Claude SDK 格式）
  → POST /v1/messages
  → NewAPI 翻译成 OpenAI 格式
  → 转发给 DeepSeek（它只懂 OpenAI 方言）
  → 返回 OpenAI 响应
  → NewAPI 翻译回 Claude 格式
  → 返回客户端
```

支持的入口协议（**Ingress**）：
- OpenAI `/v1/chat/completions`（主流）
- Claude `/v1/messages`
- Gemini `/v1beta/models/*/generateContent`
- OpenAI `/v1/responses`（新 API）
- `/v1/embeddings` / `/v1/images/generations` / `/v1/audio/*` / `/v1/rerank`

### 4.2 NewAPI 的实现方式（及问题）

NewAPI 的 `Adaptor` interface 有 8 个 `ConvertXxxRequest` 方法，**每家上游 Adapter 都要实现全部 8 个**。例如 DeepSeek Adapter 的 `ConvertClaudeRequest` 大致是：

```go
func (a *Adaptor) ConvertClaudeRequest(c, info, claudeReq) (any, error) {
    openAIReq := service.ClaudeToOpenAIRequest(claudeReq, info)  // 先翻译
    return a.ConvertOpenAIRequest(c, info, openAIReq)             // 再走本 Adapter 的 OpenAI 路径
}
```

**问题**：
- 胖 interface：Gemini Adapter 也硬要实现 `ConvertAudioRequest`（即使 Gemini 不做音频）
- 组合爆炸：新加 1 个入口协议 × 40 家 Adapter = 40 处改动
- Go `any` 丢类型：所有方法返回 `any`，Adapter 内部到处类型断言

### 4.3 我们的做法：**N+M 解耦**

把"入口协议转换"从 Adapter 剥离到 **relay 层的独立 converter**，Adapter 永远只认 canonical。

```
┌────────────────── 客户端 ──────────────────┐
│  /v1/chat/completions    OpenAI wire       │
│  /v1/messages            Claude wire       │
│  /v1beta/models/...      Gemini wire       │
└──────────────────┬─────────────────────────┘
                   │ IngressConverter::to_canonical (relay/convert/ingress/)
                   ↓
┌────────── canonical ChatRequest ───────────┐   ← 我们的 canonical = OpenAI-flat
│  (OpenAI 官方扁平结构，system + messages +  │
│   tools + tool_choice + temperature ...)   │
└──────────────────┬─────────────────────────┘
                   │ ChannelRouter → ServiceTarget
                   ↓
┌────────── Adapter（永远只认 canonical）────┐
│  build_chat_request(target, service, req)  │
│  parse_chat_response(target, body)         │
│  parse_chat_stream_event(target, raw)      │
└──────────────────┬─────────────────────────┘
                   │ reqwest → 上游 → response/stream
                   ↓
┌────────── canonical ChatResponse ──────────┐
└──────────────────┬─────────────────────────┘
                   │ EgressConverter::from_canonical (relay/convert/egress/)
                   ↓
┌──────────── 客户端格式响应 ────────────────┐
│  (取决于入口路由决定的 wire format)          │
└─────────────────────────────────────────────┘
```

**优势**：

| 维度 | NewAPI（N×M） | 我们（N+M） |
|---|---|---|
| 加 1 个入口协议（Cohere 原生） | 40 家 Adapter 每家都要加 `ConvertCohereRequest` | 写 1 个 `CohereIngressConverter`，所有 Adapter 自动支持 |
| 加 1 个上游 Adapter（Perplexity） | Adapter 要实现 8 个 `ConvertXxxRequest` | 写 1 个 `PerplexityAdapter`（只管 canonical），所有入口协议自动支持 |
| 类型安全 | Go `any`，Adapter 内部类型断言 | Rust generic，编译期强类型 |
| 能力限定 | 胖 interface 硬写空方法 | 入口协议不支持某能力（如 Gemini 不支持 audio）→ 路由层 404，不进 Adapter |

### 4.4 IngressConverter / EgressConverter trait

```rust
// relay/src/convert/ingress/mod.rs

pub trait IngressConverter {
    /// 客户端 wire 类型（如 ClaudeMessagesRequest）
    type ClientRequest: DeserializeOwned;
    /// 客户端响应类型（如 ClaudeMessagesResponse）
    type ClientResponse: Serialize;
    /// 客户端流事件类型（如 ClaudeStreamEvent）
    type ClientStreamEvent: Serialize;

    /// 入口协议识别名（用于路由 + 日志）
    const FORMAT: IngressFormat;

    /// client wire → canonical
    fn to_canonical(req: Self::ClientRequest) -> AdapterResult<ChatRequest>;

    /// canonical → client wire
    fn from_canonical(resp: ChatResponse) -> AdapterResult<Self::ClientResponse>;

    /// canonical stream event → client stream event
    ///
    /// 返 `Ok(None)` = 过滤掉（例如 client 格式不需要这个事件）
    /// 返 `Ok(Some(evt))` = 推给客户端
    fn from_canonical_stream_event(
        event: ChatStreamEvent,
        state: &mut StreamConvertState,
    ) -> AdapterResult<Option<Self::ClientStreamEvent>>;
}
```

**实现示例**：

| 实现 | 位置 | client wire | canonical 转换 |
|---|---|---|---|
| `OpenAIIngress` | `ingress/openai.rs` | `ChatRequest` (OpenAI-flat) | **identity**（无需转换） |
| `ClaudeIngress` | `ingress/claude.rs` | `ClaudeMessagesRequest` | `messages` 结构+`system` 字段重塑 |
| `GeminiIngress` | `ingress/gemini.rs` | `GeminiGenerateContentRequest` | `contents` 数组 → `messages`，`systemInstruction` → `system` |

**入口协议 wire 类型放哪**：`core/src/types/ingress_wire/`（纯类型，无逻辑）——converter 逻辑在 `relay/convert/`。

### 4.5 Embedding / Image / Audio / Rerank 入口

这些端点 NewAPI 也算"入口协议"，但它们的 client request 本身就是 canonical（没有 3 家互转问题）。我们的做法：

- 端点各自有独立的 canonical 类型：`EmbedRequest` / `ImageRequest` / `AudioRequest` / `RerankRequest`
- 各自对应独立的 Adapter 能力子 trait：`EmbedAdapter` / `ImageAdapter` / `AudioAdapter` / `RerankAdapter`
- 每家上游选择性实现（Gemini 不实现 `AudioAdapter` 就行，**编译期**就知道不支持）

### 4.6 异步任务入口（Midjourney / Suno / 视频）

照搬 NewAPI 的 `TaskAdaptor`，作为**独立 trait**（和主 `Adapter` 平级，不继承）：

```rust
pub trait TaskAdapter {
    const KIND: AdapterKind;

    // ─── 任务生命周期 ───
    fn build_task_request(target, req) -> AdapterResult<WebRequestData>;
    fn parse_submit_response(target, body) -> AdapterResult<TaskSubmission>;
    async fn poll_task(target, http, task_id) -> AdapterResult<TaskStatus>;

    // ─── 三阶段计费（NewAPI 精髓照搬）───

    /// 提交前估算：从用户请求里提取 seconds / resolution 等作为倍率输入
    fn estimate_billing(req: &TaskRequest) -> BillingRatios;

    /// 上游返回参数可能和用户要求不同（如实际 seconds），调整倍率
    fn adjust_billing_on_submit(upstream_resp: &[u8]) -> BillingRatios;

    /// 任务完成时返回最终 quota（正数触发补扣，0 = 保持预扣）
    fn adjust_billing_on_complete(task: &Task, result: &TaskInfo) -> Decimal;
}
```

**哪些上游用 `TaskAdapter` 而不是 `Adapter`**：所有"提交后轮询"的服务——Midjourney、Stable Video、Suno、Runway、Kling、Luma 等。

### 4.7 嵌套复用（NewAPI 的 ai360/lingyiwanwu 模式）

NewAPI 里 ai360、lingyiwanwu、xinference 这些"只是 OpenAI 方言 + 不同品牌"的上游，**复用 `openai.Adaptor` 并根据 ChannelType 分派元数据**。

**我们的做法**：**DB 驱动**（比 NewAPI 的代码分派更灵活）：

- `AdapterKind::OpenAICompat` 兜底所有"OpenAI 方言但品牌不同"的上游
- `ai.channel.vendor_code` 字段 = `ai.vendor` 字典的 key，存品牌名/logo/描述
- `ai.channel.models` JSONB = 该 channel 支持的模型列表
- admin 要加新厂商，只需 DB insert，不改代码

---

## 5. 数据流

### 5.1 非流式 Chat

```
┌─────────┐  POST /v1/chat/completions   ┌─────────────────┐
│ Client  │ ──────────────────────────▶ │  relay::router  │
└─────────┘  (OpenAI wire format)        └────────┬────────┘
                                                  │
                             ┌────────────────────┴────────────────┐
                             │                                     │
                             ▼                                     ▼
                   ┌──────────────────┐                ┌──────────────────────┐
                   │ AuthLayer        │                │  ChannelRouter       │
                   │ (API token 校验) │                │  pick channel+acct   │
                   └────────┬─────────┘                │  build ServiceTarget │
                            │                          └───────────┬──────────┘
                            │                                      │
                            ▼                                      ▼
                   ┌──────────────────┐              ┌──────────────────────┐
                   │ BillingLayer     │              │ AdapterDispatcher    │
                   │ (检查 quota)     │              │ build_chat_request() │
                   └────────┬─────────┘              └───────────┬──────────┘
                            │                                    │
                            └──────────┬─────────────────────────┘
                                       │
                                       ▼
                            ┌──────────────────────┐
                            │  reqwest.post(...)   │
                            │  → upstream LLM      │
                            └──────────┬───────────┘
                                       │
                                       ▼
                            ┌──────────────────────┐
                            │ AdapterDispatcher    │
                            │ parse_chat_response()│
                            └──────────┬───────────┘
                                       │
                        ┌──────────────┴────────────────┐
                        │                               │
                        ▼                               ▼
             ┌───────────────────┐          ┌──────────────────────┐
             │ BillingLayer      │          │ TrackingService      │
             │ (扣 token quota)  │          │ 写 request / execution│
             └─────────┬─────────┘          └──────────┬───────────┘
                       │                               │
                       └────────────────┬──────────────┘
                                        ▼
                               ┌────────────────┐
                               │ Response → Cli │
                               └────────────────┘
```

**关键点**：
- 核心转发链路是同步的（阻塞等上游响应）
- Billing 的预扣（reserve）在调上游**之前**做，避免超额
- Tracking 的 log 写入在响应返回**之后**做（异步 fire-and-forget，不阻塞响应）

### 5.2 流式 Chat

```
... (前面同非流式，直到 build_chat_request 那步) ...
                                       │
                                       ▼
                            ┌──────────────────────┐
                            │  reqwest stream      │
                            │  bytes_stream()      │
                            └──────────┬───────────┘
                                       │  Bytes chunks
                                       ▼
                            ┌──────────────────────┐
                            │  StreamDriver        │
                            │  • 按 \n\n 切 event  │
                            │  • Adapter.parse_... │
                            │  • 累积 Usage        │
                            └──────────┬───────────┘
                                       │  ChatStreamEvent
                                       ▼
                            ┌──────────────────────┐
                            │  重新序列化为 OpenAI │
                            │  SSE 格式给客户端    │
                            └──────────┬───────────┘
                                       │
                          (流完结束时)  │
                                       ▼
                            ┌──────────────────────┐
                            │ 同非流式的结算+日志  │
                            └──────────────────────┘
```

**关键点**：
- `Adapter::parse_chat_stream_event` 每次处理**一个 SSE 事件**（已去 `data: ` 前缀）
- StreamDriver 负责累积（token counts 要在流结束时才能 final settle）
- Client 看到的是 OpenAI 格式的 SSE，我们在 StreamDriver 里重新序列化

---

## 6. 目录结构（core）

```
core/src/
├── lib.rs                          # 重新导出
├── error.rs                        # AdapterError / SummerAiError
├── support.rs                      # 内部 serde 工具（from genai）
│
├── types/                          # canonical 类型
│   ├── mod.rs
│   ├── common/                     # 跨协议共享
│   │   ├── mod.rs
│   │   ├── message.rs              # ChatMessage + Role + ContentPart
│   │   ├── tool.rs                 # Tool + ToolCall + ToolChoice
│   │   ├── usage.rs                # Usage + FinishReason
│   │   ├── stream_event.rs         # ChatStreamEvent + StreamEnd
│   │   └── binary.rs               # Binary（图片/音频 content part）
│   └── openai/                     # OpenAI wire 格式（handler 入口）
│       ├── mod.rs
│       ├── chat.rs                 # ChatRequest (扁平 25+ 字段)
│       └── model.rs
│
├── resolver/                       # 运行时上下文
│   ├── mod.rs
│   ├── auth.rs                     # AuthData（None / Single / FromEnv）
│   ├── endpoint.rs                 # Endpoint（URL wrapper）
│   └── target.rs                   # ServiceTarget
│
├── adapter/
│   ├── mod.rs                      # Adapter trait + Capabilities + CostProfile
│   ├── kind.rs                     # AdapterKind enum
│   ├── channel_type_map.rs         # ChannelType(i16) ↔ AdapterKind
│   ├── dispatcher.rs               # 静态 match 分派
│   ├── adapters/                   # 19 实现
│   │   ├── mod.rs
│   │   ├── openai.rs
│   │   ├── openai_resp.rs
│   │   ├── openai_compat.rs
│   │   ├── azure.rs
│   │   ├── anthropic.rs
│   │   ├── gemini.rs
│   │   ├── ...
│   │   └── common/                 # adapter 共享工具
│   │       ├── build_url.rs
│   │       ├── build_headers.rs
│   │       └── parse_json.rs
│   └── stream/                     # SSE 解析工具
│       ├── mod.rs
│       ├── parser.rs               # `data: ...\n\n` 解析
│       └── driver.rs               # StreamDriver（组装流）
│
├── webc/                           # HTTP + 低级 SSE
│   ├── mod.rs
│   ├── client.rs
│   └── sse.rs
│
└── cost/                           # 价格计算
    ├── mod.rs
    └── calculator.rs
```

---

## 7. 从参考项目搬什么

| 源 | 搬法 | 目标 | 签名对齐 |
|---|---|---|---|
| genai `chat/` 全部类型 | 照搬代码，改 import | `core/types/common/` | **保持**（我们 canonical 等同 genai canonical） |
| genai `adapter/adapters/*/adapter_impl.rs` 每个 300-600 行 | 搬转换逻辑，**改 trait 签名** | `core/adapter/adapters/*.rs` | **改**（去 `ChatOptionsSet`、去 `ModelIden`、接入 `ServiceTarget`） |
| genai `adapter/adapters/openai/streamer.rs` SSE 解析 | 拆出来 | `core/adapter/stream/` | **改**（接入我们的 `parse_chat_stream_event`） |
| genai `webc/web_stream.rs` | 照搬 | `core/webc/sse.rs` | 保持 |
| genai `resolver/auth_data.rs` | 照搬 | `core/resolver/auth.rs` | 保持 |
| genai `resolver/endpoint.rs` | 照搬 | `core/resolver/endpoint.rs` | 保持 |
| ironclaw `llm/costs.rs` | 搬 PriceTable + 算法 | `core/cost/calculator.rs` | **改**（价格源从 HashMap 改 DB） |
| ironclaw `llm/circuit_breaker.rs` | 搬（Phase 6） | `relay/service/circuit_breaker/` | **改**（状态按 `channel_account_id` 分组） |
| ironclaw `llm/retry.rs` | 搬（Phase 6） | `relay/service/retry/` | 保持（逻辑不依赖业务） |
| ironclaw `llm/failover.rs` | 搬（Phase 6） | `relay/service/failover/` | **改**（候选来源 DB） |

**不搬**：

| 源 | 理由 |
|---|---|
| genai `client/*` | SDK 入口，relay 不需要 |
| genai `adapter_kind.rs::from_model()` | relay 不按 model 名推 adapter |
| genai `ChatOptionsSet` | 我们 ChatRequest 扁平，直接取字段 |
| genai `Headers`（自研 HashMap） | 我们直接用 reqwest HeaderMap |
| genai `ModelIden` | 用 `ServiceTarget.actual_model: String` 替代 |
| ironclaw `provider.rs`（instance-based Provider trait） | 和我们 ZST 风格冲突 |
| ironclaw `oauth / codex_*` | OAuth 太重，先用 API key |
| ironclaw `reasoning_models.rs / vision_models.rs / image_models.rs` | 硬编码 model 清单，我们 DB 驱动 |
| ironclaw `smart_routing.rs` | 太复杂，Phase 6 再看 |

---

## 8. 扩展点（加新 adapter 三步走）

1. **`core/adapter/kind.rs`** 加一个变体（例如 `Perplexity`）
2. **`core/adapter/channel_type_map.rs`** 的 `TryFrom<i16>` / `From<AdapterKind>` 加分支，约定编码（例如 `60`）
3. **`core/adapter/adapters/perplexity.rs`** 新文件，`pub struct PerplexityAdapter;` + `impl Adapter`
4. **`core/adapter/dispatcher.rs`** 每个 match 加一行

借助 Rust exhaustive match，忘记任一步编译失败——这是我们选 ZST + static dispatch 的主要理由。

---

## 9. 与分支差异记录

**本项目与 feature/extract-summer-ai 分支的差异**：

| 分支做法 | 本项目选择 | 理由 |
|---|---|---|
| `core/provider/` 自研 4 provider（OpenAI/Anthropic/Gemini/Azure） | 用 genai 搬 19 adapter | 19 家覆盖更广，代码现成 |
| `core/convert/{message,content,tool}.rs` 独立转换层 | adapter 内部 inline 转换 | 少一层间接 |
| `core/stream/{sse_parser,event_stream,chunk_aggregator}.rs` | 合并到 `adapter/stream/` | 流和 adapter 强耦合，分开徒增依赖 |
| 有 `hub/` 子 crate（DDD 尝试） | 不要 hub | 分支自己都在清理 |
| `model/entity/` 按功能域分 11 子目录 | 同样按域分 | **照搬** |
| `relay/src/service/` 按 openai endpoint 分 | 同样 | **照搬** |
| `admin/src/router + service` | 同样 | **照搬** |
| `billing/src/service/engine/` 三阶段扣费 | 同样 | **照搬** |

**本项目与 genai 的差异**：

| genai 做法 | 本项目选择 | 理由（**业务驱动**） |
|---|---|---|
| `Adapter` trait 9 方法 | 5 核心 + 3 元信息 + 1 运维（`fetch_model_names`） + 子 trait `EmbedAdapter` | 按业务需要裁剪，不为精简而精简——详见 §3.1.1 |
| `ChatOptionsSet`/`EmbedOptionsSet` 视图 | 直接取 `ChatRequest` 扁平字段 | 入口 `ChatRequest` 已对齐 OpenAI wire 扁平，再套视图多余 |
| `ModelIden { kind, name }` | `ServiceTarget.actual_model: String` | `ServiceTarget` 已携带 kind，包装重复 |
| `Headers`（自研 HashMap） | `reqwest::HeaderMap` 直传 | relay 最后要进 reqwest，自研 Headers 要二次转换 |
| `Client::exec_chat(model, ...)` | Handler 直调 `AdapterDispatcher` + reqwest | relay 不是 SDK，无门面需求 |
| `AdapterKind::from_model("gpt-4o")` 推断 | `ChannelType::TryFrom<i16>` 从 DB 读 | adapter 由 channel 决定，不由 model 名推断 |
| `to_chat_stream` 返 `Stream<Item>` | `parse_chat_stream_event` 单事件 | **stream 中段 billing/log/guardrail 要看每 event**，封装成 Stream 会盖住这层 |
| `default_endpoint` / `default_auth` | **保留**（作 fallback） | 开发测试方便；OpenAI 官方是事实标准，channel 没配 base_url 时用默认也 OK |
| `all_model_names` | **保留** 并改名 `fetch_model_names` | `/v1/models` 动态拉、admin 测试 channel、Ollama 本地发现都要它 |

**本项目与 ironclaw 的差异**：

| ironclaw 做法 | 本项目选择 | 理由 |
|---|---|---|
| `Provider` trait 实例方法（`&self`） | ZST + 静态分派 | 零运行时开销 |
| 内存 HashMap 价格表 | DB `ai.channel_model_price` | 后台可调 |
| 内存 Provider registry | DB `ai.channel` + `ai.vendor` | DB 就是 registry |
| Session state 管理 | 不做 | relay 无会话层 |
| 全栈 OAuth 流程 | 先 API key，OAuth 后期按需 | 简化 |

---

## 10. 暂不确定的点（留给后续讨论）

- **Token 配额消耗的原子性**：NewAPI 用 Redis + Lua，我们 Redis 暂未接入。v1 先用 DB 事务（悲观锁）跑通，Phase 6 替换成 Redis
- **Stream 过程中失败的计费**：部分 token 已出、上游 500 —— 算不算钱？NewAPI 是"按已返回 token 结算"，我们照这个
- **多租户隔离**：v1 不做，全局一个租户。后期看 `ai.channel.tenant_id` 字段引入
- **Embedding / Image / Audio**：Phase 3 只做 chat，其他后期

---

## 11. 变更流程

本文档是 **契约**。任何改动需：

1. 新开 `docs/ADR/<date>-<title>.md` 记录动因、对比、结论
2. 更新本文档对应章节，并在文末"变更日志"追加一行
3. 代码 PR 必须和文档 PR 一起走

**变更日志**：

| 日期 | 修改 | 提案 |
|---|---|---|
| 2026-04-19 | 初版 | N/A |
