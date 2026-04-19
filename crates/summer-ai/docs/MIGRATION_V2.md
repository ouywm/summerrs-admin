# summer-ai 迁移规划 v2（执行手册）

> **契约文档**：[`ARCHITECTURE.md`](./ARCHITECTURE.md)（trait / 目录 / 切分的设计）
> **本文档**：按 Phase 拆解的**可执行步骤**，每 Phase 独立编译 + 独立验证。
>
> v1 ([`MIGRATION.md`](./MIGRATION.md)) 保留作历史，本文档取代它指导执行。

更新日期：2026-04-19

---

## 0. 总体思路

### 0.1 执行原则

1. **结构照搬 `feature/extract-summer-ai` 分支**：6 个 sub-crate 切分（去掉 hub，5 个）
2. **代码混合来源**：
   - **Entity 层**：从分支搬（已写好 80+ 表）
   - **Core 协议层**：从 genai 搬（19 adapter），签名按 ARCHITECTURE 精简
   - **Relay / Admin / Billing 业务层**：从分支搬代码结构，按需调整
3. **绝不大爆炸**：每 Phase 结束 `cargo build` 通过 + 可独立 smoke test
4. **两条主轴**：
   - **协议完备度**：能对接多少家上游（核心 → 2 家 → 5 家 → 19 家）
   - **业务完备度**：能跑通多少链路（裸转发 → 带 token → 带计费 → 带日志 → 带 admin）
   - 先广度铺开最小链路，再沿两轴逐步加料

### 0.2 Phase 概览

| Phase | 目标 | 天数 | 产出 |
|---|---|---|---|
| **P0** | 结构骨架 | 0.25 | 5 个 sub-crate 编译通过（空实现 + 依赖图） |
| **P1** | Core 协议层（只做 OpenAI） | 0.75 | `build → send → parse` 链路能跑通 1 个 curl |
| **P2** | Model entity（核心域） | 0.5 | channels + requests + platform 三个域 Entity 可用 |
| **P3** | Relay 最小链路 | 1 | curl 到本地 /v1/chat/completions 成功转发（硬编码 channel） |
| **P3.5** | 多入口协议（Claude+Gemini） | 0.75 | Claude SDK / Gemini SDK 能直接请求本 relay |
| **P4** | Relay DB 化 | 0.75 | ChannelRouter 从 DB 选 channel/account |
| **P5** | Auth + Log | 0.75 | API Token 鉴权 + 请求日志写入 |
| **P6** | Billing | 1 | 三阶段扣费 + group_ratio |
| **P7** | Admin CRUD | 1 | 后台可管理 channel / token / price |
| **P8** | 多 Adapter | 0.75 | Anthropic / Gemini / Ollama / Cohere 全上（5 个） |
| **P9** | 韧性层 | 1.25 | retry / failover / circuit_breaker |
| **P10** | 其余 14 个 adapter | 0.75 | 19 adapter 全上 |
| **P11** | TaskAdapter 异步任务 | 1.5 | Midjourney 等长任务 + 三阶段计费 |

**预计总工期：~11.25 天**（分支的 60% 完成度基础上）。

---

## P0: 结构骨架（0.25 天）

### 目标

建好 5 个 sub-crate 的**最小可编译骨架**，依赖图清晰。

### 步骤

1. **清理当前 `crates/summer-ai/src/` 下的旧内容**
   - 保留 `router/chat.rs` 做参考（稍后改到 `relay/src/router/`）
   - 其他如 llm/ 旧文件删除

2. **建 `core/model/relay/admin/billing` 5 个 sub-crate**
   - `crates/summer-ai/core/` **保留当前**（已有最简骨架）
   - `crates/summer-ai/model/` **保留当前**（已有 channels/ entity）
   - `crates/summer-ai/relay/Cargo.toml` + `src/lib.rs`（空 Plugin）
   - `crates/summer-ai/admin/Cargo.toml` + `src/lib.rs`（空 Plugin）
   - `crates/summer-ai/billing/Cargo.toml` + `src/lib.rs`（空 Plugin）

3. **更新 `summer-ai/Cargo.toml` 为 workspace root + 主 crate**
   ```toml
   [package]
   name = "summer-ai"
   version = "0.0.1"
   edition = "2024"

   [workspace]
   members = ["core", "model", "relay", "admin", "billing"]

   [dependencies]
   summer-ai-core = { path = "core" }
   summer-ai-model = { path = "model" }
   summer-ai-relay = { path = "relay" }
   summer-ai-admin = { path = "admin" }
   summer-ai-billing = { path = "billing" }
   summer = { workspace = true }
   summer-web = { workspace = true }
   ```

4. **根 `/Cargo.toml`** `[workspace.dependencies]` 添加：
   ```toml
   summer-ai-core = { path = "crates/summer-ai/core" }
   summer-ai-model = { path = "crates/summer-ai/model" }
   summer-ai-relay = { path = "crates/summer-ai/relay" }
   summer-ai-admin = { path = "crates/summer-ai/admin" }
   summer-ai-billing = { path = "crates/summer-ai/billing" }
   ```

