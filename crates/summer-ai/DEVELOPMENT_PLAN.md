# summer-ai 完整开发蓝图

更新日期：2026-03-31

---

## 一、项目概述与现状

### 1.1 项目定位

`summer-ai` 是一个 Rust 实现的 AI 模型中继网关，对标 one-api (Go) / litellm (Python) / portkey-gateway (TypeScript)。目标是提供统一的 OpenAI 兼容接口，将下游应用的请求智能路由到多个上游 AI 供应商。

### 1.2 当前架构

```
summer-ai (总入口, re-export)
├── summer-ai-core     # Provider 适配层
│   ├── provider/      # OpenAI, Anthropic, Gemini, Azure 适配器
│   └── types/         # 全局类型定义 (Chat, Embedding, Error, Model 等)
├── summer-ai-hub      # 业务逻辑层
│   ├── auth/          # API Key 鉴权中间件 + Token 提取器
│   ├── relay/         # 核心中继：计费、限流、路由、流式处理
│   ├── service/       # 渠道/令牌/日志/模型的 CRUD 及缓存
│   ├── router/        # Axum 路由定义 (OpenAI API + 透传)
│   └── job/           # 定时任务 (渠道健康恢复)
└── summer-ai-model    # 数据模型层
    ├── entity/        # SeaORM 实体 (ability, channel, token, log 等)
    ├── dto/           # 请求 DTO
    └── vo/            # 响应 VO
```

### 1.3 已实现功能清单

| 模块 | 功能 | 完成度 |
|---|---|---|
| **Provider 适配** | OpenAI (Chat/Completions/Embeddings/Responses) | ✅ 高 |
| | Anthropic Claude (消息/工具调用/Thinking 模式) | ✅ 高 |
| | Google Gemini (多模态/工具调用/Embeddings) | 🟡 中 |
| | Azure OpenAI (Deployment 路由/V1 API) | ✅ 高 |
| **计费** | 预扣费/结算/退款 (BillingEngine) | ✅ 有缺陷 |
| **限流** | RPM/TPM/并发限流 (RateLimitEngine) | ✅ 有缺陷 |
| **路由** | 优先级+权重+故障剔除 (ChannelRouter) | ✅ 基础 |
| **鉴权** | Bearer Token 哈希校验 (AiAuthLayer) | ✅ 基础 |
| **流式** | SSE 流转发与解析 | ✅ 有缺陷 |
| **健康检查** | 渠道成功率追踪 + 定时恢复 | ✅ 基础 |
| **资源亲和** | File ID 与渠道绑定 (ResourceAffinityService) | ✅ |

### 1.4 已知问题总览 (来自 ISSUES.md)

| 严重程度 | 数量 | 代表性问题 |
|---|---|---|
| 🔴 Critical | 6 | SSE UTF-8 截断损坏、计费 DB/Redis 不原子（我们要的是最终一致性就行！）、限流 Key 永不过期 |
| 🟠 High | 10 | 路由 N+1 查询、缓存击穿、Token 缓存窗口过长 |
| 🟡 Medium | 12 | Token 估算不精确、Core 层耦合 axum、Entity 缺 Relation |
| 🔵 Low | 8 | 软删除不一致、JSON Schema 校验缺失 |
| **总计** | **36** | |

---

## 二、参考项目研究总结

我们研究了 `docs/relay/` 下 **30 个参考项目**（Rust 13 / Go 8 / Python 5 / TypeScript 3 / Java 1），以下是对 summer-ai 最有价值的发现：

### 2.1 Rust 参考项目（13 个）

| 项目 | 定位 | 对 summer-ai 的核心启示 |
|---|---|---|
| **hadrian** | 企业级全功能网关 | Feature Flags 精细控制功能集；CEL 表达式做 RBAC；语义缓存 (pgvector)；WASM 编译支持 |
| **ai-gateway** (Noveum) | 极速代理 | AWS Bedrock 原生签名 (aws-sigv4)；Elasticsearch 遥测；极致编译优化 (lto=fat) |
| **hub** (Traceloop) | 双模式网关 | YAML 静态配置 vs DB 动态配置双模式；Pipeline 插件化架构；SecretObject 凭据管理 |
| **lunaroute** | 多 crate 高性能代理 | **16 crate 的 Workspace 范例**；双协议透传 (OpenAI+Anthropic 原生)；PII 脱敏 (Aho-Corasick)；JSONL+SQLite 双持久化 |
| **unigateway** | CLI 集成网关 | MCP 协议原生支持；交互式配置向导；`route explain` 诊断 |
| **llm-providers** | 编译时元数据库 | **PHF 编译时哈希表** 存储 100+ 模型元数据（价格/能力/上下文长度），纳秒级查询，零运行时 I/O |
| **llm-connector** | 协议抽象库 | 适配器模式标准实现；Builder 模式灵活配置超时和 BaseUrl；原生支持推理模型的 `thinking_budget` |
| **claude-code-mux** | 轻量中继 | 正则模型名重写；OAuth 2.0 个人账号中转；tiktoken-rs Token 计算；仅 6MB RAM |
| **crabllm** | Rust 版 LiteLLM | 多 crate：core/provider/proxy；SQLite + Redis 双存储 |
| **ultrafast-ai-gateway** | 极速网关 | 极简设计追求最低延迟 |
| **model-gateway-rs** | 模型网关 | 标准化的请求/响应转换管道 |
| **llmg** | 轻量网关 | 配置驱动的简洁路由 |
| **anthropic-proxy-rs** | 单 Provider 代理 | Anthropic 专用优化 |

### 2.2 Go 参考项目（8 个）

