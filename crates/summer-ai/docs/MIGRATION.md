# summer-ai 迁移方案：全量照搬 genai + ironclaw

> **前提**：两个源库已 clone 到项目内
> - `docs/relay/rust/rust-genai/` —— 协议适配框架（19 adapters + canonical types）
> - `docs/relay/rust/ironclaw/` —— 运行时保护（retry / failover / circuit_breaker / cost / cache）
>
> 本文档描述**文件级别**的迁移映射。完成后 summer-ai 就是"genai 的 ZST 协议层 + ironclaw 的韧性层 + 我们的 DB 驱动 Router"。

---

## 0. 迁移总览

| 层 | 源 | 目标 | 行数（源） | 策略 |
|---|---|---|---|---|
| canonical types | `genai/src/chat/` + `common/` + `embed/` | `core/src/types/` | ~2400 | **全量拷贝** |
| adapter 框架 | `genai/src/adapter/` | `core/src/adapter/` | ~1200 | **全量拷贝** |
| 19 个 adapter | `genai/src/adapter/adapters/` | `core/src/adapter/adapters/` | ~9200 | **全量拷贝** |
| resolver | `genai/src/resolver/` | `core/src/resolver/` | ~630 | **全量拷贝** |
| webc | `genai/src/webc/` | `core/src/webc/` | ~660 | **全量拷贝** |
| Client | `genai/src/client/` | —— | ~1150 | **不搬**（relay 不需要 SDK Client） |
| 计费模型 | `ironclaw/src/llm/costs.rs` | `core/src/cost/` | 196 | **拷贝 + 改绑 DB** |
| retry | `ironclaw/src/llm/retry.rs` | `relay/src/middleware/retry.rs` | 524 | **拷贝** |
| failover | `ironclaw/src/llm/failover.rs` | `relay/src/router/failover.rs` | 1339 | **拷贝 + 改绑 DB** |
| circuit_breaker | `ironclaw/src/llm/circuit_breaker.rs` | `relay/src/middleware/breaker.rs` | 786 | **拷贝** |
| response_cache | `ironclaw/src/llm/response_cache.rs` | `relay/src/middleware/cache.rs` | 795 | **拷贝** |
| smart_routing | `ironclaw/src/llm/smart_routing.rs` | `relay/src/router/smart.rs` | 1852 | **拷贝 + 改绑 DB** |

**总搬运量**：核心 ~14k 行（genai）+ 保护层 ~5.5k 行（ironclaw）= **~20k 行**。

---

## 1. 最终目录形态