5. **主 `summer-ai/src/lib.rs`**：聚合 5 个 Plugin
   ```rust
   pub use summer_ai_admin::SummerAiAdminPlugin;
   pub use summer_ai_billing::SummerAiBillingPlugin;
   pub use summer_ai_core;
   pub use summer_ai_model;
   pub use summer_ai_relay::SummerAiRelayPlugin;

   /// 用户可一行注册所有 sub-plugin
   pub struct SummerAiPlugin;

   #[async_trait]
   impl Plugin for SummerAiPlugin {
       async fn build(&self, app: &mut AppBuilder) {
           SummerAiRelayPlugin.build(app).await;
           SummerAiAdminPlugin.build(app).await;
           SummerAiBillingPlugin.build(app).await;
       }
       fn name(&self) -> &str { "summer_ai::SummerAiPlugin" }
   }
   ```

### 验证

```bash
cargo build -p summer-ai-core -p summer-ai-model \
            -p summer-ai-relay -p summer-ai-admin -p summer-ai-billing -p summer-ai
# 期望：5 个 sub-crate + 主 crate 全部编译通过（相互链接也通过）
```

---

## P1: Core 协议层——只做 OpenAI（0.75 天）

### 目标

按 ARCHITECTURE 定义的**精简 Adapter trait**，实现 **OpenAIAdapter** 一个，能组装请求 + 解析响应 + 解析流式。

### 步骤

1. **搬 canonical types**（`core/src/types/common/` + `openai/`）
   - `common/message.rs`、`tool.rs`、`usage.rs`、`stream_event.rs`、`binary.rs`
     - 从 genai `chat/chat_message.rs`、`chat/tool/*`、`chat/usage.rs`、`chat/chat_stream.rs` 拷贝类型定义
     - **去掉**：`MessageOptions`（cache control 放 ServiceTarget 后期处理）、`ContentPart::ReasoningContent`（v1 先不管 reasoning）
     - 保留：`ChatRole`、`ChatMessage`、`MessageContent`、`ContentPart::{Text, ImageUrl, InputAudio, ToolCall}`、`ToolCall`、`Tool`、`ToolChoice`、`Usage`、`FinishReason`、`ChatStreamEvent`、`StreamEnd`
   - `openai/chat.rs` **保留当前**（25+ 扁平字段的 ChatRequest，已对齐官方）

2. **写 `core/src/resolver/`**
   - `auth.rs`：`AuthData::{None, Single, FromEnv}` — 当前已有，保留
   - `endpoint.rs`：`Endpoint(Arc<String>)` + `trimmed()` — 当前已有，保留
   - `target.rs`：按 ARCHITECTURE §3.3 扩展 `ServiceTarget`（加 `logical_model`, `channel_id`, `channel_account_id`, `capabilities_override`）

3. **写 `core/src/adapter/`**
   - `mod.rs`：Adapter trait（按 ARCHITECTURE §3.1 精简版）、Capabilities、CostProfile、ServiceType、WebRequestData
   - `kind.rs`：AdapterKind enum（19 变体）+ `as_str`/`as_lower_str`/`from_lower_str`
   - `channel_type_map.rs`：`TryFrom<i16> for AdapterKind` + 反向
   - `dispatcher.rs`：静态 match 分派（只实现 OpenAI 分支，其他变体 `unimplemented!`）
   - `adapters/mod.rs`：`pub mod openai;`
   - `adapters/openai.rs`：完整实现
     - 参考 genai `adapter/adapters/openai/adapter_shared.rs`（~545 行）
     - **改签名**：`build_chat_request(target, service, req)` / `parse_chat_response(target, body)` / `parse_chat_stream_event(target, raw)`
     - **字段取法改扁平**：`req.temperature` 直接取，不走 `options_set.temperature()`
   - `stream/mod.rs` + `stream/parser.rs`：SSE 行解析（`data: {...}\n\n`）

4. **写 `core/src/webc/`**
   - `mod.rs` + `sse.rs`：bytes stream → `event_source_stream`（从 genai `webc/event_source_stream.rs` 拷贝）
   - 不做 web_client（relay 直接用 reqwest）

5. **写 `core/src/cost/`**
   - `calculator.rs`：`PriceTable { input, output, cache_write, cache_read: Decimal }` + `TokenCost::compute(usage, price, cost_profile)` — 从 ironclaw `llm/costs.rs` 提炼
   - 价格源**暂为参数**（不接 DB），签名 `TokenCost::compute(..., &PriceTable)`

6. **core/src/lib.rs** 重新导出所有公共类型

### 验证

```bash
cargo test -p summer-ai-core
# 跑 OpenAI adapter 的测试（参考 genai 原装测试，签名改一下）
# 期望：build_chat_request 构造正确的 URL/Headers/Payload；parse_chat_response 解析 OpenAI 官方样本响应
```

单测样本（保证至少这些通过）：