| 项目 | 核心启示 |
|---|---|
| **one-api** | 最成熟的"渠道-令牌"管理模式；Adaptor 适配器架构标准；**模型倍率 × 渠道倍率**计费体系；Ability 表驱动动态路由 |
| **new-api / one-hub** | one-api 的二开生态；one-hub 增加了 Telegram Bot、Prometheus 监控、按次收费 |
| **bifrost** | **µs 级延迟** (5000 RPS 仅增 11µs)；sync.Pool 对象池；语义缓存 (Qdrant)；HashiCorp Vault 密钥管理；MCP 协议支持 |
| **APIPark** | Prompt-to-API 封装（将 Prompt 模板化为标准 REST API）；企业级 RBAC 审批流；OpenAPI 3.0 自动文档 |
| **axonhub** | Pipeline 中间件链；Transformer 协议互转层；SDK 透明中转；<100ms 故障转移 |
| **CLIProxyAPI** | OAuth 个人订阅账号 API 化；翻译器注册表模式；自动模型降级 (opus→sonnet) |
| **proxify** | **流平滑控制器** (打字机效果)；**SSE 心跳保活** (防 ELB 超时断开)；热加载 routes.json |

### 2.3 Python + TypeScript + Java（9 个）

| 项目 | 核心启示 |
|---|---|
| **litellm** (Python) | **100+ Provider 适配参考**；路由策略 (P95 延迟/最低成本/轮询)；统一异常映射；MCP 工具调用集成 |
| **portkey-gateway** (TS) | **122KB 体积** 的边缘网关 (Hono)；Config-driven Routing (Header 中携带 JSON 配置)；40+ Guardrails |
| **crewAI** (Python) | 多智能体编排 (Crews + Flows)；中介者模式 |
| **llm-router-api** (Python) | 双层权限控制；优化的 UTF-8 流处理 StreamProcessor |
| **llamaxing** (Python) | JWT 鉴权 (Azure Entra ID)；Langfuse 可观测性集成；Sidecar 部署模式 |
| **solon-ai** (Java) | 方言 (Dialect) 模式适配厂商差异（类似 JDBC）；原生 MCP 集成；ReAct 智能体支持 |

### 2.4 关键设计模式总结

| 模式 | 出现频率 | 说明 |
|---|---|---|
| **Adapter / Adaptor** | 几乎所有项目 | 将异构厂商 API 转为统一内部格式 |
| **Middleware / Pipeline** | hadrian, axonhub, portkey, bifrost | 鉴权/限流/计费/审计按需插入 |
| **Registry** | CLIProxyAPI, litellm | 动态注册协议转换器和模型映射 |
| **Strategy** | litellm, bifrost | 路由算法封装为可替换策略 |
| **PHF / Compile-time Data** | llm-providers | 静态模型元数据编译时固化 |
| **Feature Flags** | hadrian | 按需裁剪功能集 (tiny → full) |

---

## 三、开发路线图

### 阶段 0：项目脚手架与规范 (Week 0)

在写任何功能代码前，先建立工程规范。

#### 0.1 错误处理统一规范

当前错误处理存在三种混乱模式：`.ok()` 静默丢失、`.expect()` 生产 panic、`let _ =` 忽略关键结果。

**执行步骤：**

1. 在 `core/src/types/error.rs` 中定义统一的 `AiError` 枚举：
   ```rust
   #[derive(Debug, thiserror::Error)]
   pub enum AiError {
       #[error("provider error: {provider} - {message}")]
       Provider { provider: String, status: u16, message: String },
       #[error("stream parse error: {0}")]
       StreamParse(String),
       #[error("billing error: {0}")]
       Billing(String),
       #[error("rate limited: {0}")]
       RateLimited(String),
       #[error("authentication failed: {0}")]
       Auth(String),
       // ...
   }
   ```

2. 制定层级规范：
   - `core` 层：始终返回 `Result<T, AiError>`，禁止 `panic!` / `expect()` / `unwrap()`
   - `hub` 层：`tracing::error!` + 降级，关键路径 `Result` 必须处理
   - 所有 `let _ =` 用于关键路径时，必须伴随 `tracing::error!`

3. 全局搜索并替换：
   - `rg "\.expect\(" core/ hub/` → 替换为 `map_err()`
   - `rg "let _ =" hub/src/relay/` → 添加错误日志
   - `rg "\.ok\(\)" core/src/provider/` → 改为 `?` 或显式处理

#### 0.2 Clippy + CI 强制规则

在 workspace 根目录的 `Cargo.toml` 或 `.cargo/config.toml` 中添加：
```toml
[workspace.lints.clippy]
unwrap_used = "deny"
expect_used = "deny"
panic = "deny"
```

#### 0.3 测试框架搭建

```
crates/summer-ai/
├── core/tests/
│   ├── provider_openai.rs      # OpenAI 适配器单元测试
│   ├── provider_anthropic.rs   # Anthropic 适配器单元测试
│   └── sse_parser.rs           # SSE 流解析测试
├── hub/tests/
│   ├── billing_test.rs         # 计费引擎测试
│   ├── rate_limit_test.rs      # 限流引擎测试
│   ├── channel_router_test.rs  # 路由选择测试
│   └── integration/
│       └── relay_e2e.rs        # 完整中继 E2E 测试
└── model/tests/
    └── migration_test.rs       # Schema 迁移测试
```

---

### 阶段 1：Critical 漏洞修复 (Week 1-2)

这是最高优先级。不修复这些问题，系统不可上生产。

#### 1.1 C-01: SSE 流 UTF-8 截断损坏修复

**影响文件：** `core/src/provider/openai.rs:53`, `anthropic.rs:274`, `gemini.rs:298`

**当前问题：** 三个 provider 的 `parse_stream` 使用 `String::from_utf8_lossy` 转换网络字节块。多字节 UTF-8 字符 (中文/emoji) 被拆分到两个 TCP 包时，`lossy` 用 `U+FFFD` 替换，造成不可逆乱码。

**执行步骤：**

