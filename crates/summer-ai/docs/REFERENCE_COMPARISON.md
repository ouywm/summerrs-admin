# summer-ai vs NewAPI / OneAPI / AxonHub —— 底层能力对比

> 模型版本：Claude Opus 4.7 (1M context)
> 更新日期：2026-04-21
> 前置阅读：[`ARCHITECTURE.md`](./ARCHITECTURE.md) · [`ROADMAP.md`](./ROADMAP.md) · [`MIGRATION_V2.md`](./MIGRATION_V2.md)

## 目的

- 以"底层能力"为单位，列出 summer-ai 现状 vs 三个主流参考实现（NewAPI / OneAPI / AxonHub）的差距。
- 给出**可以直接映射到 MIGRATION_V2 的 Phase**的优先级建议，避免再踩"先铺广度还是先收业务"的纠结。
- 作为后续 ADR / 迭代评审的参考锚点。

**阅读范围**：`crates/summer-ai/` 全部源码（core/relay/model/admin/billing 五个 sub-crate）+ 自家 `docs/` 六篇契约文档 + `docs/relay/go/{axonhub,new-api,one-api}/` 三个参考实现。

---

## 一、summer-ai 当前完成度（对照 MIGRATION_V2 的 11 个 Phase）

| Phase | 状态 | 实测证据 |
|---|---|---|
| **P0** 结构骨架 | ✅ 完成 | 5 个 sub-crate 全部编译通过，顶层 lib.rs 做聚合 |
| **P1** Core OpenAI | ✅ 完成 | `core/adapter/adapters/openai.rs` 624 行 |
| **P2** Model entity | ✅ 超额（整个 NewAPI 域都搬了） | `model/src/entity/` 已含 channels/requests/billing/alerts/governance/guardrails/tenancy/file_storage/conversations/platform/misc 十一域，~80 张表 |
| **P3** 最小链路 | ✅ 完成 | `router/openai/chat.rs` 72 行走通 |
| **P3.5** 多入口协议 | ✅ 完成（含 Responses ingress） | `convert/ingress/claude.rs` 1558 行 + `gemini.rs` 1009 行 + `openai_responses.rs` 1490 行 |
| **P4** Relay DB 化 | ✅ 完成（还做了 Redis 缓存） | `service/channel_store.rs` 774 行，四级 Redis key + DB fallback + 权重随机 |
| **P5** Auth + Log | 🟡 Auth 完成（154 行 AiAuthLayer），**Log 仅骨架**（`tracking/mod.rs` 只有 `pub mod context/service`，没看到实际落库代码） |
| **P6** Billing | ❌ **空壳 plugin**（`billing/src/lib.rs` 11 行，只有 `pub mod`） |
| **P7** Admin CRUD | ❌ **空壳 plugin**（同上） |
| **P8** 多 Adapter | 🟡 **只做 3 家**（OpenAI/Claude/Gemini），目标 5 家；`channel_type_to_adapter_kind` 里 `Baidu → OpenAICompat` 兜底 |
| **P9** 韧性层 | ❌ 全无（无 retry/failover/circuit_breaker） |
| **P10** 其他 14 adapter | ❌ 全无 |
| **P11** TaskAdapter | ❌ 全无 |

**结论**：协议层（core+convert）做得最透，类型更干净；**业务面基本停在最小链路**——billing / admin / 韧性 / 异步任务 是四个零实现的主空白。

---

## 二、底层缺口（按模块对比）

### 2.1 调度编排（orchestrator）—— 差距最大

