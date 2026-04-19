# summer-ai 设计文档

> 模型版本：Claude Opus 4.7 (1M context)
> 修订：2026-04-19

`summer-ai` 是 **LLM 中转站（relay / AI gateway）** ——
它接受前端 OpenAI 兼容请求，按配置路由到**任意动态上游**（官方 OpenAI / Azure /
OpenRouter / 硅基流动 / DeepSeek / 阿里云百炼 / 自建 vllm / ollama / Anthropic /
Gemini / ...），统一做认证 / 计费 / 限流 / 审计 / 观测。

本文档回答一个核心问题：

> **"上游是动态的，如何设计？"**

---

## 0. Relay 与 SDK 的本质区别

| 维度 | SDK（genai / async-openai） | Relay（summer-ai） |
|---|---|---|
| 上游数量 | 1 个（`Client::new()` 定死） | **N 个**（DB 里每个 `ai.channel` 一个） |
| 上游决策时机 | 编译期或应用启动期 | **每次请求**重新决策 |
| 上游配置变更 | 改代码重编译 | **DB UPDATE 热更新** |
| 鉴权 | 环境变量 / 硬编码 | **每个 channel 独立凭证**，JSONB 多格式 |
| 协议分派 | 调用方显式选（`.openai().chat()`） | 由**上游配置**决定 |
| 失败处理 | 错误返回调用方 | **自动 failover** 到其它 channel |

**"上游动态"不是一件事，它是 6 件事**。设计的核心就是把这 6 件事分别扔给
边界清晰的组件，让**协议层**永远只做一件静态的事。

---

## 1. 动态的 6 个维度

| # | 维度 | DB 来源 | 运行时何时才知道 |
|---|---|---|---|
| 1 | **协议家族**（OpenAI / Anthropic / Gemini / Azure / Ollama） | `channel.channel_type` (smallint) | 请求到达 → router.pick |
| 2 | **Endpoint** (`base_url`) | `channel.base_url` | 同上；failover 时换 |
| 3 | **凭证** | `channel_account.credentials` (JSONB) + `credential_type` | 同上；OAuth token 可能过期 |
| 4 | **Model 映射** | `channel.model_mapping` (JSONB) | 同上 |
| 5 | **额外 headers**（OpenRouter `HTTP-Referer`、Anthropic `anthropic-version`） | `channel.config` (JSONB) | 同上 |
| 6 | **路由策略**（哪个 channel / account / 权重 / 健康度） | DB 多表 + 运行时指标 | 每次请求 |

---

## 2. 整体分层

```text
┌─────────────────────────────────────────────────────────────┐
│ (A) Protocol 层 — 编译期静态                                 │
│    pub trait Adapter { ... associated fn only ... }         │
│    ZST：OpenAIAdapter / AnthropicAdapter / GeminiAdapter... │
│    每个 adapter 仅做：ChatRequest ↔ wire format             │
└────────────────────────┬────────────────────────────────────┘
                         ▲ enum 静态分派
┌────────────────────────┴────────────────────────────────────┐
│ (B) AdapterDispatcher — kind → Adapter 的 match              │
│    pub fn chat(kind: AdapterKind, target, req) -> ...       │
│    编译期的"注册表"，加新 provider 改一处                      │
└────────────────────────┬────────────────────────────────────┘
                         ▲ 被 Relay 层调用
┌────────────────────────┴────────────────────────────────────┐
│ (C) ServiceTarget — 每次请求现场构造的 POJO                   │
│    { endpoint, auth, actual_model, extra_headers }          │
│    没有 &self、没有 trait，就是数据                           │
└────────────────────────┬────────────────────────────────────┘
                         ▲ 由下面 3 个解析器填充
   ┌─────────────────────┼──────────────────────────┐
   │                     │                          │
   ▼                     ▼                          ▼
┌──────────────┐   ┌──────────────┐        ┌──────────────┐
│ ChannelRouter │  │ Credential    │        │ ModelResolver │
│ 选 channel +  │  │ Resolver      │        │ logic model → │
│ account       │  │ JSONB → Auth  │        │ actual model  │
│ (权重/优先级/ │  │ Data          │        │               │
│  健康/租户)   │  │ 按 credential_│        │               │
│               │  │ type 分派     │        │               │
└──────┬───────┘   └──────┬───────┘        └──────┬───────┘
       ▲                  ▲                       ▲
       └────────┬─────────┴──────────────┬────────┘
                │                        │
   ┌────────────┴────────────┐ ┌─────────┴──────────────┐
   │ ChannelStore            │ │ DB                     │
   │ ArcSwap<Snapshot>       │ │ ai.channel             │
   │ + 后台 tick 刷新          │ │ ai.channel_account     │
   │ + Pg LISTEN/NOTIFY      │ │ ai.model_config        │
   │   事件驱动热更           │ │ (+ governance / rate)  │
   └─────────────────────────┘ └────────────────────────┘
```