1. 创建公共 SSE 解析器 `core/src/types/sse_parser.rs`：
   ```rust
   pub struct SseParser {
       byte_buffer: Vec<u8>,
   }

   impl SseParser {
       pub fn new() -> Self { Self { byte_buffer: Vec::with_capacity(4096) } }

       /// 追加字节块，返回解析出的完整事件列表
       pub fn feed(&mut self, chunk: &[u8]) -> Result<Vec<SseEvent>, AiError> {
           self.byte_buffer.extend_from_slice(chunk);
           let mut events = Vec::new();
           // 在 byte_buffer 中搜索 b"\n\n" 分隔符
           while let Some(pos) = find_double_newline(&self.byte_buffer) {
               let event_bytes: Vec<u8> = self.byte_buffer.drain(..pos + 2).collect();
               let text = String::from_utf8(event_bytes)
                   .map_err(|e| AiError::StreamParse(format!("invalid UTF-8: {e}")))?;
               if let Some(event) = Self::parse_event(&text)? {
                   events.push(event);
               }
           }
           Ok(events)
       }
   }
   ```

2. 支持多种 SSE 分隔符 (`\n\n`, `\r\n\r\n`, `\r\r`)：
   ```rust
   fn find_double_newline(buf: &[u8]) -> Option<usize> {
       buf.windows(2).position(|w| w == b"\n\n")
           .or_else(|| buf.windows(4).position(|w| w == b"\r\n\r\n"))
           .or_else(|| buf.windows(2).position(|w| w == b"\r\r"))
   }
   ```

3. 在三个 provider 的 `parse_stream` 中替换为 `SseParser`

4. **测试用例：**
   - 正常 ASCII 流
   - 中文字符被拆分到两个 chunk
   - emoji (4 字节 UTF-8) 被拆分到三个 chunk
   - 混合 `\r\n\r\n` 分隔符

#### 1.2 C-03: 计费 DB/Redis 原子性修复

**影响文件：** `hub/src/relay/billing.rs:96-141`

**当前问题：** `pre_consume` 先 DB `UPDATE` 扣款，再写 Redis 记录。两步间崩溃 → 用户额度永久丢失。

**执行步骤：**

1. 引入 Pending 状态机制：
   ```rust
   pub async fn pre_consume(&self, ...) -> ApiResult<i64> {
       // 步骤 1: 先在 Redis 写入 Pending 记录（标记"扣款意图"）
       let pending_key = format!("billing:pending:{request_id}");
       self.cache.set_json(&pending_key, &PendingRecord { token_id, quota, .. }, TTL_5MIN).await?;

       // 步骤 2: DB 扣款
       let result = token::Entity::update_many()
           .col_expr(token::Column::RemainQuota, Expr::col(...).sub(quota))
           .exec(&self.db).await?;

       // 步骤 3: 将 Pending 升级为正式记录
       self.cache.set_json(&record_key, &record, TTL).await?;
       self.cache.del(&pending_key).await?;

       Ok(quota)
   }
   ```

2. 添加启动恢复扫描：
   ```rust
   /// 进程启动时扫描所有 pending 记录，执行补偿
   pub async fn recover_pending_records(&self) -> ApiResult<()> {
       let pending_keys = self.cache.scan("billing:pending:*").await?;
       for key in pending_keys {
           let record: PendingRecord = self.cache.get_json(&key).await?;
           // 检查 DB 是否已扣款 → 如已扣，补写 Redis 记录
           // 如未扣款 → 清理 Pending key
       }
   }
   ```

#### 1.3 C-06: 限流 Key 永不过期修复

**影响文件：** `hub/src/relay/rate_limit.rs`

**当前问题：** `INCR` 与 `EXPIRE` 分开调用。`EXPIRE` 失败 → Key 永不过期 → 配额永久锁定。

**执行步骤：**

用 Redis Lua 脚本合并为原子操作：
```rust
const RATE_LIMIT_LUA: &str = r#"
    local current = redis.call('INCR', KEYS[1])
    if current == 1 then
        redis.call('EXPIRE', KEYS[1], ARGV[1])
    end
    return current
"#;

pub async fn increment_rate_limit(&self, key: &str, ttl_seconds: u64) -> ApiResult<i64> {
    let result: i64 = redis::Script::new(RATE_LIMIT_LUA)
        .key(key)
        .arg(ttl_seconds)
        .invoke_async(&mut self.redis)
        .await?;
    Ok(result)
}
```

#### 1.4 C-02: RouteHealth 非原子 Read-Modify-Write

**影响文件：** `hub/src/service/route_health.rs:106-131`

**执行步骤：**

将三个健康计数器改为 Redis Hash + `HINCRBY`：
```rust
/// 原子递增指定字段
pub async fn increment_penalty(&self, channel_id: i64, field: &str) -> ApiResult<i64> {
    let key = format!("route_health:{channel_id}");
    let lua = r#"
        local val = redis.call('HINCRBY', KEYS[1], ARGV[1], 1)
        redis.call('EXPIRE', KEYS[1], ARGV[2])
        return val
    "#;
    redis::Script::new(lua)
        .key(&key)
        .arg(field)  // "penalty" | "rate_limit" | "overload"
        .arg(self.ttl_seconds)
        .invoke_async(&mut self.redis).await
        .map_err(|e| ApiError::internal(e.to_string()))
}
```

#### 1.5 C-04: 流式结算 fire-and-forget 修复

**影响文件：** `hub/src/relay/stream.rs`

**当前问题：** 使用 `tokio::spawn` 异步结算。进程崩溃/重启 → 大量请求免费。

**执行步骤：**

1. 引入 `JoinSet` 追踪所有计费任务：
   ```rust
   pub struct BillingTaskTracker {
       tasks: tokio::task::JoinSet<()>,
   }

   impl BillingTaskTracker {
       pub fn spawn(&mut self, future: impl Future<Output = ()> + Send + 'static) {
           self.tasks.spawn(future);
       }

       /// 优雅停机时调用，等待所有计费任务完成
       pub async fn shutdown(&mut self, timeout: Duration) {
           tokio::time::timeout(timeout, async {
               while self.tasks.join_next().await.is_some() {}
           }).await.ok();
       }
   }
   ```

2. 在 `SummerAiHubPlugin` 的 `build()` 中注册 shutdown hook：
   ```rust
   // 监听 SIGTERM/SIGINT
   tokio::spawn(async move {
       signal::ctrl_c().await.unwrap();
       billing_tracker.shutdown(Duration::from_secs(30)).await;
       std::process::exit(0);
   });
   ```