- `openai_build_chat_request_minimal`（只有 model + messages）
- `openai_build_chat_request_full`（tools + tool_choice + temperature + stream）
- `openai_parse_response_basic`（官方示例响应）
- `openai_parse_stream_first_chunk`（`data: {"choices":[{"delta":{"role":"assistant"}}]}`）
- `openai_parse_stream_content_delta`（内容增量）
- `openai_parse_stream_done`（`data: [DONE]`）

---

## P2: Model Entity（核心域）（0.5 天）

### 目标

搬 3 个**核心域**的 entity（channels + requests + platform），剩余域（billing/alerts/guardrails/tenancy/…）放后续 Phase 需要时再搬。

### 步骤

1. **channels 域** ← 当前已有大部分，从分支补齐
   - `channel`, `channel_account`, `channel_model_price`, `channel_model_price_version`, `channel_probe`
   - `model_config`（逻辑模型 → channel 路由配置）
   - `routing_rule`, `routing_target`
   - `ability`（channel 能力声明）
   - `vendor`（厂商字典）

2. **requests 域** ← 从分支全搬
   - `log`（每请求一条日志，成功/失败）
   - `request`（完整请求快照，含 token 消耗）
   - `request_execution`（每次上游尝试，含重试）
   - `retry_attempt`（重试详情）
   - `trace`, `trace_span`（分布式追踪）
   - `task`, `scheduler_outbox`, `dead_letter_queue`（任务队列）
   - `idempotency_record`（幂等键）
   - `error_passthrough_rule`（错误透传规则）

3. **platform 域** ← 从分支搬
   - `token`（API Token 表！NewAPI 最核心）
   - `session`（用户登录 session）
   - `rbac_policy`, `rbac_policy_version`（权限）
   - `config_entry`（动态配置）
   - `plugin`, `plugin_binding`（插件注册表）

4. **`model/src/entity/mod.rs`** 顶层模块按域组织：
   ```rust
   pub mod channels;
   pub mod requests;
   pub mod platform;
   // 后续 phase 再加：
   // pub mod billing;
   // pub mod alerts;
   // pub mod guardrails;
   // pub mod tenancy;
   ```

5. **`model/src/lib.rs`** 提供 `sync_schema(&db)` 函数（sea-orm schema-sync）

### 验证

```bash
cargo build -p summer-ai-model
# 启动一个测试 PG：
psql -d summer_dev -c "SELECT * FROM ai.channel LIMIT 0"     # 字段定义匹配 entity
psql -d summer_dev -c "SELECT * FROM ai.token LIMIT 0"
psql -d summer_dev -c "SELECT * FROM ai.request LIMIT 0"
```

**SQL 迁移**：
- 从分支 `sql/ai/*.sql` 整套拿过来（分支有完整 DDL）
- 放到 `sql/ai/` 下，按我们的命名规范重命名（如需）

---

## P3: Relay 最小链路（1 天）

### 目标

`POST /v1/chat/completions` 能跑通：handler 接收 → 硬编码 channel（ServiceTarget）→ 走 `AdapterDispatcher::build_chat_request` → `reqwest` → `parse_chat_response` → 返回。

**暂不做**：DB 读 channel、auth、billing、log——这些 P4-P7 逐步加。

### 步骤

1. **`relay/src/lib.rs`** + `plugin.rs`：SummerAiRelayPlugin（挂 router + 注册 `reqwest::Client`）

2. **`relay/src/router/mod.rs`**
   ```rust
   pub mod openai;
   pub fn routes(router: Router) -> Router {
       openai::routes(router)
   }
   ```

3. **`relay/src/router/openai/chat.rs`**
   - 从当前 `summer-ai/src/router/chat.rs` 迁过来
   - 去掉 `#[no_auth]`（P5 会加上真正的 auth layer）
   - 改用新 Adapter trait 签名
   - 硬编码 ServiceTarget（base_url + env api_key）

4. **`relay/src/service/chat/mod.rs`**
   - 抽出业务逻辑：`ChatService::invoke(target, req) -> Result<Response>`
   - Handler 只做 request parsing + response serialization，业务逻辑走 service

5. **`relay/src/service/chat/stream.rs`**
   - 流式处理（bytes_stream → parse_chat_stream_event → 重新序列化 SSE）
   - 处理 UTF-8 拼接（chunk 可能在 UTF-8 多字节中间断开）

6. 主 `summer-ai/src/lib.rs` 的 `SummerAiPlugin::build` 把 `SummerAiRelayPlugin.build(app).await;` 打开

### 验证

```bash
# 启动
cargo run -p app

# curl 非流
curl -X POST http://localhost:8080/v1/chat/completions \
  -H 'Content-Type: application/json' \
  -d '{"model":"gpt-4o-mini","messages":[{"role":"user","content":"hi"}]}'

# curl 流
curl -N -X POST http://localhost:8080/v1/chat/completions \
  -H 'Content-Type: application/json' \
  -d '{"model":"gpt-4o-mini","messages":[{"role":"user","content":"hi"}],"stream":true}'
```

**验收标准**：
- 非流式返 OpenAI 格式 JSON，含 choices/usage
- 流式返 `data: {...}\n\ndata: [DONE]\n\n` 格式
- 日志有 `summer-ai forwarding via adapter` + 选中的 channel_id（现在硬编码为 1）