```
crates/summer-ai/
├── Cargo.toml
├── core/                                 # 【协议层：纯 genai 移植】
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs                        # 重新导出
│       ├── error.rs                      # 保留当前 + 合并 genai/error.rs
│       ├── types/                        # ≈ genai/src/chat + common + embed
│       │   ├── mod.rs
│       │   ├── common/                   # 跨协议共享
│       │   │   ├── message.rs            ← chat/chat_message.rs
│       │   │   ├── content_part/         ← chat/content_part/
│       │   │   ├── message_content.rs    ← chat/message_content.rs
│       │   │   ├── tool/                 ← chat/tool/
│       │   │   ├── usage.rs              ← chat/usage.rs
│       │   │   ├── stream_event.rs       ← chat/chat_stream.rs
│       │   │   ├── binary.rs             ← chat/binary.rs
│       │   │   ├── model_iden.rs         ← common/model_iden.rs (改：去 Adapter 推断)
│       │   │   └── model_name.rs         ← common/model_name.rs
│       │   ├── openai/                   # OpenAI 线协议 wire 类型
│       │   │   ├── chat.rs               ← (保留现有扁平版)
│       │   │   └── model.rs
│       │   └── embed/                    ← embed/
│       │       ├── embed_options.rs
│       │       ├── embed_request.rs
│       │       └── embed_response.rs
│       ├── adapter/                      ← genai/src/adapter/
│       │   ├── mod.rs                    (保留我们现在的 Adapter trait)
│       │   ├── kind.rs                   ← adapter_kind.rs（改：去 from_model）
│       │   ├── dispatcher.rs             ← dispatcher.rs（改：去 Client 相关方法）
│       │   ├── adapter_types.rs          ← adapter_types.rs
│       │   ├── inter_stream.rs           ← inter_stream.rs
│       │   └── adapters/                 ← adapters/ 全量 19 个
│       │       ├── mod.rs
│       │       ├── support.rs
│       │       ├── openai/{mod,adapter_impl,adapter_shared,streamer,embed}.rs
│       │       ├── openai_resp/{mod,adapter_impl,streamer}.rs
│       │       ├── anthropic/...
│       │       ├── gemini/...
│       │       ├── groq/...
│       │       ├── cohere/...
│       │       ├── ollama/...
│       │       ├── ollama_cloud/...
│       │       ├── xai/...
│       │       ├── deepseek/...
│       │       ├── fireworks/...
│       │       ├── together/...
│       │       ├── nebius/...
│       │       ├── zai/...
│       │       ├── bigmodel/...
│       │       ├── aliyun/...
│       │       ├── mimo/...
│       │       ├── github_copilot/...
│       │       └── vertex/...
│       ├── resolver/                     ← resolver/
│       │   ├── mod.rs
│       │   ├── auth.rs                   ← auth_data.rs (重命名)
│       │   ├── auth_resolver.rs          ← auth_resolver.rs
│       │   ├── endpoint.rs               ← endpoint.rs
│       │   ├── model_mapper.rs           ← model_mapper.rs
│       │   ├── service_target.rs         (保留我们现有)
│       │   ├── service_target_resolver.rs ← service_target_resolver.rs
│       │   └── error.rs                  ← error.rs
│       ├── webc/                         ← webc/  （发送 + SSE 解析）
│       │   ├── mod.rs
│       │   ├── web_client.rs
│       │   ├── web_stream.rs
│       │   ├── event_source_stream.rs
│       │   └── error.rs
│       └── cost/                         ← ironclaw/src/llm/costs.rs
│           └── mod.rs                    (PriceTable, CostProfile 扩展版)
│
├── model/                                # 【DB Entity：保持不动，已齐全】
│   └── src/entity/channels/              (channel, channel_account, ...)
│
└── src/                                  # 【Relay 运行时：我们自己写 + 搬 ironclaw 韧性层】
    ├── lib.rs                            SummerAiPlugin
    ├── config.rs
    ├── router/
    │   ├── mod.rs
    │   ├── chat.rs                       /v1/chat/completions handler
    │   ├── models.rs                     /v1/models
    │   ├── channel_store.rs              DB→内存缓存 + picker
    │   ├── channel_type_map.rs           ChannelType(i16) → AdapterKind
    │   ├── failover.rs                   ← ironclaw failover.rs
    │   └── smart.rs                      ← ironclaw smart_routing.rs
    └── middleware/
        ├── retry.rs                      ← ironclaw retry.rs
        ├── breaker.rs                    ← ironclaw circuit_breaker.rs
        └── cache.rs                      ← ironclaw response_cache.rs
```

---

## 2. 业务表字段 ↔ 代码类型 映射矩阵

这是"贴合我们业务"的**核心**——搬过来的代码必须从我们的 DB 取数据，不能再读 env var 或硬编码。