#### 1.6 C-05: Multipart 内存溢出防护

**影响文件：** `hub/src/router/openai/image_multipart.rs`

```rust
// 在路由定义处添加
use axum::extract::DefaultBodyLimit;

Router::new()
    .route("/v1/images/edits", post(image_edit))
    .layer(DefaultBodyLimit::max(20 * 1024 * 1024))  // 20MB 限制
```

#### 1.7 H-03: 全局 expect/panic 清理

**执行步骤：**

1. `rg "\.expect\(" crates/summer-ai/` → 列出所有位置
2. 逐个替换为 `map_err()` + `?` 或 `.unwrap_or_default()`
3. 特别关注 `hub/src/auth/middleware.rs` 中的 `extract_api_key`

**本阶段测试：**
- 单元测试覆盖 SSE 解析器的所有边界情况
- 集成测试验证计费的预扣-结算-退款链路原子性
- 压测验证限流 Key 在高并发下正确过期

---

### 阶段 2：性能优化与架构解耦 (Week 3-5)

#### 2.1 缓存击穿防御 (SingleFlight)

**影响文件：** `hub/src/service/runtime_cache.rs`

**执行步骤：**

引入 `moka` 缓存库替代手动 Redis 缓存方案：
```toml
# hub/Cargo.toml
moka = { version = "0.12", features = ["future"] }
```

```rust
use moka::future::Cache;

pub struct RuntimeCache {
    /// 本地 L1 缓存（内置 SingleFlight）
    local: Cache<String, Arc<CachedConfig>>,
    /// Redis L2 缓存
    redis: RedisPool,
    /// DB L3
    db: DatabaseConnection,
}

impl RuntimeCache {
    pub async fn get_channel_config(&self, channel_id: i64) -> ApiResult<Arc<CachedConfig>> {
        let key = format!("channel:{channel_id}");
        // moka 的 get_with 自带 SingleFlight 语义
        self.local.get_with(key, async {
            // 先查 Redis
            if let Some(cached) = self.redis.get_json(&key).await.ok().flatten() {
                return Arc::new(cached);
            }
            // Redis miss → 查 DB
            let config = self.load_from_db(channel_id).await.unwrap();
            self.redis.set_json(&key, &config, Duration::from_secs(300)).await.ok();
            Arc::new(config)
        }).await
    }
}
```

#### 2.2 路由层 N+1 查询优化

**影响文件：** `hub/src/relay/channel_router.rs`

**当前问题：** 循环内逐个 Redis 查询渠道健康状态，2N 次网络往返。

**执行步骤：**

```rust
pub async fn build_channel_plan(&self, group: &str, model: &str) -> ApiResult<RouteSelectionPlan> {
    // 步骤 1: 批量获取候选渠道 (单次 DB 查询)
    let candidates = self.get_candidate_channels(group, model).await?;
    let channel_ids: Vec<i64> = candidates.iter().map(|c| c.id).collect();

    // 步骤 2: 批量获取健康快照 (单次 Redis MGET)
    let health_keys: Vec<String> = channel_ids.iter()
        .map(|id| format!("route_health:{id}"))
        .collect();
    let health_snapshots: Vec<Option<HealthSnapshot>> =
        self.cache.mget_json(&health_keys).await?;

    // 步骤 3: 内存中过滤 + 排序
    let available: Vec<_> = candidates.into_iter()
        .zip(health_snapshots)
        .filter(|(_, health)| {
            health.as_ref().map_or(true, |h| !h.is_disabled())
        })
        .collect();

    // 步骤 4: 按优先级 + 权重选择
    self.select_by_priority_and_weight(available)
}
```

#### 2.3 LastUsedIp 批量写入

**影响文件：** `hub/src/service/token.rs`

**执行步骤：**

实现攒批处理器：
```rust
pub struct BatchWriter {
    buffer: tokio::sync::mpsc::Sender<WriteRequest>,
}

impl BatchWriter {
    pub fn new(db: DatabaseConnection, batch_size: usize, flush_interval: Duration) -> Self {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<WriteRequest>(1024);
        tokio::spawn(async move {
            let mut batch = Vec::with_capacity(batch_size);
            let mut interval = tokio::time::interval(flush_interval);
            loop {
                tokio::select! {
                    Some(req) = rx.recv() => {
                        batch.push(req);
                        if batch.len() >= batch_size {
                            Self::flush(&db, &mut batch).await;
                        }
                    }
                    _ = interval.tick() => {
                        if !batch.is_empty() {
                            Self::flush(&db, &mut batch).await;
                        }
                    }
                }
            }
        });
        Self { buffer: tx }
    }

    pub async fn write_ip(&self, token_id: i64, ip: String) {
        self.buffer.send(WriteRequest { token_id, ip }).await.ok();
    }
}
```

#### 2.4 Core 层解耦 axum

**影响文件：** `core/src/types/error.rs`

**执行步骤：**

1. 将 `IntoResponse` 的实现从 `core` 移到 `hub`
2. `core` 的 `Cargo.toml` 移除 `axum` 依赖
3. `core` 仅定义错误枚举和 `From` 转换，`hub` 负责：
   ```rust
   // hub/src/error.rs
   impl IntoResponse for AiError {
       fn into_response(self) -> Response { ... }
   }
   ```

#### 2.5 Entity 层完善 Relation 定义

**影响文件：** `model/src/entity/`

```rust
// model/src/entity/channel.rs
impl RelationTrait for Relation {
    fn def(&self) -> RelationDef {
        match self {
            Self::ChannelAccount => Entity::has_many(channel_account::Entity).into(),
            Self::Ability => Entity::has_many(ability::Entity).into(),
        }
    }
}
```

消除 `ModelService::list_available` 中的 4 次独立查询 → 1 次 JOIN 查询。

#### 2.6 精确 Token 计算

**执行步骤：**

```toml
# core/Cargo.toml
tiktoken-rs = "0.5"
```