---

## P3.5: 多入口协议（Claude + Gemini）（0.75 天）

### 目标

让 relay 支持**除 OpenAI 外**的原生入口协议：
- Claude SDK 的 `POST /v1/messages`
- Gemini SDK 的 `POST /v1beta/models/{model}:generateContent`（非流）和 `:streamGenerateContent`（流）

无论客户端用什么协议发，内部都走 canonical（OpenAI-flat），Adapter 永远只认 canonical。

**OpenAIResponses 入口**（`/v1/responses`）先不做——放 P8（多 Adapter）时顺带做，因为它主要服务 GPT-5 / o1 这些 reasoning 模型。

### 步骤

1. **入口 wire 类型**（`core/src/types/ingress_wire/`）
   - `claude.rs`：`ClaudeMessagesRequest` / `ClaudeResponse` / `ClaudeStreamEvent`
   - `gemini.rs`：`GeminiChatRequest` / `GeminiChatResponse` / `GeminiPart` 等
   - 照抄 NewAPI `dto/claude_dto.go` / `dto/gemini_dto.go` 的字段定义

2. **IngressConverter / EgressConverter trait**（`core/src/convert/mod.rs` 或直接放 `relay/src/convert/`）
   - 签名见 `CONVERSION_SPEC.md §6.1`
   - 定义 `IngressCtx` / `StreamConvertState`

3. **`OpenAIIngress`**（`relay/src/convert/ingress/openai.rs`）
   - `to_canonical` / `from_canonical` / `from_canonical_stream_event` 都是 **identity**（canonical 就是 OpenAI-flat）
   - 作 trait 实现的参考模板

4. **`ClaudeIngress` + `ClaudeEgress`**（按 `CONVERSION_SPEC.md §1`）
   - `ingress/claude.rs`：ClaudeMessagesRequest → ChatRequest（§1.1 系统 + §1.2 content + §1.3 tool）
   - `egress/claude.rs`：ChatResponse → ClaudeResponse（§1.4）
   - `egress/claude.rs` 的流状态机：`ClaudeStreamState` + 6 种事件重组（§1.7）**最复杂一块**
   - ReasonMap 工具：finish_reason ↔ stop_reason（§1.6）

5. **`GeminiIngress` + `GeminiEgress`**（按 `CONVERSION_SPEC.md §2`）
   - Role 映射：user↔user / model↔assistant
   - Parts 拆解：inlineData/fileData/functionCall/functionResponse（§2.3/§2.5）
   - 流响应每块一个完整 `GeminiChatResponse`（§2.8）

6. **路由注册**（`relay/src/router/`）
   - 新建 `claude/mod.rs` + `claude/messages.rs`（`POST /v1/messages`）
   - 新建 `gemini/mod.rs` + `gemini/generate_content.rs`（`POST /v1beta/models/{model}:generateContent[:streamGenerateContent]`）
   - 所有新路由的内部 pipeline：
     ```
     Ingress::to_canonical → ChannelRouter::pick → AdapterDispatcher::build_chat_request
       → upstream → parse_chat_response → Egress::from_canonical → client
     ```

7. **单元测试 round-trip**（`CONVERSION_SPEC.md §6.4`）
   - 每种 converter：`client_wire → canonical → client_wire` 语义等价
   - 至少覆盖：纯文本、tool_use+tool_result、多模态、流式

### 验证

```bash
# 启动（P4 前还是硬编码 channel，P3.5 期间 OPENAI_API_KEY 还在用）
cargo run -p app

# Claude 入口（客户端用 anthropic-sdk 格式，上游是 OpenAI）
curl -X POST http://localhost:8080/v1/messages \
  -H 'x-api-key: demo-token' \
  -H 'anthropic-version: 2023-06-01' \
  -H 'Content-Type: application/json' \
  -d '{"model":"gpt-4o-mini","max_tokens":64,"messages":[{"role":"user","content":"hi"}]}'
# 期望：返回 Claude 格式 {"id":"msg_...","type":"message","role":"assistant","content":[{"type":"text","text":"..."}],"stop_reason":"end_turn","usage":{...}}

# Claude 入口 + 流式
curl -N -X POST http://localhost:8080/v1/messages \
  -H 'x-api-key: demo-token' \
  -H 'anthropic-version: 2023-06-01' \
  -d '{"model":"gpt-4o-mini","max_tokens":64,"stream":true,"messages":[{"role":"user","content":"say hello"}]}'
# 期望：SSE 序列 message_start → content_block_start → content_block_delta* → content_block_stop → message_delta → message_stop

# Gemini 入口
curl -X POST 'http://localhost:8080/v1beta/models/gpt-4o-mini:generateContent?key=demo' \
  -H 'Content-Type: application/json' \
  -d '{"contents":[{"role":"user","parts":[{"text":"hi"}]}]}'
# 期望：返回 Gemini 格式 {"candidates":[{"content":{"role":"model","parts":[{"text":"..."}]},"finishReason":"STOP"}],"usageMetadata":{...}}
```

### 产出