### 切分原则

- **(A)(B) 是编译期** — 加新协议要写代码（但新一家 OpenAI-compat 厂商不需要）
- **(C) 是每次请求** — 数据驱动的纯 POJO
- **下面的解析器 + Store** 是**运行时**层 — 从 DB 读配置 + 维护在线指标

---

## 3. 每层职责

### 3.1 Protocol 层（`core/adapter`）

**关键原则**：
- 所有方法都是 **associated fn**（无 `&self`）
- Adapter 是 **ZST**
- **永远不持有** endpoint / api_key / 任何运行时状态
- 协议默认值（`default_auth()` / `default_endpoint()`）只作 fallback

```rust
pub trait Adapter {
    const KIND: AdapterKind;
    const DEFAULT_API_KEY_ENV_NAME: Option<&'static str>;

    fn default_auth() -> AuthData {
        match Self::DEFAULT_API_KEY_ENV_NAME {
            Some(env) => AuthData::from_env(env),
            None => AuthData::None,
        }
    }
    fn default_endpoint() -> Endpoint;
    fn capabilities() -> Capabilities;

    fn to_web_request_data(
        target: &ServiceTarget,
        service_type: ServiceType,
        request: &ChatRequest,
    ) -> Result<WebRequestData>;

    fn to_chat_response(
        target: &ServiceTarget,
        response: WebResponse,
    ) -> Result<ChatResponse>;

    fn to_chat_stream(
        target: &ServiceTarget,
        response: reqwest::Response,
    ) -> BoxStream<'static, Result<ChatStreamEvent>>;
}
```

ZST 实现示例：

```rust
pub struct OpenAIAdapter;

impl Adapter for OpenAIAdapter {
    const KIND: AdapterKind = AdapterKind::OpenAI;
    const DEFAULT_API_KEY_ENV_NAME: Option<&'static str> = Some("OPENAI_API_KEY");

    fn default_endpoint() -> Endpoint {
        Endpoint::from_static("https://api.openai.com/v1/")
    }
    // ...
}
```

### 3.2 Dispatcher 层（`core/adapter/dispatcher.rs`）

编译期注册表 + 运行时 O(1) 分派：

```rust
pub struct AdapterDispatcher;

impl AdapterDispatcher {
    pub async fn chat(
        kind: AdapterKind,
        target: &ServiceTarget,
        request: &ChatRequest,
    ) -> Result<ChatResponse> {
        match kind {
            AdapterKind::OpenAI       => OpenAIAdapter::to_chat(...),
            AdapterKind::OpenAICompat => OpenAICompatAdapter::to_chat(...),
            AdapterKind::Azure        => AzureAdapter::to_chat(...),
            AdapterKind::Anthropic    => AnthropicAdapter::to_chat(...),
            AdapterKind::Gemini       => GeminiAdapter::to_chat(...),
            AdapterKind::Ollama       => OllamaAdapter::to_chat(...),
        }
    }
}
```