```rust
pub fn count_tokens(model: &str, text: &str) -> Result<usize, AiError> {
    let bpe = tiktoken_rs::get_bpe_from_model(model)
        .unwrap_or_else(|_| tiktoken_rs::cl100k_base().unwrap());
    Ok(bpe.encode_with_special_tokens(text).len())
}
```

#### 2.7 BigDecimal 精度修复

```rust
// 禁止：通过 String 降级为 f64
// let f = big_decimal.to_string().parse::<f64>().unwrap();

// 正确：使用 BigDecimal 原生运算
let result = &quota * &multiplier;  // BigDecimal * BigDecimal
```

---

### 阶段 3：Provider 生态扩张 (Week 6-10)

#### 3.1 Provider Adapter Trait 增强

当前 `ProviderAdapter` trait：
```rust
pub trait ProviderAdapter: Send + Sync {
    fn build_request(&self, client, base_url, api_key, req, actual_model) -> Result<RequestBuilder>;
    fn parse_response(&self, body, model) -> Result<ChatCompletionResponse>;
    fn parse_stream(&self, response, model) -> Result<BoxStream<'static, Result<ChatCompletionChunk>>>;
}
```

增强为：
```rust
pub trait ProviderAdapter: Send + Sync {
    /// 供应商名称
    fn name(&self) -> &'static str;

    /// 支持的能力列表
    fn capabilities(&self) -> &[Capability];

    /// 构建 Chat Completion 请求
    fn build_chat_request(...) -> Result<RequestBuilder>;

    /// 构建 Embedding 请求
    fn build_embedding_request(...) -> Result<RequestBuilder> {
        Err(AiError::Unsupported("embeddings"))  // 默认不支持
    }

    /// 构建 Image 生成请求
    fn build_image_request(...) -> Result<RequestBuilder> {
        Err(AiError::Unsupported("image_generation"))
    }

    /// 构建 Audio 请求 (TTS/STT)
    fn build_audio_request(...) -> Result<RequestBuilder> {
        Err(AiError::Unsupported("audio"))
    }

    /// 解析非流式响应
    fn parse_response(&self, body, model) -> Result<ChatCompletionResponse>;

    /// 解析流式响应（使用公共 SseParser）
    fn parse_stream(&self, response, model) -> Result<BoxStream<'static, Result<ChatCompletionChunk>>>;

    /// 自定义鉴权 (如 AWS Bedrock 需要 SigV4 签名)
    fn authenticate(&self, req: RequestBuilder, credentials: &Credentials) -> Result<RequestBuilder> {
        // 默认: Bearer Token
        Ok(req.bearer_auth(&credentials.api_key))
    }

    /// 自定义错误映射
    fn map_error(&self, status: u16, body: &str) -> AiError {
        AiError::Provider { provider: self.name().into(), status, message: body.into() }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Capability {
    ChatCompletion,
    Streaming,
    ToolCalling,
    Vision,
    Embedding,
    ImageGeneration,
    AudioTts,
    AudioStt,
    Thinking,
    ResponsesApi,
}
```

#### 3.2 新 Provider 接入计划

按优先级排序：

| 优先级 | Provider | 关键难点 | 参考项目 |
|---|---|---|---|
| P0 | **DeepSeek** | 兼容 OpenAI 格式，仅需 base_url 配置 | llm-connector |
| P0 | **Groq** | 兼容 OpenAI 格式，极快推理 | ai-gateway |
| P1 | **AWS Bedrock** | SigV4 签名 + 独特的请求/响应格式 | ai-gateway (aws-sigv4) |
| P1 | **Vertex AI** | OAuth2 服务账号鉴权 + Safety Settings | litellm |
| P1 | **Mistral** | 兼容 OpenAI 格式 | one-api |
| P2 | **Ollama** | 本地部署，REST API | crabllm |
| P2 | **vLLM** | OpenAI 兼容服务器 | - |
| P2 | **SiliconFlow** | 国内加速，OpenAI 兼容 | one-api |
| P2 | **通义千问** | 阿里云私有协议 | one-api, new-api |
| P2 | **文心一言** | 百度私有协议 + 独特的 token 机制 | one-api |
| P3 | **Fireworks** | OpenAI 兼容 | litellm |
| P3 | **Together** | OpenAI 兼容 | litellm |
| P3 | **Cohere** | 独特的 Chat 协议 | litellm |

**每个 Provider 接入标准流程：**

1. 在 `core/src/provider/` 下创建 `{provider}.rs`
2. 实现 `ProviderAdapter` trait
3. 在 `core/src/provider/mod.rs` 中注册
4. 编写单元测试（至少覆盖：request 构建、response 解析、stream 解析、error 映射）
5. 编写集成测试（使用 Mock Server）
6. 在 `model/src/entity/channel.rs` 的 `ChannelType` 枚举中添加新变体

**AWS Bedrock 接入示例 (最复杂的 Provider)：**

```rust
// core/src/provider/bedrock.rs
pub struct BedrockAdapter;

impl ProviderAdapter for BedrockAdapter {
    fn name(&self) -> &'static str { "bedrock" }

    fn authenticate(&self, req: RequestBuilder, credentials: &Credentials) -> Result<RequestBuilder> {
        // 使用 aws-sigv4 签名
        let signing_settings = SigningSettings::default();
        let signing_params = v4::SigningParams::builder()
            .access_key(&credentials.access_key_id)
            .secret_key(&credentials.secret_access_key)
            .region(&credentials.region)
            .service_name("bedrock-runtime")
            .time(SystemTime::now())
            .settings(signing_settings)
            .build()?;
        // ... 签名逻辑
    }

    fn build_chat_request(&self, ...) -> Result<RequestBuilder> {
        // Bedrock 使用 /model/{modelId}/invoke 或 /model/{modelId}/invoke-with-response-stream
        let url = format!("{}/model/{}/invoke", base_url, model_id);
        // 转换请求体为 Bedrock 格式
    }
}
```

#### 3.3 协议扩展

除了 Chat Completion，扩展对以下 OpenAI 端点的支持：