- `core/src/types/ingress_wire/{claude,gemini}.rs` 共 ~600 行（wire struct 定义）
- `relay/src/convert/` 共 ~1500 行（3 个 ingress + 3 个 egress + stream state machine）
- `relay/src/router/{claude,gemini}/` 共 ~200 行（路由）
- 单测 ~800 行

---

## P4: Relay DB 化（0.75 天）

### 目标

把 P3 的硬编码 ServiceTarget 换成"从 DB 查 channel + 权重随机选 account"。

### 步骤

1. **`relay/src/service/channel_store.rs`**（新）
   - `ChannelStore`：内存快照 `{ channels: Vec<Channel>, accounts_by_channel: BTreeMap<i64, Vec<ChannelAccount>> }`
   - `load_from_db(db)`：查 `status=1` + `schedulable=true` 的行
   - `pick(logical_model: &str) -> Option<(Channel, ChannelAccount)>`：按 priority 排序，权重随机选
   - 后台任务 `spawn_refresh(db, interval)` 定期刷新

2. **`relay/src/service/channel/target.rs`**（新）
   - `build_service_target(channel: &Channel, account: &ChannelAccount, logical_model: &str) -> ServiceTarget`
   - 解析 `channel.model_mapping` 映射 logical_model → actual_model
   - 解析 `channel.extra_headers` JSONB
   - 解析 `channel_account.credentials` JSONB → AuthData
   - 解析 `channel.channel_type` → AdapterKind（`TryFrom<i16>`）

3. **Handler** 改成：
   ```rust
   let (channel, account) = store.pick(&req.model).ok_or_else(no_channel_available)?;
   let target = build_service_target(&channel, &account, &req.model);
   let kind = AdapterKind::try_from(channel.channel_type)?;
   // 后面和 P3 一样
   ```

4. **Plugin 注册**：`ChannelStore` 作 component，handler 用 `Component<Arc<ChannelStore>>` 提取

### 验证

```bash
# 先插数据
psql -c "INSERT INTO ai.channel (name, channel_type, base_url, models, status, priority, weight)
         VALUES ('openai-prod', 1, 'https://api.openai.com', '[\"gpt-4o-mini\"]'::jsonb, 1, 1, 100);"
psql -c "INSERT INTO ai.channel_account (channel_id, credentials, status, schedulable, weight)
         VALUES (1, '{\"api_key\":\"sk-xxx\"}'::jsonb, 1, true, 100);"

# 启动后 curl（和 P3 一样），但现在请求的 channel 来自 DB
curl -X POST http://localhost:8080/v1/chat/completions ...

# 禁用 channel 测试
psql -c "UPDATE ai.channel SET status=2 WHERE id=1;"
# 等 refresh_secs 秒后再 curl，期望 503 "no channel available"
```

---

## P5: Auth + Log（0.75 天）

### 目标

1. API Token 鉴权：`Authorization: Bearer sk-xxx` 校验 `ai.token` 表
2. 请求日志：每个请求写一条 `ai.log` + 一条 `ai.request`

### 步骤

**Auth**：

1. **`relay/src/auth/extractor.rs`**：`TokenContext { token_id, user_id, quota_remaining, allowed_models, ... }`
2. **`relay/src/auth/middleware.rs`**：`AiAuthLayer` Tower layer
   - 解析 `Authorization: Bearer sk-xxx`
   - 查 `ai.token` 表（带 in-memory cache，避免每请求一次 DB）
   - 验证：token 存在、未过期、未禁用、quota 够
   - 塞 `TokenContext` 到 request extensions
3. Handler 提取 `Extension(ctx): Extension<TokenContext>`

**Log**：

1. **`relay/src/service/log/mod.rs`**
   - `LogService::emit(entry)`：异步写 `ai.log`
   - fire-and-forget（用 tokio::spawn）
2. **`relay/src/service/tracking/mod.rs`**
   - `TrackingService::record_request(ctx, target, req, resp, usage, latency)`
   - 写 `ai.request`（完整快照）+ `ai.request_execution`（每次上游尝试，目前只 1 次）

3. Handler 集成：响应返回前收集 metrics，返回后 spawn 一个 log 任务

### 验证

```bash
# 插 token
psql -c "INSERT INTO ai.token (name, key, user_id, quota_remaining, status)
         VALUES ('test', 'sk-my-token-abc', 1, 1000000, 1);"

# 无 token 请求：期望 401
curl -X POST http://localhost:8080/v1/chat/completions \
  -H 'Content-Type: application/json' -d '...'
# → {"error": "unauthorized"}

# 有 token：期望成功
curl -X POST http://localhost:8080/v1/chat/completions \
  -H 'Authorization: Bearer sk-my-token-abc' \
  -H 'Content-Type: application/json' -d '{"model":"gpt-4o-mini", ...}'
# → 成功响应

# 检查 log
psql -c "SELECT id, token_id, status, elapsed_ms FROM ai.log ORDER BY id DESC LIMIT 3;"
psql -c "SELECT id, logical_model, actual_model, prompt_tokens, completion_tokens FROM ai.request ORDER BY id DESC LIMIT 3;"
```