| 能力 | NewAPI | OneAPI | AxonHub | summer-ai |
|---|---|---|---|---|
| 多 candidate 选择 | ✅ `channel_select.go` | ✅ `distributor` middleware | ✅ `candidates.go` 598 行 | ❌ 只选一个 |
| LB 策略丰富度 | Weight + Priority | Weight | **9 种**（BP/RR/Weight/Latency/Random/RateLimit/Trace/Composite/ModelAwareCB） | Weight 一种 |
| 同 channel 重试 | ✅ | ❌ | ✅ `ChannelRetryable` | ❌ |
| 跨 channel failover | ✅ `relay.go` retry | ✅ | ✅ `Retryable.NextChannel` | ❌ |
| Circuit Breaker | ✅ `channel_satisfy.go` | ❌ | ✅ `lb_strategy_model_aware_circuit_breaker.go` + `biz/model_circuit_breaker.go` | ❌ |
| 连接追踪 | ✅ `channel_affinity_cache.go` | ❌ | ✅ `connection_tracker.go` | ❌ |
| 空响应探测 | ❌ | ❌ | ✅ `WithEmptyResponseDetection` 预读 3 事件 | ❌ |
| 率限跟踪 | ✅ `rate-limit` middleware | ✅ `rate-limit` middleware | ✅ `rate_limit_tracking.go` | ❌ |
| channel 自动禁用 | ✅ `channel-billing.go` | ✅ `controller/channel.go` | ✅ `channel_auto_disable.go` | ❌ |
| channel 健康探测 | ✅ `monitor/channel.go` | ✅ `monitor/` | ✅ `channel_probe.go` | ❌（entity 有，后台任务无） |
| 定时同步 models | ✅ `missing_models.go` | ❌ | ✅ `channel_model_sync.go` | ❌（adapter 有 `fetch_model_names` 但没调度） |
| 请求改写模板 | ❌ | ❌ | ✅ `channel_override_template.go` | ❌ |
| Model affinity 粘连 | ✅ channel_affinity | ❌ | ❌ | ❌ |

### 2.2 计费 billing —— 核心空白

| 能力 | 参考实现 | summer-ai |
|---|---|---|
| 三阶段扣费（reserve/settle/refund） | NewAPI `billing_session.go` + AxonHub `biz/quota.go` | ❌ |
| group_ratio 分组倍率 | NewAPI `setting/ratio_setting/` | ❌（entity 有） |
| PriceResolver（channel × model） | NewAPI `controller/pricing.go` | ❌（`channel_model_price` entity 有） |
| 价格版本化 | AxonHub `channelmodelpriceversion` | ❌（entity 有） |
| token_counter 本地估算 | NewAPI `service/token_counter.go` + `tokenizer.go` | ❌（需引 tiktoken-rs 或 tokenizers） |
| CostProfile 应用（cache 折扣） | NewAPI + AxonHub cost_calc | ❌（core 有 trait 但未算） |
| 异步任务三阶段结算 | NewAPI `task_billing.go` `task_polling.go` | ❌ |
| billing_session 贯穿 | NewAPI `billing_session.go` | ❌ |
| 充值/兑换/订阅 | NewAPI topup/redemption/subscription 全套 | ❌（entity 齐全） |
| CacheCreation 5m/1h 拆分 | NewAPI reasonmap + convert.go | ❌（规格写了未实装） |
| usage_billing_dedup 防重复 | NewAPI | ❌（entity 有） |

### 2.3 异步任务（TaskAdapter）—— 整块空白

- **NewAPI** 已有 10 家 task adapter：midjourney / suno / vidu / luma / runway / kling / sora / hailuo / jimeng / doubao / ali / vertex
- **NewAPI 独有**：`task_polling.go` 独立轮询线程 + `sweepTimedOutTasks` CAS 防并发覆盖 + `scheduler_outbox` 分片
- **summer-ai**：`TaskAdapter` trait 都还没定义；`model/entity/requests/task.rs` 和 `scheduler_outbox.rs` 等表都放着没人写

### 2.4 协议广度 —— 已有 3 家，差 16 家

| 家数 | 参考 | summer-ai |
|---|---|---|
| OpenAI 家族（OpenAI/Azure/OpenAICompat/Responses） | NewAPI 6+ / OneAPI 5 / AxonHub 5 | OpenAI ✅ + OpenAICompat 兜底 + Responses ingress 写完但 **outbound/adapter 未接通** |
| 原生协议 | Anthropic/Gemini/Cohere/Ollama | Anthropic ✅ + Gemini ✅ + Cohere ❌ + Ollama ❌ |
| OpenAI 方言 | Groq/DeepSeek/Xai/Fireworks/Together/Nebius/Zai/BigModel/Aliyun/Mimo/Vertex/GithubCopilot/OllamaCloud | ❌ 全无 |
| 特殊协议 | AWS Bedrock（Sigv4）/ Vertex（GCP ADC）/ GithubCopilot OAuth | AxonHub 有完整独立子包；❌ |
| 入口端点 | 除 chat 外 embeddings/images/audio/rerank/files/batches/realtime(ws) | NewAPI 全有；❌ 只 chat |