```
POST /v1/embeddings              → EmbeddingAdapter
POST /v1/images/generations      → ImageAdapter
POST /v1/audio/transcriptions    → AudioAdapter (STT)
POST /v1/audio/speech            → AudioAdapter (TTS)
GET  /v1/models                  → 统一模型列表
POST /v1/chat/completions        → 已有
POST /v1/responses               → 已有 (ResponsesRuntimeMode)
```

每个端点在 `hub/src/router/openai.rs` 中注册路由，走相同的 鉴权→限流→计费→转发 链路。

#### 3.4 编译时模型元数据库

参考 `llm-providers` 项目，用 PHF 固化模型信息：

```rust
// core/src/types/model_registry.rs
use phf::phf_map;

pub struct ModelInfo {
    pub provider: &'static str,
    pub context_window: u32,
    pub input_price_per_1m: f64,   // $/1M tokens
    pub output_price_per_1m: f64,
    pub capabilities: &'static [Capability],
}

pub static MODEL_REGISTRY: phf::Map<&'static str, ModelInfo> = phf_map! {
    "gpt-4o" => ModelInfo {
        provider: "openai", context_window: 128000,
        input_price_per_1m: 2.5, output_price_per_1m: 10.0,
        capabilities: &[Capability::ChatCompletion, Capability::Vision, Capability::ToolCalling],
    },
    "claude-sonnet-4-20250514" => ModelInfo {
        provider: "anthropic", context_window: 200000,
        input_price_per_1m: 3.0, output_price_per_1m: 15.0,
        capabilities: &[Capability::ChatCompletion, Capability::Vision, Capability::Thinking],
    },
    // ... 100+ 模型
};
```

或者使用 `build.rs` + JSON 文件动态生成，方便维护。

---

### 阶段 4：智能路由与可靠性 (Week 11-14)

#### 4.1 路由策略框架

参考 litellm 的 Strategy 模式 + bifrost 的高性能实现：

```rust
pub trait RoutingStrategy: Send + Sync {
    fn name(&self) -> &'static str;
    fn select(&self, candidates: &[CandidateChannel], context: &RoutingContext) -> Option<usize>;
}

// 策略实现
pub struct WeightedRandom;           // 当前默认
pub struct LowestLatency;            // P95 延迟最低
pub struct LowestCost;               // 价格最低
pub struct RoundRobin;               // 简单轮询
pub struct RpmTpmAware;              // 配额感知（避让高负载渠道）
pub struct PriorityFallback;         // 按优先级梯队+组内加权

pub struct RoutingContext {
    pub model: String,
    pub estimated_tokens: u32,
    pub user_preference: Option<RoutingPreference>,
    pub latency_requirements: Option<Duration>,
}
```

在 `ChannelRouter` 中通过配置选择策略：
```rust
pub struct ChannelRouter {
    strategies: HashMap<String, Box<dyn RoutingStrategy>>,
    default_strategy: String,
}
```

#### 4.2 多级 Fallback

```rust
pub async fn relay_with_fallback(
    &self, req: &ChatCompletionRequest, plan: &RouteSelectionPlan
) -> ApiResult<ChatCompletionResponse> {
    let mut last_error = None;
    for candidate in &plan.candidates {
        match self.try_relay(req, candidate).await {
            Ok(response) => return Ok(response),
            Err(e) => {
                tracing::warn!(channel_id = candidate.channel_id, error = %e, "fallback to next");
                self.health.record_failure(candidate.channel_id, &e).await;
                last_error = Some(e);

                // 429 错误时增加延迟
                if e.is_rate_limited() {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
            }
        }
    }
    Err(last_error.unwrap_or(ApiError::no_available_channel()))
}
```

#### 4.3 熔断器 (Circuit Breaker)

```rust
pub struct CircuitBreaker {
    state: AtomicU8,       // 0=Closed, 1=Open, 2=HalfOpen
    failure_count: AtomicU32,
    failure_threshold: u32,
    recovery_timeout: Duration,
    last_failure_time: AtomicU64,
}

impl CircuitBreaker {
    pub fn allow_request(&self) -> bool {
        match self.state() {
            State::Closed => true,
            State::Open => {
                if self.elapsed_since_last_failure() > self.recovery_timeout {
                    self.transition(State::HalfOpen);
                    true  // 放行一个探测请求
                } else {
                    false
                }
            }
            State::HalfOpen => false, // 已有探测请求在飞行中
        }
    }

    pub fn record_success(&self) {
        self.failure_count.store(0, Ordering::Relaxed);
        self.transition(State::Closed);
    }

    pub fn record_failure(&self) {
        let count = self.failure_count.fetch_add(1, Ordering::Relaxed) + 1;
        if count >= self.failure_threshold {
            self.transition(State::Open);
            self.last_failure_time.store(now_millis(), Ordering::Relaxed);
        }
    }
}
```

#### 4.4 渠道恢复防抖

防止渠道在"恢复 → 立即失败 → 再恢复"之间反复跳转：

```rust
pub struct ChannelRecoveryJob {
    backoff: HashMap<i64, Duration>, // channel_id → 当前等待时间
}

impl ChannelRecoveryJob {
    pub async fn try_recover(&mut self, channel_id: i64) {
        let wait = self.backoff.get(&channel_id).copied().unwrap_or(Duration::from_secs(300));

        // 探测
        match self.probe(channel_id).await {
            Ok(()) => {
                self.backoff.remove(&channel_id);
                self.enable_channel(channel_id).await;
            }
            Err(_) => {
                // 指数退避: 5min → 10min → 20min → 最大 1h
                let next_wait = (wait * 2).min(Duration::from_secs(3600));
                self.backoff.insert(channel_id, next_wait);
            }
        }
    }
}
```

---

### 阶段 5：流式优化 (Week 12-13)

参考 proxify 的流平滑与心跳保活。

#### 5.1 SSE 心跳保活

防止云厂商 LB (如 AWS ELB) 因 30s 无响应而断开长连接：

