# summer-ai 开发路线图

> 模型版本：Claude Opus 4.7 (1M context)
> 修订：2026-04-19
> 前置阅读：[`DESIGN.md`](./DESIGN.md)

---

## 0. 面向不熟悉 relay 的开发者

如果你之前没做过 LLM 中转 / API 网关类的项目，这个文档比 DESIGN.md 更重要。

### 0.1 开发哲学：walking skeleton 优于横向铺开

**反例**（横向铺开）——很多人以为的正确顺序：

```text
1. 把所有 providers 写完（OpenAI + Anthropic + Gemini + Azure）
2. 把 ChannelStore 写完
3. 把 Router 写完
4. 把 Client 写完
5. 把 Handler 写完
6. 最后拼起来跑第一次请求
```

**问题**：前 5 步你都**看不到任何效果**，bug 堆到第 6 步爆发，调试地狱。而且
写 Provider 时你根本不知道"到底需要它返回什么形状的东西"，都是猜的。

**正确做法**（走路骨架 + 纵向切片）：

```text
Day 1：先让"最简单的一条请求"从头到尾跑通，哪怕：
  - 只有一个硬编码的 OpenAI 上游
  - api_key 写在代码里
  - handler 直接调 reqwest 转发，不抽象
  - 没有流式、没有 DB、没有 channel

Day 2 以后：保持"能跑"的前提下，把里面每一段硬编码替换成真实组件。
  curl 每次都能验证——这就是安全网。
```

一次 `curl` 验证通过 = 一个清晰的 commit，可以 push 上去睡觉。

---

## 0.2 你现在"从 providers 开始"的分析

如果你已经开始写 `LlmProvider` trait 了，不用撤销——但**暂停完善它**。

`providers` 本身是好的抽象（DESIGN.md §3.1），但**它依赖上下文才能验证**：

```
provider 需要 → ChatRequest/Response 类型
            → HTTP client
            → auth header 怎么塞
            → stream 怎么解析
            → 错误怎么包
```

你一个个想清楚、写完再联调，很容易越写越乱。**换条路**：先用**硬编码**走通一次
请求，再把硬编码换成 provider trait。这样 provider 的形状是**被端到端需求拉出
来的**，不是你凭空设计的。

---

## 1. 先看懂一次请求

```text
┌─────────────┐   POST /v1/chat/completions              ┌──────────────┐
│   前端 App   │   Body: { model, messages }   ────►     │  summer-ai    │
│ (OpenAI SDK)│   Header: Authorization: Bearer sk-xxx   │    (你的中转)  │
└─────────────┘                                           └──────┬───────┘
                                                                 │
                       拿到请求后要做 8 件事：                     │
                                                                 │
 ┌───────────────────────────────────────────────────────────────┴───┐
 │ 1. 解包 HTTP body 成 ChatRequest                                   │
 │ 2. 鉴权：用我方 Bearer token 查 user / tenant                       │
 │ 3. 选 channel + account（按 model + 租户 + 权重）                   │
 │ 4. 解析 channel_account.credentials → 上游真实 API Key              │
 │ 5. 按 channel.channel_type 选 Adapter（OpenAI / Anthropic / ...）  │
 │ 6. Adapter 把 ChatRequest 转成上游协议格式（HTTP body + headers）   │
 │ 7. 发出去；拿到响应 / SSE 流                                        │
 │ 8. Adapter 解析响应 → ChatResponse，返回前端                        │
 └────────────────────────────────┬───────────────────────────────────┘
                                  │
                                  ▼
                  ┌─────────────────────────────────┐
                  │   OpenAI / Anthropic / ... API  │
                  └─────────────────────────────────┘
```

**8 件事里**：
- 任何一件都可以先硬编码（"查 user" 返回固定 tenant；"选 channel" 返回固定
  base_url + key）
- 只要流水线 8 步都在，就能打通一次请求
- 然后逐步**替换每一个硬编码为真实实现**

---

## 2. 分阶段开发顺序（10 个阶段）

每个阶段有：**目标** / **要动的文件** / **验证方式**。按顺序做，不跳步。