**为什么不用 trait object**：
- 运行时 adapter 数量**有限且已知**（5-10 个协议家族）
- 静态分派零开销，且编译期穷尽检查
- 加新 adapter 的开销就是"改一个 enum + 一个 match 分支"

### 3.3 ServiceTarget（`core/resolver/target.rs`）

一次请求的运行时上下文，**就是一个 POJO**：

```rust
pub struct ServiceTarget {
    pub endpoint: Endpoint,
    pub auth: AuthData,
    pub actual_model: String,
    pub extra_headers: BTreeMap<String, String>,
}
```

Adapter 通过 `&ServiceTarget` 读取运行时信息；不会在构造期持有。

### 3.4 AuthData（`core/resolver/auth.rs`）

```rust
pub enum AuthData {
    None,
    Bearer(String),
    Header { name: String, value: String },
    QueryParam { name: String, value: String },
    AzureApiKey { key: String, api_version: String },
    Oauth { access_token: String, refresh_token: Option<String>, expires_at: Option<DateTime<Utc>> },
    AwsSigV4 { access_key_id: String, secret_access_key: String, region: String },
}

impl AuthData {
    pub fn from_env(name: &str) -> Self { ... }
    pub fn resolve(&self) -> Result<String> { ... }  // 支持 env 懒加载
}
```

### 3.5 ChannelRouter（Relay 层）

```rust
#[async_trait]
pub trait ChannelRouter: Send + Sync {
    async fn pick(
        &self,
        model: &str,
        tenant: Option<&str>,
        hint: Option<&RouteHint>,
    ) -> Result<ChannelPick>;
}

pub struct ChannelPick {
    pub channel: ChannelModel,
    pub account: ChannelAccountModel,
}
```

默认实现：
1. 从 `ChannelStore.snapshot()` 拿快照
2. 过滤出 `channel.models` 包含 `model` 的 enabled channel
3. 按 `tenant_id` 过滤（多租户）
4. 按 `priority` 取最高优先级集合
5. 剔除 `health.status != Healthy` 的
6. 按 `weight` 加权随机选一个 channel
7. 在它的 schedulable accounts 里再按 weight 随机选一个 account

### 3.6 CredentialResolver（Relay 层）

根据 `account.credential_type` 字段分派：

```rust
pub fn resolve_credentials(
    account: &ChannelAccountModel,
    kind: AdapterKind,
) -> Result<AuthData> {
    match account.credential_type.as_str() {
        "api_key" => resolve_api_key(account, kind),
        "oauth"   => resolve_oauth(account),
        "aws_sigv4" => resolve_sigv4(account),
        "cookie"  => resolve_cookie(account),
        _ => Err(CredentialError::Unsupported),
    }
}

fn resolve_api_key(account: &ChannelAccountModel, kind: AdapterKind) -> Result<AuthData> {
    let key = account.credentials["api_key"]
        .as_str()
        .ok_or(CredentialError::MissingField("api_key"))?;
    Ok(match kind {
        AdapterKind::OpenAI | AdapterKind::OpenAICompat => AuthData::Bearer(key.into()),
        AdapterKind::Anthropic => AuthData::Header {
            name: "x-api-key".into(),
            value: key.into(),
        },
        AdapterKind::Gemini => AuthData::QueryParam {
            name: "key".into(),
            value: key.into(),
        },
        AdapterKind::Azure => {
            let ver = account.credentials["api_version"]
                .as_str()
                .unwrap_or("2024-08-01-preview");
            AuthData::AzureApiKey { key: key.into(), api_version: ver.into() }
        }
        _ => AuthData::Bearer(key.into()),
    })
}
```

**这里是解决"凭证动态"的关键**：
- `credentials` 是 JSONB 允许任意 schema
- `credential_type` 字段告诉 resolver 如何解析
- 协议家族（`AdapterKind`）决定这把 key 该塞进哪个 auth 槽