### 2.5 安全/合规/鉴权

| 能力 | 参考 | summer-ai |
|---|---|---|
| OAuth（Codex / Copilot / 自建） | NewAPI `codex_oauth.go` / `oauth.go` | ❌（`docs/oauth-design.md` 设计了未实装） |
| Passkey / WebAuthn | NewAPI `controller/passkey.go` | ❌ |
| 2FA | NewAPI `twofa.go` | ❌ |
| Session 管理 | NewAPI + AxonHub | ❌（entity 有） |
| Prompt Protection | AxonHub `biz/prompt_protection_*` + `orchestrator/prompt_protecter.go` | ❌（entity 有） |
| sensitive words | NewAPI `service/sensitive.go` | ❌ |
| audit_log 写入 | all | ❌（entity 有） |
| domain_verification | NewAPI | ❌（entity 有） |
| RBAC | AxonHub `internal/authz/` + `scopes/` | ❌（entity `rbac_policy` 有） |

### 2.6 观测/运维

| 能力 | 参考 | summer-ai |
|---|---|---|
| tracking 落库（request/execution/log 3 表） | all | 🟡 骨架有未实装 |
| trace_span 分布式追踪 | AxonHub `internal/tracing/` | ❌（entity 有） |
| daily_stats 聚合任务 | NewAPI `model/log.go` | ❌ |
| alert_rule + event + silence | —（各自简化实现） | ❌（entity 齐但无逻辑） |
| metrics (Prometheus/OTEL) | AxonHub `internal/metrics/` | ❌ |
| live_streaming preview | AxonHub 独有 | ❌ |
| simulator（e2e 假上游） | AxonHub `llm/simulator/` | ❌ |
| missing_models 自动提示 | NewAPI `controller/missing_models.go` | ❌ |

### 2.7 基础工具库

| 工具 | 参考 | summer-ai |
|---|---|---|
| chunk_buffer（SSE 增量缓冲 + 订阅） | AxonHub `pkg/chunkbuffer/` | 🟡 部分（`stream_driver.rs` 422 行里有） |
| ring_buffer | AxonHub `pkg/ringbuffer/` | ❌ |
| rate_limiter（令牌桶/滑窗） | all | ❌（`governance_rate_limit` entity 有） |
| tokenizer（本地估算） | NewAPI `tokenizer.go`（tiktoken-go） | ❌ |
| idempotency_record | — | ❌（entity 有） |
| streams 操作符（map/filter/slice） | AxonHub `llm/streams/` | ❌（Rust 可直接用 futures） |

### 2.8 Admin 面 —— 整块空白

- NewAPI `controller/` 有 **50+ 个文件**的 admin API，AxonHub `biz/` 有 **70+ 业务服务**
- summer-ai `admin/src/` 只有 `lib.rs + plugin.rs` 两个文件，0 个 router，0 个 service
- 缺：Channel / ChannelAccount / Price / Token / User / Log / Vendor / Model / Pricing / Redemption / Topup 所有 CRUD
- 缺：Dashboard 聚合、playground、前端 React 页

---

## 三、优先级建议（按"阻塞生产"程度排）

### P0 阻塞生产（必须先做，≈ 8 工作日）

1. **P5-Log 真正落库** — tracking 写入 `ai.request / ai.request_execution / ai.log` 三表（骨架已在，只差实装）
2. **P6 三阶段计费** — reserve/settle/refund + PriceResolver + group_ratio + usage_billing_dedup（entity 全齐，纯业务代码）
3. **P9 韧性三件套** — retry + failover（多 candidate 遍历）+ circuit_breaker（按 `channel_account_id` 分组）；**推荐搬 AxonHub 两层接口（`Retryable` + `ChannelRetryable`）**
4. **P7 admin MVP** — Channel / ChannelAccount / Token / Price 的 4 组 CRUD（没 admin 运营改不了任何配置）