---

### 阶段 1：走路骨架（MVP 0.1，目标：一行硬编码全部打通，耗时 2-4 小时）

**目标**：前端发 `POST /v1/chat/completions` → summer-ai 转发到 OpenAI → 响应
原样返回前端。**没有 adapter、没有 channel、没有 DB。**

**要动的文件**：
- `crates/summer-ai/Cargo.toml` — 加 `axum`、`reqwest`、`serde_json`
- `crates/summer-ai/src/lib.rs` — `SummerAiPlugin`，挂 axum route
- `crates/summer-ai/src/handler.rs` — handler 直接 reqwest 转发

**伪代码**（就这么多）：

```rust
async fn chat_completions(Json(body): Json<serde_json::Value>) -> Response {
    let api_key = std::env::var("OPENAI_API_KEY").unwrap();
    let resp = reqwest::Client::new()
        .post("https://api.openai.com/v1/chat/completions")
        .bearer_auth(&api_key)
        .json(&body)
        .send().await.unwrap()
        .text().await.unwrap();
    (StatusCode::OK, resp).into_response()
}
```

**验证**：
```bash
export OPENAI_API_KEY=sk-xxx
cargo run -p app
# 另起终端
curl -X POST http://localhost:8080/v1/chat/completions \
  -H 'Content-Type: application/json' \
  -d '{"model":"gpt-4o-mini","messages":[{"role":"user","content":"hi"}]}'
# 应看到 OpenAI 返回的 JSON
```

**这一步打通了什么**：
- Cargo workspace 配置对
- Plugin 能注册
- axum route 能路由
- 网络能出站（没被防火墙 / DNS 挡）

这是**后续所有工作的地基**。如果这一步不通，先不要写别的。

---

### 阶段 2：引入 canonical types（目标：body 从 Value 变成强类型，耗时 2 小时）

**目标**：不再用 `serde_json::Value`，定义 `ChatRequest / ChatMessage /
ChatResponse`，handler 能 `Json<ChatRequest>` 解包。

**要动的文件**：
- 新增 `crates/summer-ai/core/src/types/` 目录
  - `message.rs` — `ChatMessage`, `MessageContent`, `Role`
  - `request.rs` — `ChatRequest`
  - `response.rs` — `ChatResponse`, `Usage`
- `handler.rs` — 参数类型从 `Json<Value>` 改 `Json<ChatRequest>`

**关键决策**：字段加 `#[serde(flatten)] extra: Map` 透传 unknown 字段，这样
上游私有字段（reasoning / 供应商定制）不丢。

**验证**：`curl` 同阶段 1，**必须仍然工作**。

---

### 阶段 3：抽出 Adapter trait（目标：把"转发 OpenAI"逻辑封进一个 ZST，耗时 3 小时）

**目标**：handler 不直接 reqwest，调一个 `Adapter::to_web_request_data(target,
req)` 函数来构造请求。只做 OpenAI-compat，**硬编码的 base_url 和 api_key 还
在**——但换成通过参数传。

**要动的文件**：
- `core/src/adapter/mod.rs` — `Adapter` trait（参考 DESIGN §3.1）
- `core/src/adapter/adapters/openai/mod.rs` — `OpenAIAdapter` ZST
- `core/src/resolver/target.rs` — `ServiceTarget` struct（就是个 POJO）
- `handler.rs` — 构造 ServiceTarget（硬编码 endpoint + key）→ 调
  `OpenAIAdapter::to_web_request_data`

**验证**：curl 同阶段 1，响应结构**一样**。区别只是代码组织变了。

**这步的收获**：Adapter 的签名（接什么参数、返什么）是**从端到端实际需求**拉
出来的。你现在不用猜"Provider 应该怎么设计"。

---

### 阶段 4：AdapterKind + Dispatcher（目标：留好多协议的位置，耗时 2 小时）

**目标**：加 `AdapterKind::OpenAICompat`，加 `AdapterDispatcher`。虽然目前
只有一个 adapter，但流水线已完备——以后加 Anthropic 只加一行 match。