### 3.7 ChannelStore（Relay 层）

```rust
pub struct ChannelStore {
    inner: ArcSwap<Snapshot>,
}

pub struct Snapshot {
    channels: Vec<ChannelModel>,
    accounts_by_channel: BTreeMap<i64, Vec<ChannelAccountModel>>,
    // 预索引：按 model name / tenant id 查询 O(1)
    channels_by_model: HashMap<String, Vec<i64>>,
    channels_by_tenant: HashMap<String, Vec<i64>>,
}

impl ChannelStore {
    /// 读快照无锁（ArcSwap）
    pub fn snapshot(&self) -> Arc<Snapshot> {
        self.inner.load_full()
    }

    /// 后台定时刷新（每 30 秒）
    pub fn spawn_refresh(self: Arc<Self>, db: DatabaseConnection, interval: Duration) {
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;
                if let Ok(snapshot) = Snapshot::load(&db).await {
                    self.inner.store(Arc::new(snapshot));
                }
            }
        });
    }

    /// Pg LISTEN/NOTIFY 事件驱动（立即刷新）
    pub fn spawn_listen(self: Arc<Self>, db: DatabaseConnection) {
        // 订阅 `ai_channel_changes` channel，收到通知就 reload
    }
}
```

---

### 3.8 Adapter 的 `cost_profile()`（协议级计费常量）

**问题场景**：Anthropic 的 prompt caching 对"读缓存 token"只收 **1/10 单价**，
对"写缓存 token"收 **1.25 倍单价**（5 分钟 TTL）或 **2 倍单价**（1 小时 TTL）。

- 这些系数是 **Anthropic 官方协议规定**的，任何走 Anthropic 协议的 channel 都一样。
- OpenAI 的缓存读打 5 折，写不加价。
- Gemini / DeepSeek 目前无 prompt cache。

这些**协议级常量**不该放 DB（运营误操作会算错账），也不该每个 adapter 硬编码
在算账代码里（重复、容易漏）。应作为 `Adapter` trait 的 associated function：

```rust
pub trait Adapter {
    // ... 前面的 default_endpoint / default_auth / capabilities ...

    /// 协议级计费模型。实际单价（$/1K tokens）由 DB 里的 `channel_model_price` 决定；
    /// 这里只描述"同一个 token 在不同计费类型下的系数关系"。
    fn cost_profile() -> CostProfile {
        CostProfile::default()
    }
}

#[derive(Debug, Clone)]
pub struct CostProfile {
    /// cache write 的单价乘数（Anthropic 5m TTL=1.25, 1h TTL=2.0; 其它=1.0）。
    pub cache_write_multiplier: Decimal,
    /// cache read 的**折扣**（Anthropic=0.1 即 1 折；OpenAI=0.5 即 5 折；其它=1.0）。
    pub cache_read_discount: Decimal,
    /// 是否声明支持 prompt caching。用于 router / middleware 做前置判断。
    pub supports_prompt_cache: bool,
}
```

各 adapter 的默认值：

| Adapter | `cache_write_multiplier` | `cache_read_discount` | `supports_prompt_cache` |
|---|---|---|---|
| `OpenAIAdapter` | `1.0` | `0.5` | `true` |
| `AnthropicAdapter`（5m） | `1.25` | `0.1` | `true` |
| `AnthropicAdapter`（1h） | `2.0` | `0.1` | `true` |
| `GeminiAdapter` | `1.0` | `1.0` | `false` |
| `OpenAICompatAdapter` | `1.0` | `1.0` | `false` |

### 与 `channel_model_price` 表的职责边界

```text
计费单价（$/1K tokens） → ai.channel_model_price 表（运营 CRUD）
计费系数（cache 打折）  → Adapter::cost_profile() 常量（代码级）
实际扣款               → middleware 读 usage × 单价 × 系数
```