```rust
pub fn build_sse_response_with_keepalive(
    upstream: BoxStream<'static, Result<SseEvent>>
) -> impl IntoResponse {
    let heartbeat = tokio_stream::wrappers::IntervalStream::new(
        tokio::time::interval(Duration::from_secs(15))
    ).map(|_| Ok(Event::default().comment("keepalive")));

    let merged = futures::stream::select(
        upstream.map(|event| event.map(|e| Event::default().data(e.data))),
        heartbeat,
    );

    Sse::new(merged).keep_alive(KeepAlive::default())
}
```

#### 5.2 流平滑控制器

```rust
pub struct StreamSmoother {
    min_interval: Duration,  // 最小输出间隔 (如 20ms)
}

impl StreamSmoother {
    pub fn smooth(
        &self, upstream: BoxStream<'static, Result<SseEvent>>
    ) -> BoxStream<'static, Result<SseEvent>> {
        let min_interval = self.min_interval;
        Box::pin(async_stream::stream! {
            let mut last_emit = Instant::now();
            tokio::pin!(upstream);
            while let Some(event) = upstream.next().await {
                let elapsed = last_emit.elapsed();
                if elapsed < min_interval {
                    tokio::time::sleep(min_interval - elapsed).await;
                }
                last_emit = Instant::now();
                yield event;
            }
        })
    }
}
```

---

### 阶段 6：可观测性 (Week 15-16)

#### 6.1 OpenTelemetry 链路追踪

```toml
# hub/Cargo.toml
opentelemetry = "0.28"
opentelemetry-otlp = "0.28"
tracing-opentelemetry = "0.28"
```

```rust
pub fn init_telemetry(endpoint: &str) -> Result<()> {
    let tracer = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(opentelemetry_otlp::new_exporter().tonic().with_endpoint(endpoint))
        .install_batch(opentelemetry_sdk::runtime::Tokio)?;

    let telemetry_layer = tracing_opentelemetry::layer().with_tracer(tracer);
    tracing_subscriber::registry()
        .with(telemetry_layer)
        .with(tracing_subscriber::fmt::layer())
        .init();
    Ok(())
}
```

在中继链路中注入 Span：
```rust
#[tracing::instrument(skip(req), fields(
    request_id = %request_id,
    model = %req.model,
    provider = %channel.provider_name,
    channel_id = %channel.id,
))]
pub async fn relay_request(...) -> ApiResult<Response> { ... }
```

#### 6.2 Prometheus 指标

```rust
use metrics::{counter, histogram, gauge};

// 请求计数
counter!("ai_relay_requests_total", "provider" => provider, "model" => model, "status" => status);

// 延迟分布
histogram!("ai_relay_latency_seconds", "provider" => provider).record(duration.as_secs_f64());

// Token 消耗
counter!("ai_tokens_consumed_total", "direction" => "input", "model" => model).increment(usage.prompt_tokens);
counter!("ai_tokens_consumed_total", "direction" => "output", "model" => model).increment(usage.completion_tokens);

// 成本
counter!("ai_cost_total", "provider" => provider, "model" => model).increment(cost);

// 渠道健康度
gauge!("ai_channel_health", "channel_id" => id.to_string()).set(health_score);
```

#### 6.3 审计日志增强

```rust
pub struct AuditLogger {
    /// 脱敏规则
    redaction_rules: Vec<RedactionRule>,
}

impl AuditLogger {
    pub fn log_request(&self, log: &mut AiLog) {
        // API Key 脱敏: sk-abc...xyz → sk-abc***xyz
        log.api_key = self.redact_api_key(&log.api_key);
        // 可选：请求/响应体脱敏
        if let Some(ref mut body) = log.request_body {
            self.apply_redaction_rules(body);
        }
    }
}
```

---

### 阶段 7：Redis 降级与高可用 (Week 17)

#### 7.1 Redis 降级策略

当前 Redis 失败 → 所有 AI 请求失败。需要分层降级：

```rust
pub struct ResilientCache {
    redis: Option<RedisPool>,
    local_fallback: Cache<String, Vec<u8>>,
}

impl ResilientCache {
    pub async fn get(&self, key: &str) -> Option<Vec<u8>> {
        // 优先 Redis
        if let Some(ref redis) = self.redis {
            match redis.get(key).await {
                Ok(val) => return val,
                Err(e) => {
                    tracing::warn!("redis get failed, falling back to local: {e}");
                    counter!("redis_fallback_total").increment(1);
                }
            }
        }
        // 降级到本地缓存
        self.local_fallback.get(key).await
    }
}
```

| 模块 | Redis 不可用时的行为 |
|---|---|
| 路由缓存 | Fallback 到 DB 直查 |
| Token 缓存 | Fallback 到 DB 直查 |
| 限流 | 放行 + 记录 Warning |
| 计费 | Fallback 到纯 DB 模式 |
| 健康快照 | 使用本地内存缓存 |

#### 7.2 鉴权安全加固

从 fail-open 改为 fail-close：

```rust
fn requires_auth(uri: &Uri) -> bool {
    // Fail-close: 默认需要鉴权
    let exempt_paths = ["/health", "/metrics", "/v1/models"];
    !exempt_paths.iter().any(|p| uri.path() == *p)
}
```

---

### 阶段 8：插件系统 (Week 18-19)

参考 hadrian 的中间件系统和 portkey 的 Guardrails。

#### 8.1 中间件钩子

```rust
#[async_trait]
pub trait RelayMiddleware: Send + Sync {
    fn name(&self) -> &'static str;

    /// 请求前处理（可修改请求或直接拒绝）
    async fn on_request(&self, req: &mut RelayRequest) -> Result<(), AiError> {
        Ok(())
    }

    /// 响应后处理（可修改响应）
    async fn on_response(&self, req: &RelayRequest, resp: &mut RelayResponse) -> Result<(), AiError> {
        Ok(())
    }

    /// 错误处理
    async fn on_error(&self, req: &RelayRequest, error: &AiError) -> Result<(), AiError> {
        Ok(())
    }
}
```

#### 8.2 内置中间件