**要动的文件**：
- `core/src/adapter/kind.rs` — `enum AdapterKind`（先只有一个变体也行）
- `core/src/adapter/dispatcher.rs` — `AdapterDispatcher::chat(kind, ...)`
- `handler.rs` — 硬编码 `kind = AdapterKind::OpenAICompat`，走 dispatcher

**验证**：curl 同。

---

### 阶段 5：流式（目标：SSE 能流回前端，耗时 3-4 小时）

**目标**：`stream: true` 的请求从上游拉 SSE，边拉边转给前端。

**要动的文件**：
- `core/src/types/stream_event.rs` — `ChatStreamEvent { Start, TextDelta, End }`
- `core/src/stream.rs` — `SseParser`
- `core/src/adapter/adapters/openai/stream.rs` — SSE → `ChatStreamEvent` 转换
- `handler.rs` — 按 `req.stream` 分流；流式返回 `Sse<...>`

**验证**：
```bash
curl -N -X POST http://localhost:8080/v1/chat/completions \
  -H 'Content-Type: application/json' \
  -d '{"model":"gpt-4o-mini","messages":[{"role":"user","content":"写一个五言绝句"}],"stream":true}'
# 应看到 data: {...}\n\n 一行一行流式刷出
```

**到这里为止 MVP 完成**：可以跑 OpenAI 转发；剩下阶段都是"把硬编码换掉"+"加
新功能"。

---

### 阶段 6：接 DB 读 Channel（目标：替换硬编码 endpoint / api_key，耗时 1 天）

**目标**：`ai.channel` + `ai.channel_account` 里插一条，relay 从 DB 读。handler
不再硬编码上游。

**要动的文件**：
- `crates/summer-ai/model/` 已有 entity（之前移植过的，不用动）
- `crates/summer-ai/src/channel_store.rs` — `ChannelStore::load_from_db`
- `crates/summer-ai/src/router.rs` — 超简 router：`pick(model)` 找第一个
  `channels` 包含 `model` 的 enabled channel
- `crates/summer-ai/src/credential.rs` — `resolve_credentials(account, kind)`
  返回 `AuthData`
- `handler.rs` — 调 Router → 查 Account → 解析凭证 → 构造 ServiceTarget

**先不做**：权重、failover、健康检查（都是阶段 8 的事）。

**验证**：
```sql
INSERT INTO ai.channel (name, channel_type, base_url, models, status)
  VALUES ('openai-test', 1, 'https://api.openai.com', '["gpt-4o-mini"]'::jsonb, 1);
INSERT INTO ai.channel_account (channel_id, name, credentials, status, schedulable)
  VALUES (1, 'primary', '{"api_key":"sk-xxx"}'::jsonb, 1, true);
```
然后 `curl` 不再需要设置 `OPENAI_API_KEY` 环境变量。
再 `UPDATE ai.channel SET status = 2`，**等 30 秒**（tick 刷新），再 curl
应该返回 503 no_channel。

---

### 阶段 7：SummerAiPlugin 集成（目标：融入 summer 框架，耗时 2 小时）

**目标**：通过 `[summer-ai] enabled=true` 配置开关、依赖 `SeaOrmPlugin` 拿
DatabaseConnection、`app.add_router_layer` 挂路由、后台 tick 刷新
ChannelStore。

**要动的文件**：
- `crates/summer-ai/src/config.rs` — `SummerAiConfig`
- `crates/summer-ai/src/lib.rs` — `SummerAiPlugin::build` 逻辑
- `config/app-dev.toml` — 加 `[summer-ai]` section
- `crates/app/src/main.rs` — 注册 `SummerAiPlugin`

**验证**：`cargo run -p app`，启动日志应看到 `summer-ai plugin initialized`。

**到这里，第一个生产可用版本完成**（只支持 OpenAI 兼容，但所有配置都在 DB，
运营可以改）。

---

### 阶段 8：路由增强（目标：多 channel 场景，耗时 1 天）

**目标**：按 model 过滤、权重加权随机、健康度剔除、租户过滤。