| 业务表字段 | 代码类型 | 映射方式 |
|---|---|---|
| `ai.channel.channel_type: i16` | `AdapterKind` | `core/src/adapter/channel_type_map.rs` 提供 `TryFrom<i16>` |
| `ai.channel.base_url: String` | `Endpoint` | `Endpoint::from_owned(row.base_url)` |
| `ai.channel.extra_headers: JSONB` | `ServiceTarget.extra_headers: BTreeMap<String,String>` | 直接反序列化 |
| `ai.channel.models: JSONB` (array) | ChannelStore 索引 | `model → Vec<channel_id>` 倒排 |
| `ai.channel.model_mapping: JSONB` (obj) | `ServiceTarget.actual_model` | `mapping[logical] ?? logical` |
| `ai.channel.capabilities: JSONB` (可能需新增) | `Capabilities` override | 覆盖 `Adapter::capabilities()` |
| `ai.channel.priority: i32` | `ChannelRouter::pick()` 排序 | 高优先走前面 |
| `ai.channel.weight: i32` | 加权随机选择 | 同优先级里权重随机 |
| `ai.channel.status: i16` | `ChannelStore` 过滤 | 只加载 Enabled |
| `ai.channel_account.credentials: JSONB` | `AuthData::Single(...)` | `credentials["api_key"]` → `AuthData::from_single` |
| `ai.channel_account.credential_type: i16` | 决定如何解析 credentials | type=1 api_key / type=2 oauth / ... |
| `ai.channel_account.schedulable: bool` | `ChannelStore` 过滤 | 只选 `schedulable=true` |
| `ai.channel_account.rate_limited_until: TIMESTAMP` | `ChannelStore` 过滤 | `> now()` 的跳过 |
| `ai.channel_account.status: i16` | `ChannelStore` 过滤 | 只选 Enabled |
| `ai.channel_account.weight: i32` | 权重随机 | 同 channel 多 account 选其一 |
| `ai.channel_model_price.*` | `PriceTable` | Router 装配 `ServiceTarget` 时顺带把 price 挂上 |
| `ai.channel_probe.*` | 不在 Router 读路径 | 独立子系统（健康探测） |
| `ai.routing_rule` / `routing_target` | `RoutingStrategy` | smart_routing 读取规则 |

**关键约定**：
- `ChannelType` enum 和 `AdapterKind` enum **一对一映射**。我们的 `ChannelType` 目前 7 个变体（OpenAi/Anthropic/Azure/Baidu/Ali/Gemini/Ollama），`AdapterKind` 搬完有 19 个。**多出的 12 个要不要加到 ChannelType？** 由业务决定——Fireworks/Together/Groq/DeepSeek/Xai/Zai/BigModel/Mimo/Nebius/GithubCopilot/Vertex/OllamaCloud/OpenAIResp，加法兼容的，值可以留空缺等业务配置时填（见步骤 E1）。
- `OpenAICompat` 这个"兼容代理"桶：genai **没有**（它只分官方），我们保留现在的 `OpenAICompat` 变体，给"所有 OpenAI 兼容但不在清单里的"兜底用。

---

## 3. 步骤分解（执行顺序）

总共 **7 大步骤**，每步都独立可编译可测试。

### 步骤 C1 — 拷贝 canonical types（~2400 行）

**做**：
1. 把 `genai/src/chat/*` 搬到 `core/src/types/common/`
   - `chat_message.rs` → `common/message.rs`
   - `chat_stream.rs` → `common/stream_event.rs`
   - `chat_options.rs`、`chat_req_response_format.rs`、`chat_request.rs`、`chat_response.rs` → **不搬**（我们的 `openai/chat.rs` 已经是扁平 OpenAI 官方字段，更贴切）
   - `tool/` 整目录 → `common/tool/`
   - `content_part/` 整目录 → `common/content_part/`
   - `usage.rs`, `binary.rs`, `printer.rs`, `message_content.rs` → `common/`
2. `genai/src/common/*` → `core/src/types/common/`（`model_iden.rs`、`model_name.rs`）
   - **改**：`ModelIden` 的 `AdapterKind` 字段保留，但构造方式从 `from_model()` 改成"从业务表读出来"
3. `genai/src/embed/*` → `core/src/types/embed/`（全量）

**替换**：全局 `use genai::` → `use crate::`（sed 一遍）

**产出验证**：`cargo build -p summer-ai-core`。

---

### 步骤 C2 — 拷贝 adapter 框架（~1200 行）