---

## P6: Billing（1 天）

### 目标

上 `billing` 子 crate 的三阶段原子扣费：

- **Reserve**（请求前）：按预估 token 数预扣 quota
- **Settle**（响应后）：按实际 token 数结算（补扣或退回）
- **Refund**（失败时）：完全退回

### 步骤

1. **搬 `billing/` entity**（P2 暂未搬的）：
   - `user_quota`, `user_subscription`, `subscription_plan`, `subscription_preconsume_record`
   - `topup`, `transaction`, `order`, `payment_method`
   - `discount`, `redemption`, `referral`
   - `group_ratio`, `usage_billing_dedup`, `usage_cleanup_task`

2. **`billing/src/service/engine/`**（核心）
   - `reserve(token_ctx, estimated_tokens, price) -> Result<ReservationId>`
   - `settle(reservation_id, actual_tokens) -> Result<Settlement>`
   - `refund(reservation_id) -> Result<()>`
   - 事务内悲观锁 `ai.user_quota` 表（v1 用 PG，v2 可选 Redis）

3. **`billing/src/service/price/`**
   - `PriceResolver::resolve(channel_id, logical_model) -> PriceTable`
   - 读 `ai.channel_model_price`（按 channel + model 维度）
   - 应用 `ai.group_ratio`（按用户组加价/减价）

4. **Relay 集成**
   - P5 的 AuthLayer 之后再加 BillingLayer：reserve → handler → settle/refund
   - settle 要拿到 upstream 的实际 usage（从 ChatResponse.usage）

### 验证

```bash
# 插 price
psql -c "INSERT INTO ai.channel_model_price (channel_id, model_name, input_price_per_million, output_price_per_million)
         VALUES (1, 'gpt-4o-mini', 0.15, 0.60);"

# 插 user_quota
psql -c "INSERT INTO ai.user_quota (user_id, quota_remaining) VALUES (1, 100);"  # 余额 100 美分

# 跑一次请求，响应后检查
psql -c "SELECT user_id, quota_remaining FROM ai.user_quota WHERE user_id=1;"
# 期望：扣除了 usage.total_tokens × 价格

# 余额不足测试
psql -c "UPDATE ai.user_quota SET quota_remaining=0 WHERE user_id=1;"
curl ... # 期望 402 "insufficient quota"
```

---

## P7: Admin CRUD（1 天）

### 目标

后台管理 API：channel / channel_account / token / price / request 查询。

### 步骤

**搬分支的 `admin/` 基本原样**，改动：
- `admin/src/router/channel/` → `GET/POST/PATCH/DELETE /admin/ai/channels`
- `admin/src/router/channel_account/` → 同样模式
- `admin/src/router/channel_model_price/`
- `admin/src/router/request/` → 只读查询（ai.log + ai.request join）
- `admin/src/router/token/`（新，分支也有）
- `admin/src/router/daily_stats/` → dashboard 聚合

service 层复用分支的 service/ 内容。

### 验证

```bash
# 用管理员账号
curl -X POST http://localhost:8080/admin/ai/channels \
  -H 'Authorization: Bearer admin-token' \
  -H 'Content-Type: application/json' \
  -d '{"name":"openrouter","channel_type":99,"base_url":"https://openrouter.ai/api/v1",
       "models":["gpt-4o","claude-sonnet-4"],"priority":2}'

curl http://localhost:8080/admin/ai/channels

curl http://localhost:8080/admin/ai/requests?limit=20
```

---

## P8: 多 Adapter（0.75 天）

### 目标

基于 P1-P7 的完整链路，**扩展 Core 的 adapter 覆盖**：加 Anthropic / Gemini / Ollama / Cohere / OpenAICompat 共 5 家。

### 步骤

每家一个文件 `core/src/adapter/adapters/{anthropic,gemini,ollama,cohere,openai_compat}.rs`：

1. 从 genai 对应 adapter 拷贝 wire-format 转换逻辑
2. 对接我们的 Adapter trait（3 方法）
3. `dispatcher.rs` 对应分支从 `unimplemented!` 换成真实调用
4. 加单测

**增量测试**（每加一个就 curl 一次）：

```bash
# Anthropic
psql -c "INSERT INTO ai.channel (name, channel_type, base_url, models, status, priority, weight)
         VALUES ('anthropic', 3, 'https://api.anthropic.com', '[\"claude-sonnet-4\"]'::jsonb, 1, 1, 100);"
psql -c "INSERT INTO ai.channel_account (channel_id, credentials, status, schedulable, weight)
         VALUES (2, '{\"api_key\":\"sk-ant-xxx\"}'::jsonb, 1, true, 100);"

curl ... -d '{"model":"claude-sonnet-4","messages":[...]}'  # 期望经 Anthropic 适配返回

# Gemini
...

# Ollama（本地）
psql -c "INSERT INTO ai.channel (..., channel_type, base_url, models) VALUES (..., 28, 'http://localhost:11434', '[\"llama3\"]'::jsonb);"
curl ... -d '{"model":"llama3", ...}'
```

---