```rust
// 内容安全过滤
pub struct ContentFilterMiddleware { ... }
// 敏感信息脱敏 (PII)
pub struct PiiRedactionMiddleware { ... }
// 请求/响应日志
pub struct AuditLogMiddleware { ... }
// 自定义 Header 注入
pub struct HeaderInjectionMiddleware { ... }
// 响应缓存 (语义缓存)
pub struct SemanticCacheMiddleware { ... }
```

---

### 阶段 9：Model 层完善 (Week 20)

#### 9.1 软删除一致性

为所有核心实体添加 `deleted_at`：

```rust
// Token 实体添加软删除
pub struct Model {
    // ... existing fields
    pub deleted_at: Option<DateTimeUtc>,
}

// 全局查询 scope
fn active_filter() -> Condition {
    Condition::all().add(Column::DeletedAt.is_null())
}
```

#### 9.2 JSON Schema 校验

为 DTO 中的 `serde_json::Value` 字段定义结构体：

```rust
// 替换 models: serde_json::Value
#[derive(Debug, Serialize, Deserialize)]
pub struct ChannelModels {
    pub supported: Vec<String>,
    pub aliases: HashMap<String, String>,  // 模型别名映射
}

// 替换 config: serde_json::Value
#[derive(Debug, Serialize, Deserialize)]
pub struct ChannelConfig {
    pub base_url: Option<String>,
    pub api_version: Option<String>,
    pub custom_headers: Option<HashMap<String, String>>,
    pub timeout_seconds: Option<u64>,
}
```

#### 9.3 数据库迁移管理

```rust
// model/src/migration/
pub struct Migrator;

impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m001_create_tables::Migration),
            Box::new(m002_add_soft_delete::Migration),
            Box::new(m003_add_channel_relations::Migration),
            // ...
        }
    }
}
```

---

### 阶段 10：质量保障与性能基准 (持续)

#### 10.1 测试覆盖目标

| 模块 | 测试类型 | 最低覆盖率 |
|---|---|---|
| billing.rs | 单元测试 (预扣/结算/退款/崩溃恢复) | 90% |
| rate_limit.rs | 单元测试 (Lua 脚本/并发安全) | 90% |
| channel_router.rs | 单元测试 (策略选择/fallback) | 85% |
| stream.rs | 单元测试 (SSE 解析/UTF-8 安全) | 95% |
| provider/*.rs | 集成测试 (Mock Server) | 80% |
| 完整中继链路 | E2E 测试 | 关键路径 100% |

#### 10.2 性能基准测试

使用 `criterion` 进行基准测试：

```rust
fn bench_sse_parser(c: &mut Criterion) {
    let data = include_bytes!("fixtures/large_stream.bin");
    c.bench_function("sse_parse_10kb", |b| {
        b.iter(|| {
            let mut parser = SseParser::new();
            parser.feed(data).unwrap();
        })
    });
}

fn bench_routing(c: &mut Criterion) {
    c.bench_function("route_selection_100_channels", |b| {
        b.iter(|| {
            router.select_by_priority_and_weight(&candidates_100);
        })
    });
}
```

目标指标（参考 bifrost 的 11µs 和 portkey 的 <1ms）：
- SSE 解析：< 100µs / event
- 路由选择：< 50µs / request
- 完整中继延迟开销：< 1ms（不含上游响应时间）
- 5000 并发下内存增量：< 200MB

#### 10.3 CI/CD Pipeline

```yaml
# .github/workflows/ci.yml
- cargo clippy --all-targets -- -D warnings
- cargo test --workspace
- cargo bench --workspace -- --output-format bencher
- cargo audit  # 漏洞扫描
- cargo deny check  # 许可证检查
```

---

## 四、开发优先级总表

| 优先级 | 阶段 | 周期 | 核心交付物 |
|---|---|---|---|
| **P0** | 阶段 0: 规范建立 | Week 0 | 错误处理规范、Clippy 规则、测试框架 |
| **P0** | 阶段 1: Critical 修复 | Week 1-2 | 6 个 Critical + 3 个 High 漏洞修复 |
| **P1** | 阶段 2: 性能优化 | Week 3-5 | SingleFlight、N+1 修复、批量写入、Core 解耦 |
| **P1** | 阶段 5: 流式优化 | Week 5-6 | SSE 心跳保活、流平滑 |
| **P2** | 阶段 3: Provider 扩张 | Week 6-10 | Bedrock/Vertex/DeepSeek/Groq 等 10+ Provider |
| **P2** | 阶段 4: 智能路由 | Week 11-14 | 多策略路由、Fallback、熔断器 |
| **P2** | 阶段 7: Redis 降级 | Week 14 | 分层降级策略、鉴权 fail-close |
| **P3** | 阶段 6: 可观测性 | Week 15-16 | OpenTelemetry、Prometheus、审计日志 |
| **P3** | 阶段 8: 插件系统 | Week 18-19 | 中间件钩子、内容安全、PII 脱敏 |
| **P3** | 阶段 9: Model 完善 | Week 20 | 软删除、JSON Schema、迁移管理 |
| **持续** | 阶段 10: 质量保障 | 全程 | 测试覆盖、性能基准、CI/CD |

---

## 五、技术选型参考

| 用途 | 推荐库 | 替代方案 |
|---|---|---|
| HTTP 框架 | `axum 0.8` | - |
| 异步运行时 | `tokio` | - |
| 数据库 ORM | `sea-orm` (已用) | `sqlx` |
| Redis 客户端 | `redis` (已用) | `fred` |
| 本地缓存 | `moka` | `quick-cache` |
| Token 计算 | `tiktoken-rs` | - |
| AWS 签名 | `aws-sigv4` | `rusoto` |
| 链路追踪 | `opentelemetry-otlp` | - |
| 指标暴露 | `metrics` + `metrics-exporter-prometheus` | - |
| SSE 解析 | 自研 `SseParser` | `eventsource-stream` |
| 静态元数据 | `phf` | `include_str!` + `serde_json` |
| 基准测试 | `criterion` | `divan` |
| 熔断器 | 自研 | `recloser` |
| PII 脱敏 | `aho-corasick` | - |