**做**：
1. `genai/src/adapter/adapter_kind.rs` → `core/src/adapter/kind.rs`
   - **保留** 19 个变体 + `as_str` / `as_lower_str` / `from_lower_str` / `default_key_env_name`
   - **删掉** `from_model()`（relay 不从 model 名推断 adapter，由 DB 决定）
   - **删掉** `from_model_namespace()`（同上）
   - **新增**：`TryFrom<i16> for AdapterKind`（对应 `ai.channel.channel_type`）
2. `genai/src/adapter/adapter_types.rs` → `core/src/adapter/adapter_types.rs`（`WebRequestData`、`ServiceType` 等）
3. `genai/src/adapter/dispatcher.rs` → `core/src/adapter/dispatcher.rs`
   - **保留**：`default_endpoint`、`default_auth`、`to_web_request_data`、`to_chat_response`、`to_chat_stream`、`to_embed_request_data`、`to_embed_response`
   - **删掉**：所有 `Client` 相关的方法（我们不走 Client 层）
4. `genai/src/adapter/inter_stream.rs` → `core/src/adapter/inter_stream.rs`（流式内部状态机）
5. `genai/src/adapter/mod.rs` → 合入 `core/src/adapter/mod.rs`
   - **保留当前的** `Adapter` trait（我们已经写得很清楚了）
   - **新增**：`capabilities()` / `cost_profile()` 方法（现在就有）

**产出验证**：`cargo build -p summer-ai-core`（此时 `adapters/` 还只有 openai，所以 dispatcher match 会编译失败；临时用 `todo!()` 占位或者直接进入 C3）。

---

### 步骤 C3 — 拷贝 19 个 adapter（~9200 行，最大块）

**做**：把 `genai/src/adapter/adapters/` **整目录**搬到 `core/src/adapter/adapters/`。

每个 adapter 目录里：
- `mod.rs` 导出 ZST struct
- `adapter_impl.rs` 实现 `Adapter` trait
- `adapter_shared.rs`（如有）共享工具
- `streamer.rs`（如有）SSE 解析

**全局替换**：
```
use genai::chat::      → use crate::types::common::
use genai::adapter::   → use crate::adapter::
use genai::resolver::  → use crate::resolver::
use genai::webc::      → use crate::webc::
use genai::common::    → use crate::types::common::
use genai::{Error, Result} → use crate::error::{AdapterError, AdapterResult}
use genai::ServiceTarget → use crate::resolver::ServiceTarget
use genai::ModelIden → use crate::types::common::ModelIden
```

**Adapter trait 签名差异**：
- genai 的 `Adapter::to_web_request_data` 第一个参数是 `ModelIden`，我们是 `&ServiceTarget`
- 修复方式：每个 adapter 内部把 `ServiceTarget.actual_model` 当做 genai 语境下的 `ModelIden.model_name`
- 简单 sed：`model_iden.model_name` → `target.actual_model.as_str()`

**保留我们的扁平 ChatRequest**：genai 用它自己的 `ChatRequest` + `ChatOptionsSet`；我们用 `types::openai::ChatRequest`（扁平）。映射层：
- 非 OpenAI 家族的 adapter（Anthropic / Gemini / Cohere / Ollama）内部自己做 "OpenAI ChatRequest → 自家 wire format" 的转换，这部分 genai 代码**完全可以照抄**，只是入口类型换成我们的扁平 `ChatRequest`
- 影响的是 adapter 内部的 `to_web_request_data` 前几行：`req.messages` 取法一样，`req.temperature` 取法从 `options.temperature` 变成 `request.temperature`

**产出验证**：`cargo build -p summer-ai-core` 全部 19 adapter 通过编译。

---

### 步骤 C4 — 拷贝 resolver + webc（~1300 行）

**resolver**：
1. `genai/src/resolver/auth_data.rs` → `core/src/resolver/auth.rs`（**我们现有版本可以全量替换**，genai 版本更完整）
2. `auth_resolver.rs` → `core/src/resolver/auth_resolver.rs`（**全量拷贝**）
3. `endpoint.rs` → **保留现有**（我们的和 genai 的一致）
4. `model_mapper.rs` → `core/src/resolver/model_mapper.rs`（**全量拷贝**）
5. `service_target_resolver.rs` → `core/src/resolver/service_target_resolver.rs`（**全量拷贝**，后面 Router 会 `impl ServiceTargetResolver for DbChannelRouter`）
6. `error.rs` → `core/src/resolver/error.rs`（合入现有 `AuthResolveError`）