**要动的文件**：
- `router.rs` — 完善 `ChannelRouter::pick`
- 新增 `health.rs` — 记录每个 channel 近 N 次调用的成功率、延迟
- `handler.rs` — 调用后回写 health

**验证**：DB 插两个 channel，一个权重 1 一个权重 9，curl 100 次，统计上游日
志里命中的比例应该接近 1:9。

---

### 阶段 9：多协议（目标：Anthropic / Gemini / Azure，每家 1-2 天）

**目标**：支持 Anthropic 原生 `/v1/messages`、Gemini `generateContent`、
Azure OpenAI。这些是**完全新的上游协议**，要新写 3 个 Adapter。

**每家 adapter 的工作**：
- 新增 `core/src/adapter/adapters/<name>/{mod, wire, stream}.rs`
- 实现 `Adapter` trait（4 个核心方法：to_web_request_data /
  to_chat_response / to_chat_stream / map_error）
- `AdapterKind` 加一个变体
- `AdapterDispatcher` 的 match 加一行
- 写 5-8 个单元测试（非流 / 流 / 错误 / tool call）

**关键难点**（注意陷阱）：
- **Anthropic**：system 是顶级字段不是消息，tool_use/tool_result 是 content block
- **Gemini**：url 要 `...generateContent`，流式是 `streamGenerateContent`，content 叫 `parts`
- **Azure**：url 要带 `api-version` query 参数，鉴权 header 是 `api-key` 不是 `Authorization`

**验证**：每个 adapter 写完后 DB 插一条对应 channel，curl 要能 work。

---

### 阶段 10：增值中间件（目标：relay 的差异化价值，每个 1-2 天）

按优先级做（不一定全做）：

#### 10.1 审计（必做，2 天）

每次请求写 `ai.request` + `ai.request_execution`：
- 请求入参（脱敏 key）
- 耗时 / 状态码 / 上游返回长度
- channel / account id

配 `SELECT * FROM ai.request ORDER BY create_time DESC LIMIT 100` 观察效果。

#### 10.2 计费（DESIGN.md §3.8，2 天）

每次请求后扣 `ai.user_quota`：
- 从 response.usage 拿 token 数
- 乘 `channel_model_price` 单价
- 乘 `Adapter::cost_profile()` 系数（cache 折扣）
- 更新 `user_quota.quota_used`

#### 10.3 限流（1 天）

按 `governance_rate_limit` 表配置 RPM / TPM / 并发数，超限返 429。

#### 10.4 能力降级（DESIGN.md §3.9，1 天）

`capability_fallback_middleware`：上游不支持 tools 时自动把 tools 转 system
prompt。

#### 10.5 Failover（1-2 天）

上游 5xx / 超时 → 自动重选 channel 重试（有上限，避免雪崩）。

#### 10.6 管理后台 CRUD（3-5 天）

REST / GraphQL API 管理 channel / channel_account / user_quota 等。前端页面
后续做。

---

## 3. 最小验证 checklist（每个阶段必须通过）

每完成一个阶段，确认：

- [ ] `cargo build -p summer-ai` 编译通过
- [ ] `cargo test -p summer-ai` 测试通过
- [ ] `cargo clippy -p summer-ai --no-deps -- -D warnings` 无新警告
- [ ] 本阶段**要验证的 curl** 能得到预期输出
- [ ] **上一阶段的 curl 仍然能得到预期输出**（回归）
- [ ] Commit 一次（每个阶段独立 commit，方便回滚）

---

## 4. 踩坑警告