**结账公式**（伪代码）：

```rust
let unit_price = channel_model_price.input_per_1k;      // 运营配置
let profile = AdapterDispatcher::cost_profile(kind);    // 代码常量

total_cost = usage.prompt_tokens       * unit_price
           + usage.cache_write_tokens  * unit_price * profile.cache_write_multiplier
           + usage.cache_read_tokens   * unit_price * profile.cache_read_discount
           + usage.completion_tokens   * channel_model_price.output_per_1k;
```

### 3.9 Capability Fallback Middleware（能力降级）

**问题场景**：用户用 Claude Code 一类工具，每次请求带 20 个 `tools` 定义。
运营把请求路由到 DeepSeek-v3（便宜）。但 DeepSeek 官方 API 不支持 function
calling，直接透传会上游 `400 Bad Request`。

**不改写请求的后果**：用户被迫放弃便宜模型 → relay 失去商业价值。

**借鉴 Zeroclaw 的能力声明 + 降级策略**：

1. `Adapter::capabilities()` 声明**协议家族**默认能力。
2. `channel.capabilities`（JSONB 字段）可覆盖——DeepSeek 虽走 OpenAI 协议，但
   channel 里声明 `{"tools": false}`。
3. Request 进入流水线时，一个**独立 middleware** 检查请求特征 vs 最终 channel
   能力，按规则改写请求，让上游"看起来支持"。

```rust
// 不进 Adapter trait，是 relay 层的 middleware
pub async fn capability_fallback_middleware(
    req: &mut ChatRequest,
    caps: &EffectiveCapabilities,   // Adapter 默认 ∩ Channel 覆盖
    fallback_cfg: &FallbackConfig,  // 各降级策略开关
) -> Result<()> {
    // tools → system prompt
    if req.options.tools.is_some()
        && !caps.tools
        && fallback_cfg.tools_as_prompt
    {
        inject_tools_as_system_prompt(req);
        req.options.tools = None;
    }

    // vision → 拒绝（或接 OCR 辅助模型，可选）
    if req.has_image_parts() && !caps.vision {
        match fallback_cfg.vision_strategy {
            VisionFallback::Reject       => return Err(UnsupportedFeature::Vision),
            VisionFallback::OcrRewrite   => rewrite_images_via_ocr(req).await?,
            VisionFallback::StripSilently => strip_image_parts(req),
        }
    }

    // response_format=json_schema → 不支持则注入 prompt 约束
    if req.options.response_format.is_some()
        && !caps.response_format_json
        && fallback_cfg.json_schema_as_prompt
    {
        inject_json_schema_as_prompt(req);
        req.options.response_format = None;
    }

    Ok(())
}
```

### 与协议层的职责边界

```text
协议转换（序列化）      → Adapter（纯 ZST，不关心降级）
能力宣告（可能性）      → Adapter::capabilities() / channel.capabilities
降级策略（可选性）      → capability_fallback_middleware（运维可开关）
```

- **Adapter 只负责一件事**：如果协议支持 tools，就把 `ChatRequest.options.tools`
  转成 wire format；否则在没人前置改写的情况下就让它错给上游。
- **Middleware 做"让错不发生"**：检测到上游不支持，主动改写请求，**在 Adapter
  看到它之前**降级好。

这样：
- 协议层保持纯净（只做协议，不懂运维）
- 降级策略可以**按 channel / 按租户独立配置**（有人要原样报错不要降级）
- 不同降级策略（tools-as-prompt / vision-as-ocr）可以**独立开发、独立测试**

---

## 4. 一次请求的完整生命周期