## P9: 韧性层（1.25 天）

### 目标

在 relay 链路上挂 retry / failover / circuit_breaker 三层 middleware。

### 步骤

1. **`relay/src/service/retry/`** ← ironclaw `llm/retry.rs`
   - 指数退避 + 抖动
   - 只重试 5xx / 429 / 网络错
   - 每次重试写 `ai.retry_attempt` 表

2. **`relay/src/service/circuit_breaker/`** ← ironclaw `llm/circuit_breaker.rs`
   - 按 `channel_account_id` 分组的熔断器
   - Open 状态时跳过该 account
   - 改状态同步回 `ai.channel_account.rate_limited_until`

3. **`relay/src/service/failover/`** ← ironclaw `llm/failover.rs`
   - 上游失败时按优先级降级到下一个 channel/account
   - 候选列表来源改为 `ChannelStore::pick_candidates(model) -> Vec<(Channel, Account)>`

### 验证

- 故意插一个坏 key 的 account + 一个好 key 的 account，观察自动切换
- 连续 N 次失败后观察熔断开启

---

## P10: 其余 14 个 Adapter（0.75 天）

### 目标

上齐 19 家：OpenAIResp, Azure, Groq, DeepSeek, Xai, Fireworks, Together, Nebius, Mimo, Zai, BigModel, Aliyun, OllamaCloud, Vertex, GithubCopilot。

机械式拷贝，基本类似 OpenAICompat 或某个已实现的近亲。

---

## P11: TaskAdapter 异步任务（1.5 天）

### 目标

支持**长时任务型** provider：Midjourney（图像）、Suno（音乐）、Runway/Luma/Kling（视频）。这类任务特点：

- 提交 → 返回 task_id（立即）
- 客户端轮询 / webhook 等待（数秒到数分钟）
- **三阶段计费**：提交时估扣 → 上游返回实际参数调整 → 任务终态结算补扣/退款

和同步 `Adapter` 完全不同的生命周期，所以拆独立 trait `TaskAdapter`（ARCHITECTURE §4.6）。

### 步骤

1. **`core/src/adapter/task.rs`**：`TaskAdapter` trait 定义 + `TaskKind` enum
   ```rust
   pub trait TaskAdapter {
       const KIND: AdapterKind;
       const TASK_KIND: TaskKind;  // Mj / Suno / Video / ...

       fn build_task_request(target: &ServiceTarget, req: &TaskRequest) -> AdapterResult<WebRequestData>;
       fn parse_submit_response(target: &ServiceTarget, body: Bytes) -> AdapterResult<TaskSubmission>;
       async fn poll_task(target: &ServiceTarget, http: &reqwest::Client, task_id: &str) -> AdapterResult<TaskStatus>;

       fn estimate_billing(req: &TaskRequest) -> BillingRatios;
       fn adjust_billing_on_submit(upstream_resp: &[u8]) -> BillingRatios;
       fn adjust_billing_on_complete(task: &Task, result: &TaskInfo) -> Decimal;
   }
   ```

2. **TaskDispatcher**（类似 `AdapterDispatcher` 但分派 `TaskAdapter`）
   - 按 `AdapterKind` match 分派到具体 TaskAdapter
   - 也是 ZST + 静态分派

3. **业务表**（P2 已搬入 entity，这里只加后端逻辑）
   - `ai.task`：任务表（status, progress, result_url, ...）
   - `ai.scheduler_outbox`：待轮询任务队列
   - `ai.dead_letter_queue`：失败的最终归宿

4. **具体 TaskAdapter 实现**（按优先级逐个加）
   - `MidjourneyAdapter`（图像生成，作 PoC）
   - 可选：`SunoAdapter`、`RunwayAdapter`、`KlingAdapter`、`LumaAdapter`

5. **轮询引擎**（`relay/src/service/task_scheduler/`）
   - 后台 tick（每 5s）从 `ai.scheduler_outbox` 取 pending 任务
   - 按 channel 分片、并发控制（避免单渠道打爆）
   - 调 `TaskDispatcher::poll_task`
   - 终态时触发 `adjust_billing_on_complete` + `billing::settle`

6. **billing 引擎扩展**（`billing/src/service/task_billing/`）
   - `reserve_task_quota(user, ratios, price)` → 提交前扣
   - `settle_task(task_id, actual_ratios)` → 终态结算
   - 支持"延迟结算"的事务一致性（task 成功才真扣，失败退回）

7. **路由**（`relay/src/router/task/`）
   - `POST /mj/submit/imagine` / `POST /mj/submit/upscale` / 等 Midjourney 端点
   - `GET /mj/task/{id}/fetch` 查询进度
   - `GET /mj/task/{id}/image-seed` 查种子
   - 或统一 OpenAI 风格：`POST /v1/images/async` + `GET /v1/tasks/{id}`

### 验证