### P1 广度扩张（≈ 5 工作日）

5. **Responses outbound** — ✅ 已完成：`OpenAIRespAdapter`、`/v1/responses` egress、`Responses`/`ResponsesStream` service 分派、built-in tools 与 `reasoning` / `previous_response_id` 透传均已接通
6. **P8 补 4 家** — Azure / Ollama / Cohere / DeepSeek（都是 OpenAICompat 近亲，复用模板）
7. **embeddings + rerank 入口** — 定义 `EmbedAdapter` / `RerankAdapter` 子 trait + 路由
8. **channel_probe + model_sync 后台任务** — 挂 tokio 定时任务，摘除不健康 channel

### P2 差异化能力（≈ 6 工作日）

9. **TaskAdapter + Midjourney 单家验证** — 包含 `scheduler_outbox` 轮询、CAS 超时清理、三阶段计费
10. **LB 策略扩展** — RR + Latency + RateLimit（TPM/RPM/并发），对齐 AxonHub 基本面
11. **audio / images 入口** — 补齐 NewAPI 级端点覆盖
12. **tokenizer 本地估算** — 接 `tiktoken-rs` 做 pre-settle 预估（用于 billing reserve）

### P3 合规与多租户（≈ 5 工作日）

13. **OAuth 全家桶** — Codex / Copilot / 自建（已有 `oauth-design.md`）
14. **prompt_protection + sensitive** — 敏感词 + 注入防护（guardrails entity 齐）
15. **audit_log 写入** + RBAC 链路打通
16. **rate_limit 规则引擎** — `governance_rate_limit` 激活
17. **多租户** — tenancy 域（organization / team / project）

### P4 观测与运维（≈ 4 工作日）

18. **daily_stats 聚合** + alert_rule 调度
19. **Prometheus / OTEL metrics**
20. **simulator** e2e 假上游（避免测试消耗真实额度）
21. **live_streaming preview**（AxonHub 特色，运维观察流式内容）

### P5 其余 14 adapter + 管理前端（≈ 7 工作日）

22. P10 机械式铺开剩余 adapter
23. React admin 前端（v1 可直接复用现有 summer-admin 页）

---

## 四、建议的落地次序（一句话）

**先闭环（日志 → 计费 → 韧性 → admin MVP）→ 再补协议广度（Responses outbound + 4 adapter + embeddings）→ 再做异步任务 TaskAdapter → 最后是观测 / 合规 / 多租户。**

现有 80 张 entity 和 4000+ 行 convert 层是非常扎实的底座，当前真正的瓶颈**不是协议层**，而是 **billing / 韧性 / admin** 这三个纯业务代码的空壳。

---

## 五、对比得到的结构性学习（给后续设计吃进去的点）

1. **AxonHub 的 `Inbound`/`Outbound` 双 transformer 抽象**：我们的 `IngressConverter` + `Adapter` 等价映射已对齐，继续保持。
2. **AxonHub 的 `Retryable` + `ChannelRetryable` 两层 trait**：比 NewAPI 单层 retry 更细，P9 直接借鉴。
3. **AxonHub 的 `candidates` 模型**：返 `Vec<Candidate>` 给 LB 层挑，而不是 Store 层直接 pick 一个——避免 Store 和 LB 耦合。当前 `ChannelStore::pick` 应重构为 `candidates()` + LB 层独立。
4. **NewAPI 的 `billing_session` 贯穿**：Request 进入时创建 session，出去时关闭——所有计费/日志都挂在 session 上。我们可以用一个 `Request extension` 承载 `BillingSession`，middleware 链上读。
5. **NewAPI 的 CAS 超时清理**：任务表 `UpdateWithStatus(oldStatus)` 原子推进，防多实例轮询器并发——TaskAdapter 实装时必带。
6. **AxonHub 的 live_streaming + chunkbuffer**：运维能实时看流式内容，对调试极有价值。技术债务小，早加比晚加便宜。

---

## 变更日志

| 日期 | 修改 | 原因 |
|---|---|---|
| 2026-04-21 | 初版 | 与 NewAPI / OneAPI / AxonHub 的全面底层能力对比，作为优先级决策锚点 |