```text
┌──────────────── HTTP 入口 ────────────────────┐
│ POST /v1/chat/completions                     │
│ body: { model: "gpt-4o-mini", messages: ... } │
│ header: Authorization: Bearer sk-mine-xxx     │  ← 我方分发给客户的 key
└──────────┬────────────────────────────────────┘
           │
           ▼
┌───────────────────────────────────────────────┐
│ 1. 入口鉴权                                    │
│    - 我方 API Key → tenant + user              │
│    - 不通过 → 401                             │
└──────────┬────────────────────────────────────┘
           │ { tenant, user }
           ▼
┌───────────────────────────────────────────────┐
│ 2. Router.pick(model, tenant, hint?)           │
│    → ChannelPick { channel, account }         │
└──────────┬────────────────────────────────────┘
           │
           ▼
┌───────────────────────────────────────────────┐
│ 3. 构造 ServiceTarget:                         │
│      kind   = channel.channel_type.into()      │ (动态 #1)
│      endpoint = Endpoint::from(channel.base_url) │ (动态 #2)
│      auth   = resolve_credentials(account, kind) │ (动态 #3)
│      actual_model = resolve_model(channel, req.model) │ (动态 #4)
│      extra_headers = resolve_headers(channel)  │ (动态 #5)
└──────────┬────────────────────────────────────┘
           │
           ▼
┌───────────────────────────────────────────────┐
│ 4. AdapterDispatcher::chat(kind, &target, &req)│  ← 静态分派（穷尽检查）
│    → Adapter::to_web_request_data(...)         │
│    → Client 发 HTTP 请求                       │
│    → Adapter::to_chat_response(body)           │
└──────────┬────────────────────────────────────┘
           │
           ▼
┌───────────────────────────────────────────────┐
│ 5. 后置中间件                                   │
│    - 写 ai.request_execution（审计）            │
│    - 扣 ai.user_quota（计费）                   │
│    - 更新 channel health（成功率/耗时）          │
└──────────┬────────────────────────────────────┘
           │
           ▼ ChatResponse → 前端
```

---

## 5. 对"动态"的回答

| 需求 | 做法 | 改动范围 |
|---|---|---|
| 新增一家 **OpenAI-compat 供应商** | `INSERT INTO ai.channel` + `INSERT INTO ai.channel_account` | **零代码** |
| 新增一个 **新协议家族**（如 Cohere native） | 写 `CohereAdapter` ZST + AdapterKind 加变体 + dispatcher match 一行 | **3 处改动** |
| 改 channel 配置（切 base_url / 换 key） | `UPDATE ai.channel` / `UPDATE ai.channel_account` | **DB UPDATE + 下次 tick 热生效** |
| 换路由策略（按用户特征 / canary） | 换 `ChannelRouter` 实现 | **仅改 Router** |
| 某 channel 挂了自动降级 | Health tracker + Router 过滤 + 请求失败重试 | **仅改中间件** |
| 新增一种凭证（OAuth / 签名 / Cookie） | `AuthData` 加变体 + `CredentialResolver` 加分派 | **2 处** |

---

## 6. 实施优先级

### P0（MVP — 能跑通"openai-compat → 任意上游"）
- [x] `ai.channel / ai.channel_account / ai.model_config` entity（已移植）
- [ ] `Adapter` trait + `AdapterKind` enum + `AdapterDispatcher`
- [ ] `OpenAIAdapter` + `OpenAICompatAdapter`（足以覆盖 90% 国内外上游）
- [ ] `ServiceTarget + AuthData + Endpoint`
- [ ] `CredentialResolver::resolve_api_key`（只做 `"api_key"` 类型）
- [ ] `ChannelStore`（ArcSwap 快照 + 30s tick 刷新）
- [ ] `ChannelRouter`（按 model 过滤 + weight 随机）
- [ ] `Client`（组装 Dispatcher + reqwest）
- [ ] axum handler `POST /v1/chat/completions`
- [ ] `SummerAiPlugin` 组装

### P1（可用化）
- [ ] `AnthropicAdapter` / `GeminiAdapter`（native 协议）
- [ ] `AzureAdapter`（deployment + api-version 特殊 URL）
- [ ] Health tracker + 失败重试 + failover
- [ ] 租户隔离（channel.tenant_id 过滤）
- [ ] 请求/响应审计（`ai.request_execution`）
- [ ] Pg LISTEN/NOTIFY 事件热更新