| 陷阱 | 症状 | 对策 |
|---|---|---|
| **先写完 providers 再联调** | 写了几天一次请求都跑不通，bug 堆积 | 改用阶段 1 的走路骨架 |
| **一上来就引入 Resolver / 热更新 / 事件驱动** | 过度设计，核心流程不稳 | 先硬编码，阶段 6 才接 DB |
| **把 API Key 写在 tracing 日志里** | 日志泄漏，安全事故 | tracing 打 API key 字段前用 `secret = "<REDACTED>"` |
| **所有 provider 都做成 trait object** | `Arc<dyn Provider>` 的热更新 / 并发陷阱（见 DESIGN §7.1） | 坚持 ZST + Dispatcher |
| **流式没做好反向压力** | 上游慢，我方内存飙 | 用 `reqwest.bytes_stream()` 直接转给 axum Sse，不要中间缓冲 |
| **`channel.credentials` JSONB 直接 `AuthData::Bearer`** | Anthropic / Azure 的 header 不同，硬转就错了 | 用 DESIGN §3.6 的 `resolve_credentials(account, kind)` 分派 |
| **计费在热路径做** | 每请求一次读 `channel_model_price` 表拖慢 QPS | 缓存到 ChannelStore 快照里；异步写 usage |

---

## 5. 时间预算（独立 dev）

| 阶段 | 预估 | 累计 |
|---|---|---|
| 1. 走路骨架 | 2-4 h | 4 h |
| 2. canonical types | 2 h | 6 h |
| 3. Adapter trait | 3 h | 9 h |
| 4. Dispatcher | 2 h | 11 h |
| 5. 流式 | 3-4 h | 15 h |
| **MVP 完成** | **≈ 2 天** | 15 h |
| 6. 接 DB | 1 d | 23 h |
| 7. Plugin 集成 | 2 h | 25 h |
| 8. 路由增强 | 1 d | 33 h |
| **生产可用版本** | **≈ 5 天** | 33 h |
| 9. 多协议（3 家） | 3-5 d | 60-75 h |
| 10.1 审计 | 2 d | 75-90 h |
| 10.2 计费 | 2 d | 90-105 h |
| 10.3 限流 | 1 d | 100-115 h |
| 10.4 能力降级 | 1 d | 110-125 h |
| 10.5 Failover | 1-2 d | 120-135 h |
| 10.6 管理后台 | 3-5 d | 140-170 h |

**全功能 relay 约 4-5 周**（每天 8 小时）。MVP 2 天，基础生产版本 1 周。

---

## 6. 何时需要回头看 DESIGN.md

- **阶段 3 前**：读 DESIGN §3.1（Adapter trait 设计）
- **阶段 4 前**：读 DESIGN §3.2 + §3.3（Dispatcher + ServiceTarget）
- **阶段 5 前**：读 DESIGN §3.9 尾部（SSE → StreamEvent 的设计动机）
- **阶段 6 前**：读 DESIGN §3.4 + §3.5 + §3.6 + §3.7（整个 resolver 栈）
- **阶段 8 前**：读 DESIGN §3.5 路由部分
- **阶段 9 前**：读 DESIGN 附录 A（协议映射）+ §7（为什么不照搬 ironclaw/zeroclaw）
- **阶段 10.2 前**：读 DESIGN §3.8（CostProfile）
- **阶段 10.4 前**：读 DESIGN §3.9（能力降级 middleware）

---

## 7. 问题排查的思考路径

遇到 bug 时，按**阶段定位**：

```
curl 不通？           → 回到阶段 1-2 检查 HTTP 链路
body 解析不对？        → 阶段 2 的 ChatRequest serde
上游报错？            → 阶段 3 的 Adapter to_web_request_data 改写对不对
响应格式不对？         → 阶段 3 的 to_chat_response
流式卡住？            → 阶段 5 的 SseParser / SSE 回写 axum
选错 channel？         → 阶段 6-8 的 Router
鉴权 401？            → 阶段 6 的 CredentialResolver
扣费不准？            → 阶段 10.2 的 CostProfile + unit price
```

每一类问题都对应一个阶段的产物。这就是为什么要**一阶段一 commit**——bug 出现
时 `git bisect` 能快速定位。

---

## 结语

**三件事做对了就不会走弯路**：

1. **先打通再优化**（走路骨架 > 横向铺开）
2. **每阶段用 curl 验证**（有回归安全网）
3. **阶段顺序不要跳**（前面的硬编码是后面设计的锚点）

按这 10 阶段走下来，你会发现**DESIGN.md 里那些抽象**不再是空中楼阁，
而是**一个个"解决我实际遇到的问题"的工具**。