**webc**：
1. `genai/src/webc/web_client.rs` → `core/src/webc/web_client.rs`（**全量拷贝**，是个 reqwest 薄包装）
2. `web_stream.rs` → `core/src/webc/web_stream.rs`（**全量拷贝**，SSE 解析主力）
3. `event_source_stream.rs` → 同名拷贝
4. `error.rs` → 同名拷贝（合入 `AdapterError`）

**产出验证**：`cargo build -p summer-ai-core` 全部 resolver / webc 通过。

---

### 步骤 C5 — 拷贝 ironclaw cost model（~200 行）

**做**：
1. `ironclaw/src/llm/costs.rs` → `core/src/cost/mod.rs`
   - 主要类型：`PriceTable`、`CostCalculator`、`TokenCost`
2. **改造**：把 `PriceTable` 从"内存 HashMap 写死价格"改成"**从 `ai.channel_model_price` 加载**"
3. 新增 `PriceBook` 结构，`ChannelStore` 启动时 `load_price_book(&db)` 一次性加载所有 price 行 → `PriceBook { by_channel_model: BTreeMap<(i64, String), PriceTable> }`

**映射**：
- `channel_model_price.input_price_per_1k` → `PriceTable.input_per_1k`
- `channel_model_price.output_price_per_1k` → `PriceTable.output_per_1k`
- `channel_model_price.cache_write_price_per_1k` → `PriceTable.cache_write_per_1k`
- `channel_model_price.cache_read_price_per_1k` → `PriceTable.cache_read_per_1k`
- `CostProfile.cache_write_multiplier` 不从 DB 读，走 `Adapter::cost_profile()` 协议常量

**产出验证**：`cargo build -p summer-ai-core`。

---

### 步骤 E1 — 扩展业务表 + ChannelType 变体（~50 行）

**做**：
1. `model/src/entity/channels/channel.rs` 的 `ChannelType` enum 扩到 19 变体（和 `AdapterKind` 对齐）
   ```rust
   OpenAi=1, Anthropic=3, Azure=14, Baidu=15, Ali=17, Gemini=24, Ollama=28,
   // 新增：
   OpenAIResp=40, Fireworks=41, Together=42, Groq=43, DeepSeek=44, Xai=45,
   Zai=46, BigModel=47, Mimo=48, Nebius=49, Cohere=50, OllamaCloud=51,
   Vertex=52, GithubCopilot=53, OpenAICompat=99,
   ```
2. 新增字段（如缺）：
   - `ai.channel.capabilities JSONB DEFAULT '{}'::jsonb`
   - `ai.channel.extra_headers JSONB DEFAULT '{}'::jsonb`（如 ServiceTarget 需要）
3. 新增文件 `core/src/adapter/channel_type_map.rs`：
   ```rust
   impl TryFrom<ChannelType> for AdapterKind { ... }
   impl From<AdapterKind> for ChannelType { ... }
   ```

**SQL 迁移**：`sql/ai/migrations/20260420_extend_channel_type.sql` 新增字段 + 更新注释。

---

### 步骤 R1 — 搬 ironclaw 韧性层（~5500 行）

**到这一步 core 已经完备**，接下来是 Relay 运行时保护。按依赖顺序：

1. **`relay/src/middleware/retry.rs`** ← `ironclaw/src/llm/retry.rs`（524 行）
   - 指数退避 + 抖动
   - 只重试 5xx / 429 / 网络错
   - **不改**：ironclaw 这部分几乎不依赖业务，直接拷