### P2（生产化）
- [ ] 计费（`ai.user_quota` 扣减）
- [ ] 速率限制（RPM/TPM，`ai.governance_rate_limit`）
- [ ] OAuth 凭证自动刷新
- [ ] Guardrail（`ai.guardrail_*`）
- [ ] OpenTelemetry
- [ ] 管理后台 CRUD API

---

## 7. 参考项目对齐

本设计直接借鉴并收敛了以下项目的核心抽象：

| 项目 | 借鉴点 |
|---|---|
| [genai](https://github.com/jeremychone/rust-genai) | `Adapter` trait（associated fn + const），`AdapterDispatcher` 静态分派，`AuthData::from_env`，`ModelIden` |
| [llm-connector](https://github.com/lipish/llm-connector) | `ServiceTarget` 运行时上下文，`Protocol` 与 `Provider` 分层，`AuthStrategy` / `HeaderPolicy` |
| [one-api](https://github.com/songquanpeng/one-api) | `channel / channel_account` 表结构，`model_mapping` JSONB，管理后台体验 |
| [litellm](https://github.com/BerriAI/litellm) | `Router` 路由器，统一 OpenAI canonical 格式，100+ provider adapter |
| [Portkey gateway](https://github.com/Portkey-AI/gateway) | 配置驱动的 fallback / loadbalance / conditional 策略 |
| [ironclaw](https://github.com/nearai/ironclaw) | **协议级计费系数**（`cache_write_multiplier` / `cache_read_discount`，见 §3.8） |
| [zeroclaw](https://github.com/zeroclaw-labs/zeroclaw) | **capabilities 驱动的能力降级**（tools → prompt 注入，见 §3.9） |

### 7.1 为什么不照搬 Ironclaw / Zeroclaw 的 `Provider` 实例化模式

Ironclaw 和 Zeroclaw 都把 Provider 设计成**带状态的实例**：

```rust
// Ironclaw 风格（不采用）
let provider: Arc<dyn LlmProvider> = Arc::new(OpenAiProvider::new(api_key, model));
provider.set_model("gpt-5");         // 可变状态
let cost = provider.cost_per_token();// 实例持有价格
provider.complete(request).await?;
```

这对它们是合理的，因为它们的**定位是 Agent 框架**：一个 agent 进程长期持有一
个 LLM client，偶尔切切 model，偶尔 fine-tune 价格。"Provider = deployment"
是自然的心智模型。

但 Relay 不一样：

| 差异 | Agent 框架（ironclaw / zeroclaw） | Relay（summer-ai） |
|---|---|---|
| 上游数量 | 1 个（进程级） | **100+ 个**（`ai.channel` 每行一个） |
| 上游变更频率 | 启动时决定 | **秒级**（运营随时改 DB） |
| 并发模型 | 单 agent 串行 | **高并发**（多租户多请求） |
| 计费 | 按进程记账 | **按每次请求**精细记账 |
| 扩展 | 加新 provider 改代码重编译 | **DB 插一行** |

如果硬搬 Provider 实例化模式到 relay：

1. **热更新崩**：channel 配置改了，要销毁旧 `Arc<dyn Provider>` 重建，涉及正在
   进行的请求如何迁移的复杂问题。
2. **并发要锁**：`set_model` 改变状态 → 要么每个请求 clone 整个 provider
   （昂贵），要么加锁（吞吐掉）。
3. **内存线性增长**：100 个 channel × 每个 provider 自带 HTTP client + 缓存 + 指
   标 = 可观的常驻开销；加到 1000 个 channel 就爆。
4. **不好测试**：一个 provider 一个实例，很难注入 mock 上游。

所以 Relay 必须坚持：

- **Adapter = 协议家族 = ZST**（零运行时开销、无并发问题）
- **`ServiceTarget` = 每次请求重新构造的 POJO**（热更新天然无缝）
- **中央 `AdapterDispatcher::match(kind, ...)`**（静态分派，编译期穷尽）

### 7.2 我们从 Ironclaw / Zeroclaw 各偷了什么

- **Ironclaw 的 `cost_per_token` + cache 系数** → 抽象成 `Adapter::cost_profile()`
  （§3.8）。**保留了计费"协议级常量"的洞察，抛弃了"绑在实例上"的实现方式**。

- **Zeroclaw 的 `supports_native_tools` + prompt-guided fallback** → 抽象成
  独立 middleware（§3.9）。**保留了能力降级的用户价值，把它从 trait 挪到 middleware
  让协议层保持纯净**。

### 7.3 与 SDK 的关键差异

- **genai**：`default_endpoint()` 写死 `api.openai.com` 是对的——它是 SDK
- **summer-ai**：`default_endpoint()` 只是 fallback；`ServiceTarget.endpoint` 永远优先——因为它是 relay

---

## 附录 A：`AdapterKind` 到 `channel.channel_type` 的映射

| `channel.channel_type` | 常量 | `AdapterKind` | 备注 |
|---|---|---|---|
| 1 | `OpenAI` | `OpenAI` | 官方 api.openai.com |
| 3 | `Anthropic` | `Anthropic` | 原生 /v1/messages |
| 14 | `Azure` | `Azure` | deployment + api-version |
| 15 | `Baidu` | `OpenAICompat` | 百度千帆（OpenAI-compat 兼容层） |
| 17 | `Ali` | `OpenAICompat` | 阿里云百炼 |
| 24 | `Gemini` | `Gemini` | 原生 generateContent |
| 28 | `Ollama` | `Ollama` | localhost:11434 |
| 其它 | `OpenAICompat` | `OpenAICompat` | 兜底（DeepSeek / 智谱 / 月之暗面 / 硅基流动 / OpenRouter / vllm ...） |

## 附录 B：`channel_account.credentials` JSONB 结构示例

```jsonc
// credential_type = "api_key"（最常见）
{
    "api_key": "sk-..."
}

// credential_type = "api_key"（Azure，带 api_version）
{
    "api_key": "xxx",
    "api_version": "2024-08-01-preview"
}

// credential_type = "oauth"
{
    "access_token": "...",
    "refresh_token": "...",
    "expires_at": "2026-05-01T00:00:00Z"
}

// credential_type = "aws_sigv4"（Bedrock）
{
    "access_key_id": "AKIA...",
    "secret_access_key": "...",
    "region": "us-west-2"
}

// credential_type = "cookie"（Claude Code 多账号代理场景）
{
    "cookie": "sessionKey=sk-ant-...",
    "csrf_token": "..."
}
```

`credential_type` 字段是**凭证解析层的 tagged union 判别位**，让 JSONB 这种弱类型字段
变得 type-safe。

## 附录 C：`channel.config` JSONB 结构示例

```jsonc
{
    "extra_headers": {
        "HTTP-Referer": "https://my.app",
        "X-Title": "MyApp",
        "anthropic-version": "2023-06-01"
    },
    "request_timeout_ms": 60000,
    "max_retries": 3,
    "circuit_breaker": {
        "threshold": 5,
        "window_secs": 60,
        "cool_down_secs": 300
    }
}
```

---

## 结语

**设计的单一主线**：
把"上游动态"这个复合问题切成 6 个独立维度，
每个维度落在一个**边界清晰的组件**上，
让**协议层保持静态 + 数据层完全动态**。

这样：
- 调试友好（每层可单独单测）
- 扩展友好（加新协议 / 新凭证 / 新路由策略不交叉改动）
- 运维友好（channel 配置热更，不重启）
- 编译期友好（静态分派穷尽检查，adapter 不可能遗漏）