```bash
# 插 MJ channel
psql -c "INSERT INTO ai.channel (name, channel_type, base_url, status)
         VALUES ('mj-relax', 99, 'https://api.midjourney.example', 1);"

# 提交任务
curl -X POST http://localhost:8080/mj/submit/imagine \
  -H 'Authorization: Bearer sk-my-token' \
  -d '{"prompt":"a cat in space --v 6"}'
# 期望：{"code":1,"result":"task_abc123","properties":{...}}

# 轮询
curl http://localhost:8080/mj/task/task_abc123/fetch -H 'Authorization: Bearer sk-my-token'
# 期望：{"status":"IN_PROGRESS","progress":"30%"} 或 {"status":"SUCCESS","imageUrl":"..."}

# 余额变化（预扣 → 实际扣）
psql -c "SELECT user_id, quota_remaining FROM ai.user_quota WHERE user_id=1;"

# 异常：任务失败，全额退回
psql -c "SELECT status FROM ai.task WHERE id=...;"
# FAILED 时 user_quota 应被退回到提交前
```

### 产出

- `core/src/adapter/task.rs` ~200 行（trait + dispatcher）
- `core/src/adapter/task_adapters/midjourney.rs` ~600 行
- `relay/src/service/task_scheduler/` ~500 行（轮询引擎）
- `billing/src/service/task_billing/` ~400 行（三阶段事务）
- `relay/src/router/task/mj.rs` ~300 行

---

## 附录 A：业务表 ↔ 代码类型 映射

| 表字段 | 代码类型 | 来源 Phase |
|---|---|---|
| `ai.channel.channel_type: i16` | `AdapterKind` via `TryFrom<i16>` | P1 |
| `ai.channel.base_url` | `Endpoint::from_owned(...)` | P4 |
| `ai.channel.extra_headers: JSONB` | `ServiceTarget.extra_headers: BTreeMap` | P4 |
| `ai.channel.models: JSONB` (array) | `ChannelStore` 倒排索引 | P4 |
| `ai.channel.model_mapping: JSONB` (obj) | `ServiceTarget.actual_model` | P4 |
| `ai.channel.capabilities: JSONB` | `ServiceTarget.capabilities_override` | P4 |
| `ai.channel.priority` / `weight` | `ChannelStore::pick()` 排序 + 随机 | P4 |
| `ai.channel.status` | 过滤 Enabled 才加载 | P4 |
| `ai.channel_account.credentials` | `AuthData::Single(...)` | P4 |
| `ai.channel_account.schedulable` | 过滤 true 才加载 | P4 |
| `ai.channel_account.rate_limited_until` | 过滤 `> now()` 才加载 | P4 |
| `ai.channel_model_price.*` | `PriceTable` via `PriceResolver` | P6 |
| `ai.token.*` | `TokenContext` via `AiAuthLayer` | P5 |
| `ai.user_quota.*` | `billing::service::engine::reserve/settle` | P6 |
| `ai.group_ratio.*` | `PriceResolver::apply_group_ratio` | P6 |
| `ai.log` | `LogService::emit` | P5 |
| `ai.request` + `request_execution` | `TrackingService::record_*` | P5 |
| `ai.retry_attempt` | retry middleware | P9 |
| `ai.routing_rule` + `routing_target` | `ChannelRouter` 规则引擎 | （未安排） |
| `ai.channel_probe.*` | 后台健康探测任务 | （未安排，建议 P11） |

---

## 附录 B：每 Phase 的 Cargo 命令 Cheatsheet

```bash
# P0
cargo build -p summer-ai && cargo tree -p summer-ai --depth 2

# P1
cargo build -p summer-ai-core
cargo test -p summer-ai-core --lib

# P2
cargo build -p summer-ai-model
# 手动跑 sea-orm schema sync（写个 bin 或在 P3 启动时）

# P3, P4, P5, P6, P7
cargo run -p app
curl ...   # 如本文各 Phase 的 curl

# P8
cargo test -p summer-ai-core  # 新加的 adapter 各自测试
curl ...

# P9
cargo build -p summer-ai-relay
# 集成测试：故意构造失败场景

# P10
cargo test -p summer-ai-core
```

---

## 附录 C：风险与规避

| 风险 | 规避 |
|---|---|
| sea-orm `with-bigdecimal` feature 跨 crate 冲突（之前踩过） | P2 model 加 feature 时先在单 crate 测 `cargo build -p app` |
| genai adapter 代码依赖 `ChatOptionsSet` 视图，搬过来要改几十处 | 写一个 helper `fn opts<'a>(req: &'a ChatRequest) -> &'a ChatRequest { req }` + 用扩展方法补缺字段，减少改动 |
| Billing 扣费事务死锁 | 按 `user_id` 升序加锁；用 PG `SELECT FOR UPDATE SKIP LOCKED` |
| Stream 中途失败的计费口径不一致 | 以 ChatStreamEvent::End 累积的 Usage 为准，失败前已传的 token 照扣 |
| 19 adapter 一次性搬容易爆 | P8 先搬 5 家验证链路，P10 再机械式铺开 14 家 |

---

## 变更日志

| 日期 | 修改 | 原因 |
|---|---|---|
| 2026-04-19 | 初版（v2），替换 MIGRATION.md v1 | 学习了 feature/extract-summer-ai 分支结构后全量重写 |