2. **`relay/src/middleware/breaker.rs`** ← `ironclaw/src/llm/circuit_breaker.rs`（786 行）
   - 熔断器
   - **改**：把状态存储从"进程内 AtomicU64"挂到"按 `channel_account_id` key 的 DashMap"，对应我们 N 个账号
   - **关联业务表**：`channel_account.rate_limited_until`（熔断打开时写回 DB，让 ChannelStore 下次刷新感知到）

3. **`relay/src/middleware/cache.rs`** ← `ironclaw/src/llm/response_cache.rs`（795 行）
   - 相同 prompt 命中缓存
   - **改**：后端从"moka 内存"可选升级为"redis"（如有 redis 就用，否则 moka）
   - 第一版直接 moka，保留可换后端

4. **`relay/src/router/failover.rs`** ← `ironclaw/src/llm/failover.rs`（1339 行）
   - 多 channel/account 失败轮转
   - **改**：候选列表来源从"配置文件"改为"`ChannelStore::pick_candidates(model)`"，返回 `Vec<(Channel, ChannelAccount)>` 按优先级排好

5. **`relay/src/router/smart.rs`** ← `ironclaw/src/llm/smart_routing.rs`（1852 行）
   - 根据 token 数 / 延迟 / 成本动态选 channel
   - **改**：规则源从"toml 配置"改为"`ai.routing_rule` + `ai.routing_target` 两张表读出来"
   - 最复杂一块，**最后做**

**顺序建议**：retry → breaker → cache → failover → smart。每步独立加 feature flag，默认关，逐步打开。

---

## 4. 不搬的清单（明确边界）

| 源文件 | 不搬原因 |
|---|---|
| `genai/src/client/*` | SDK 风格 Client，我们 Handler 直接组装 WebRequest 更直接 |
| `genai/src/adapter/adapter_kind.rs::from_model` | Relay 不按 model 名猜 adapter |
| `ironclaw/src/llm/provider.rs` | 实例化 Provider trait 风格，和 genai ZST 冲突 |
| `ironclaw/src/llm/{oauth_helpers,codex_*,anthropic_oauth,gemini_oauth}.rs` | OAuth 流程太重，v1 先用 API key |
| `ironclaw/src/llm/{bedrock,github_copilot_auth}.rs` | 特定厂商登录流程，等业务上这些 channel 时再搬 |
| `ironclaw/src/llm/rig_adapter.rs` | rig 生态对接，和我们无关 |
| `ironclaw/src/llm/{reasoning_models,vision_models,image_models}.rs` | 硬编码 model 清单，我们用 DB 驱动 |
| `ironclaw/src/llm/recording.rs` | HTTP 录放回，放测试工具里 |
| `ironclaw/src/llm/registry.rs` | 内存 Provider 注册表，我们 DB 就是 registry |
| `ironclaw/src/llm/session.rs` | 会话状态，relay 不做会话层 |

---

## 5. 完整验证路径

按 **7 步一步一验证**：

```bash
# C1
cargo build -p summer-ai-core  # types 通过

# C2
cargo build -p summer-ai-core  # adapter 框架通过（adapters/ 临时占位）

# C3
cargo build -p summer-ai-core  # 19 adapter 全部编译通过
cargo test -p summer-ai-core   # 跑 genai 带过来的原装测试（adapter 各自有测试）

# C4
cargo build -p summer-ai-core  # resolver + webc 通过

# C5
cargo build -p summer-ai-core  # cost 通过

# E1
cargo build -p summer-ai-model # ChannelType 扩充 + TryFrom 通过
psql -d summer_dev -f sql/ai/migrations/20260420_extend_channel_type.sql

# R1（分 5 小步）
cargo build -p summer-ai  # 每搬一个 middleware 编译一次
```

**端到端**（R1 完成后）：

```bash
# 启动
cargo run -p app

# 插数据
psql -c "INSERT INTO ai.channel(name, channel_type, base_url, models, status) VALUES
 ('wzw', 1, 'https://wzw.pp.ua/v1', '[\"gpt-4o-mini\"]'::jsonb, 1),
 ('anthropic', 3, 'https://api.anthropic.com', '[\"claude-sonnet-4\"]'::jsonb, 1);"
psql -c "INSERT INTO ai.channel_account(channel_id, credentials, status, schedulable) VALUES
 (1, '{\"api_key\":\"sk-xxx\"}'::jsonb, 1, true),
 (2, '{\"api_key\":\"sk-ant-xxx\"}'::jsonb, 1, true);"

# 测 OpenAI
curl -X POST http://localhost:8080/v1/chat/completions -H 'Content-Type: application/json' \
  -d '{"model":"gpt-4o-mini","messages":[{"role":"user","content":"hi"}]}'

# 测 Anthropic（走我们的 OpenAI 兼容入口，adapter 翻译成 Anthropic 协议）
curl -X POST http://localhost:8080/v1/chat/completions -H 'Content-Type: application/json' \
  -d '{"model":"claude-sonnet-4","messages":[{"role":"user","content":"hi"}]}'

# 测失败 failover：故意插错 key，观察换 account
# 测 retry：临时关 wzw，观察重试
# 测 cache：两次一样的请求，第二次日志 cache_hit=true
```

---

## 6. 工程量 & 时间表

| 步 | 代码量（含测试） | 预估耗时 |
|---|---|---|
| C1 types | 2400 | 0.5 天 |
| C2 adapter 框架 | 1200 | 0.25 天 |
| C3 19 adapters | 9200 | **1.5 天**（主要是 sed 替换 + 编译错误修） |
| C4 resolver + webc | 1300 | 0.5 天 |
| C5 cost | 200 | 0.25 天 |
| E1 DB 扩展 | 150 | 0.25 天 |
| R1 retry | 550 | 0.25 天 |
| R1 breaker | 800 | 0.5 天 |
| R1 cache | 800 | 0.5 天 |
| R1 failover | 1400 | 0.75 天 |
| R1 smart_routing | 1900 | 1 天 |
| **合计** | **~20000** | **~6.5 天** |

---

## 7. 风险 & 规避

| 风险 | 规避 |
|---|---|
| genai 的 ChatRequest 和我们扁平版不同，adapter 内部取字段方式不同 | C3 里最费时的环节；写一个 helper `fn opts(req: &ChatRequest) -> OptionsView` 把扁平字段聚合成 genai 风格的视图，最小化改动 |
| ironclaw 依赖的 crate 版本和我们 workspace 不一致 | 逐个 `cargo.toml` 对账，优先用我们 workspace 版本 |
| ironclaw failover 依赖它自己的 Provider trait | R1-failover 时改成 `Box<dyn ServiceTargetResolver>` 接口，接我们的 ChannelRouter |
| ironclaw code 可能 GPL / AGPL 引入合规问题 | 动手前先看 `ironclaw/LICENSE-MIT` 和 `LICENSE-APACHE`（已看到是 MIT + Apache 双 license，安全） |
| 20k 行 copy-paste 的编译错误会爆表 | **严格按步骤** C1→C5 走，每步编译干净了再下一步。不要一次性全搬 |

---

## 8. 下一步决策点

**需要你拍板**：

1. **`ChannelType` 扩展方式**：
   - (a) 直接改 enum，加 12 个变体（我倾向）
   - (b) 保留现状，用 `OpenAICompat` + `channel.vendor_code` 字段区分厂商
2. **Client 层要不要留一个薄的**：
   - (a) 不留，Handler 直接 `http.post(data.url)...`（当前做法，我倾向）
   - (b) 留一个 `RelayClient` 封装 retry + breaker + cache，Handler 调一个方法
3. **执行起点**：C1 先搬 types，还是先搬 C3 adapters（C3 依赖 C1+C2）？
4. **ironclaw 韧性层本轮要不要全搬**：
   - (a) 本轮只搬到 C5（协议层完备），韧性层分阶段（我倾向）
   - (b) 一次全搬到 R1 最后，上来就有 retry + breaker + cache

给我你的选择，我就按顺序动手。
