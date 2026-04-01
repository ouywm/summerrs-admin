# summer-ai-hub 完整开发路线图

更新时间：2026-03-31

---

## 目录

- [一、项目定位与愿景](#一项目定位与愿景)
- [二、技术架构总览](#二技术架构总览)
- [三、当前完成度盘点](#三当前完成度盘点)
- [四、参考项目体系](#四参考项目体系)
- [五、开发路线：全阶段详细步骤](#五开发路线全阶段详细步骤)
  - [Phase 1：核心中转引擎（已完成）](#phase-1核心中转引擎已完成)
  - [Phase 2：Provider-Native Runtime 深化](#phase-2provider-native-runtime-深化)
  - [Phase 3：Provider 覆盖扩展](#phase-3provider-覆盖扩展)
  - [Phase 4：可观测性与运维工程化](#phase-4可观测性与运维工程化)
  - [Phase 5：Realtime 协议支持](#phase-5realtime-协议支持)
  - [Phase 6：路由健康引擎成熟化](#phase-6路由健康引擎成熟化)
  - [Phase 7：模型管理与 Provider 模板](#phase-7模型管理与-provider-模板)
  - [Phase 8：用户平台能力](#phase-8用户平台能力)
  - [Phase 9：企业级治理](#phase-9企业级治理)
  - [Phase 10：管理后台 UI](#phase-10管理后台-ui)
- [六、数据库 Schema 演进路线](#六数据库-schema-演进路线)
- [七、测试与验收策略](#七测试与验收策略)
- [八、风险与技术决策](#八风险与技术决策)

---

## 一、项目定位与愿景

### 1.1 是什么

`summer-ai-hub` 是一个**平台化的 Rust AI Gateway**，目标是：

- 吸收 `one-api` / `new-api` 的**平台运营能力**（渠道管理、Token 发放、配额计费、日志统计）
- 保留 Rust 网关的**工程内核优势**（高性能、类型安全、零成本抽象、流式处理）
- 提供**企业级治理**能力（多租户、RBAC、Guardrail、审计）

### 1.2 技术栈

| 层 | 技术 |
|---|---|
| 语言 | Rust 2024 edition |
| 框架 | Summer（自研，基于 Axum + Tokio） |
| ORM | SeaORM 2.0 |
| 数据库 | PostgreSQL（`ai` schema） |
| 缓存 | Redis |
| HTTP 客户端 | reqwest |
| SSE | reqwest-eventsource + async-stream |
| 序列化 | serde + serde_json |

### 1.3 核心设计原则

1. **配置全走数据库** — 运行时可动态调整，不依赖配置文件
2. **三段式 Provider Adapter** — `build_request` → `parse_response` → `parse_stream`
3. **预扣 + 后结算计费** — 原子操作保证配额一致性
4. **Plugin 架构** — `SummerAiHubPlugin` 通过 `AppBuilder` 注册
5. **serde(flatten) 透传未知字段** — 最大化 OpenAI 兼容性

---

## 二、技术架构总览

### 2.1 Crate 分层

```
summer-ai (伞 crate)
├── summer-ai-core      协议适配层：ProviderAdapter trait + 类型定义
├── summer-ai-hub       核心引擎：Relay + Auth + Router + Service + Job
└── summer-ai-model     数据模型：Entity + DTO + VO
```

依赖方向：`app` → `summer-ai-hub` → `summer-ai-model` → `summer-common`

### 2.2 请求处理流水线

```
客户端请求
  │
  ├── 1. AiAuthLayer (Tower Layer) → Bearer Token 验证
  │     └── SHA-256 Hash + Redis 缓存 (1min TTL)
  │     └── AiToken Extractor 注入 Axum Extensions
  │
  ├── 2. Handler 解析请求体
  │     └── 提取 model + endpoint_scope
  │
  ├── 3. ChannelRouter 选路
  │     └── Ability 表查询 → priority 分组 → health 评分 → weight 加权随机
  │     └── RouteSelectionPlan (Iterator) 支持 fallback 遍历
  │
  ├── 4. RateLimit 检查
  │     └── RPM / TPM / Concurrency (Redis 滑动窗口)
  │
  ├── 5. BillingEngine 预扣配额
  │     └── 原子 DB 更新 token.remain_quota
  │
  ├── 6. ProviderAdapter 构建上游请求
  │     └── get_adapter(channel_type) → 零状态全局静态实例
  │     └── build_request() 协议转换
  │
  ├── 7. UpstreamHttpClient 发送请求
  │     ├── 非流式：parse_response() → ChatCompletionResponse
  │     └── 流式：  parse_stream() → SSE 桥接 → 客户端
  │
  ├── 8. 成功：post_consume() 结算 + 写 Log
  │     └── 流式：后台 Tokio task 延迟结算
  │
  └── 9. 失败：refund() 回滚 + exclude channel → 重试下一候选
```

### 2.3 核心模块职责

| 模块 | 位置 | 职责 |
|---|---|---|
| **ProviderAdapter** | `core/src/provider/` | 协议归一化：OpenAI/Anthropic/Gemini/Azure → 统一类型 |
| **AiAuthLayer** | `hub/src/auth/` | Token 验证、Redis 缓存、Axum extractor |
| **ChannelRouter** | `hub/src/relay/channel_router.rs` | 渠道选择：priority + weight + health + fallback |
| **BillingEngine** | `hub/src/relay/billing.rs` | 预扣/结算/回滚配额，模型倍率 × 分组倍率 |
| **RateLimit** | `hub/src/relay/rate_limit.rs` | 分布式限流：RPM/TPM/并发 (Redis) |
| **SSE Stream** | `hub/src/relay/stream.rs` | 上游 SSE → 协议转换 → 下游 SSE，后台结算 |
| **ResourceAffinity** | `hub/src/service/resource_affinity.rs` | 有状态资源绑定：Files/Assistants/Threads 亲和路由 |
| **RouteHealth** | `hub/src/service/route_health.rs` | 短期惩罚计数 + TTL 衰减 (Redis) |
| **RuntimeCache** | `hub/src/service/runtime_cache.rs` | Redis JSON/计数器/有序集合包装 |
| **LogBatchQueue** | `hub/src/service/log_batch.rs` | 批量异步写日志到 PostgreSQL |
| **ChannelRecovery** | `hub/src/job/channel_recovery.rs` | 定时探测 AutoDisabled 渠道并恢复 |

---

## 三、当前完成度盘点

### 3.1 已完成能力一览

#### 核心中转链路 ✅

| 能力 | 状态 | 说明 |
|---|---|---|
| OpenAI chat/completions | ✅ 完成 | 流式 + 非流式，多 provider 协议转换 |
| Responses API | ✅ 完成 | Anthropic/Gemini 走 bridge |
| Embeddings | ✅ 完成 | Gemini 原生支持 |
| Completions | ✅ 完成 | |
| Images (generations/edits/variations) | ✅ 完成 | |
| Audio (speech/transcriptions/translations) | ✅ 完成 | |
| Moderations | ✅ 完成 | |
| Rerank | ✅ 完成 | |
| Models (list/detail/delete) | ✅ 完成 | |

#### 资源型 Passthrough ✅

| 接口族 | 状态 |
|---|---|
| Files (CRUD + content) | ✅ |
| Batches (CRUD + cancel) | ✅ |
| Assistants (CRUD) | ✅ |
| Threads + Messages + Runs + Steps | ✅ |
| Vector Stores + Files + File Batches | ✅ |
| Fine-tuning Jobs + Events + Checkpoints | ✅ |
| Uploads + Parts + Complete + Cancel | ✅ |

#### 基础设施 ✅

| 能力 | 状态 |
|---|---|
| Token 验证 (SHA-256 + Redis) | ✅ |
| 渠道路由 (priority + weight + health) | ✅ |
| 计费引擎 (预扣 + 结算 + 回滚) | ✅ |
| 分布式限流 (RPM/TPM/Concurrency) | ✅ |
| 资源亲和路由 | ✅ |
| 路由健康惩罚 | ✅ |
| 渠道自动恢复 (cron) | ✅ |
| 批量日志写入 | ✅ |
| Runtime 缓存 | ✅ |
| Dashboard / Runtime API | ✅ |

#### Provider Adapter ✅

| Provider | chat | responses | embeddings | stream | error mapping |
|---|---|---|---|---|---|
| OpenAI (type=1) | ✅ | ✅ | ✅ | ✅ | ✅ |
| Anthropic (type=3) | ✅ | ✅ bridge | ❌ | ✅ | ✅ |
| Gemini (type=24) | ✅ | ✅ bridge | ✅ native | ✅ | ✅ |
| Azure (type=14) | ✅ | ✅ | ✅ | ✅ | ✅ (委托 OpenAI) |

#### 管理后端 API ✅

| 资源 | CRUD | 额外操作 |
|---|---|---|
| Channel | ✅ | 测速 (test)、启用/禁用 |
| Channel Account | ✅ | |
| Token | ✅ | |
| Model Config | ✅ | |
| Log | ✅ | 查询 + 统计 |
| Dashboard | ✅ | 汇总统计 |
| Runtime | ✅ | health / routes / summary |

### 3.2 当前还缺什么

| 缺口 | 优先级 | 对标差距 |
|---|---|---|
| Provider 覆盖不足 (仅 4 家) | P0 | 对比 new-api/one-api 30+ provider |
| Provider-native runtime 不完整 | P0 | responses/images/audio 大量走 passthrough |
| Realtime 协议 | P0 | WebSocket 代理未实现 |
| Prometheus / OTel | P1 | 无外部可观测接口 |
| 管理后台 UI | P1 | 后端 API 有但无前端 |
| 用户体系 / SSO / OIDC | P2 | 平台化必要能力 |
| 支付 / 充值 / 钱包 | P2 | 商业化闭环 |
| 多租户 / 审计 / Guardrail | P2 | 企业级能力 |

---

## 四、参考项目体系

本项目的设计决策基于对 30 个开源项目的深度源码分析，所有代码已克隆至 `docs/relay/`。

### 4.1 核心参考矩阵

| 项目 | 语言 | 定位 | 我们主要学什么 |
|---|---|---|---|
| **one-api** | Go | 渠道聚合平台 | Adaptor 三段式、计费流程、Ability 路由模型 |
| **new-api** | Go | one-api 增强版 | Realtime、MCP、支付、更多 provider |
| **one-hub** | Go | one-api 增强版 | 更广的 OpenAI 接口覆盖面 |
| **Traceloop Hub** | Rust | 完整 LLM Gateway | Provider trait、SSE 管道 |
| **crabllm** | Rust | LiteLLM 风格 | Extension hook、stream::unfold、Registry |
| **anthropic-proxy-rs** | Rust | 协议桥接 | Anthropic ↔ OpenAI 状态机转换 |
| **Hadrian** | Rust | 企业网关 | SSO、RBAC、多租户、审计 |
| **LiteLLM** | Python | 统一调用 + Gateway | Provider 广度、企业版参考 |
| **Portkey Gateway** | TS | Provider Registry | 高性能 adapter 注册模式 |
| **Bifrost** | Go | 网关内核 + UI | 插件体系、Next.js 管理面 |
| **APIPark** | Go | API 平台 | 开发者门户、Playground |
| **axonhub** | Go | 请求追踪 | request/execution/trace 三层 |

### 4.2 参考项目按能力定位

```
                  平台化程度（用户/支付/运营）
                        ↑
                        │
        new-api ●───────┤──────────● APIPark
                        │
        one-api ●───────┤
                        │
        one-hub ●───────┤──────────● LiteLLM (企业版)
                        │
     summer-ai-hub ●────┤──────────● Bifrost
     (当前位置)         │
                        │──────────● Hadrian (企业级)
                        │
          crabllm ●─────┤──────────● Portkey
                        │
     Hub (Traceloop) ●──┤
                        │
      ai-gateway ●──────┤
                        │
                        └──────────────────────→ 工程内核成熟度
```

---

## 五、开发路线：全阶段详细步骤

### Phase 1：核心中转引擎（已完成）

> **状态**: ✅ 全部完成
>
> 这一阶段的所有目标都已达成，不再需要进一步工作。

**已完成内容**：

1. ✅ SeaORM Entity 生成（8 张核心表）
2. ✅ 数据库 Schema 创建（`sql/ai/` 77 个 DDL）
3. ✅ OpenAI 透传 Adapter
4. ✅ Token 验证 + Redis 缓存
5. ✅ 非流式 chat/completions 端到端
6. ✅ 流式 SSE 中继
7. ✅ 预扣 + 后结算计费
8. ✅ Anthropic / Gemini / Azure Adapter
9. ✅ 渠道路由引擎
10. ✅ 管理后端 API (Channel / Token / ModelConfig / Log / Dashboard)
11. ✅ 资源型 Passthrough (Files / Batches / Assistants / Threads / VectorStores / FineTuning / Uploads)
12. ✅ 资源亲和路由
13. ✅ 分布式限流
14. ✅ 渠道自动恢复
15. ✅ 路由健康惩罚

---

### Phase 2：Provider-Native Runtime 深化

> **目标**：把当前大量靠 OpenAI passthrough 的接口，替换成真正 provider-aware 的 runtime
>
> **优先级**：P0 — 当前最高优先事项

#### Step 2.1：Responses API Provider-Native 化

**当前状态**：Anthropic / Gemini 的 responses 走 bridge（先翻译成 chat 再转回来）

**目标**：各 provider 对 responses 有真实的运行时语义、失败语义、回退语义

**具体任务**：

| # | 任务 | 涉及文件 | 验收标准 |
|---|---|---|---|
| 2.1.1 | Anthropic responses 回归测试 | `core/src/provider/anthropic.rs` | bridge 在 thinking/tool_use 场景不丢数据 |
| 2.1.2 | Gemini responses 回归测试 | `core/src/provider/gemini.rs` | bridge 在 function_call 场景不丢数据 |
| 2.1.3 | responses 错误语义归一化 | `hub/src/router/openai.rs` | provider 原生错误 → OpenAI error format |
| 2.1.4 | responses 流式 bridge 回归 | `hub/src/relay/stream.rs` | SSE chunk 格式与 OpenAI responses 流一致 |

#### Step 2.2：Images / Audio / Rerank Provider 策略确认

**当前状态**：这些接口全部走 `openai_passthrough.rs`

**目标**：确认每个接口对每个 provider 的策略：native / bridge / unsupported

**具体任务**：

| # | 任务 | 说明 |
|---|---|---|
| 2.2.1 | 制作 "接口 × Provider × 策略" 矩阵 | 枚举每个接口在每个 provider 上的真实支持情况 |
| 2.2.2 | Gemini images/generations native | Gemini 有原生 Imagen API，可做 native adapter |
| 2.2.3 | Anthropic 不支持的接口返回 `unsupported` | 如 images/audio → 返回 422 而非静默 passthrough |
| 2.2.4 | 为 passthrough 接口补 usage/settlement 边界 | 资源创建型接口需要明确计费语义 |

#### Step 2.3：资源型接口 Route-Level 回归

**目标**：资源型接口不只是"有路由"，而是有完整的运行时语义

**具体任务**：

| # | 任务 | 验收标准 |
|---|---|---|
| 2.3.1 | Files 创建 → 后续读取走资源亲和 | `POST /v1/files` → `GET /v1/files/{id}` 命中同一 channel |
| 2.3.2 | Assistants/Threads/Runs 链路回归 | 完整创建 → 运行 → 读取 → 取消链路 |
| 2.3.3 | Batches 生命周期回归 | 创建 → 查询 → 取消 → 清理 |
| 2.3.4 | Vector Stores 资源链回归 | 创建 → 添加文件 → 搜索 → 删除 |
| 2.3.5 | 资源型 POST 接口补计费语义 | runs 等有 usage 的接口需要结算 |

---

### Phase 3：Provider 覆盖扩展

> **目标**：从 4 家 provider 扩展到 8-10 家，覆盖主流 AI API
>
> **优先级**：P0

#### Step 3.1：OpenRouter (type=TBD)

**理由**：聚合入口，一个 adapter 可覆盖大量模型

**具体任务**：

| # | 任务 | 说明 |
|---|---|---|
| 3.1.1 | 注册 Provider 类型编号 | 在 `ChannelType` enum 新增变体 |
| 3.1.2 | 实现 `OpenRouterAdapter` | OpenAI 兼容 + 额外 header (`X-Title`, `HTTP-Referer`) |
| 3.1.3 | 模型映射 | OpenRouter 模型名格式 `provider/model` |
| 3.1.4 | 错误码映射 | OpenRouter 特定错误 → ProviderErrorKind |
| 3.1.5 | chat 流式/非流式回归 | 至少覆盖 3 个上游模型 |

**涉及文件**：
- 新增：`core/src/provider/openrouter.rs`
- 修改：`core/src/provider/mod.rs` (注册)
- 修改：`model/src/entity/_entity/channel.rs` (ChannelType enum)

#### Step 3.2：Ollama (type=28)

**理由**：本地模型场景刚需

**具体任务**：

| # | 任务 | 说明 |
|---|---|---|
| 3.2.1 | 实现 `OllamaAdapter` | Ollama 提供 OpenAI 兼容 `/v1/chat/completions` |
| 3.2.2 | 特殊处理：无 usage 返回 | Ollama 可能不返回 token usage |
| 3.2.3 | 特殊处理：本地网络 | base_url 通常是 `http://localhost:11434` |
| 3.2.4 | embeddings 支持 | Ollama 支持 `/api/embeddings` |
| 3.2.5 | 模型拉取状态 | 考虑是否暴露 `POST /api/pull` |

**涉及文件**：
- 新增：`core/src/provider/ollama.rs`

#### Step 3.3：Qwen / 阿里通义 (type=17)

**理由**：国内主力 provider

**具体任务**：

| # | 任务 | 说明 |
|---|---|---|
| 3.3.1 | 实现 `QwenAdapter` | DashScope API 或 OpenAI 兼容模式 |
| 3.3.2 | 多模态支持 | qwen-vl 系列图片输入 |
| 3.3.3 | function calling 映射 | Qwen tool_call 格式 |
| 3.3.4 | SSE 流式差异 | DashScope SSE 格式与 OpenAI 不完全一致 |

**涉及文件**：
- 新增：`core/src/provider/qwen.rs`

#### Step 3.4：AWS Bedrock (type=TBD)

**理由**：企业级 AI 接入刚需

**具体任务**：

| # | 任务 | 说明 |
|---|---|---|
| 3.4.1 | 引入依赖 | `aws-sdk-bedrockruntime` + `aws-sigv4` |
| 3.4.2 | 实现 `BedrockAdapter` | AWS Sigv4 签名认证 |
| 3.4.3 | 请求映射 | Bedrock Converse API → OpenAI 格式 |
| 3.4.4 | 流式处理 | Bedrock 用 `ResponseStream` (非标准 SSE) |
| 3.4.5 | 多模型路由 | 同一 Bedrock 渠道可路由到 Claude/Llama/Mistral |

**涉及文件**：
- 新增：`core/src/provider/bedrock.rs`
- 修改：`core/Cargo.toml` (aws deps)

#### Step 3.5：Baidu / 文心 (type=15)

**具体任务**：

| # | 任务 | 说明 |
|---|---|---|
| 3.5.1 | 实现 `BaiduAdapter` | 文心 ERNIE API |
| 3.5.2 | 认证方式 | API Key + Secret Key → Access Token |
| 3.5.3 | 请求格式转换 | 文心自有格式 → OpenAI |
| 3.5.4 | Token 自动刷新 | Access Token 有效期管理 |

#### Step 3.6：每个新 Provider 的统一验收标准

每新增一个 provider 必须完成：

- [ ] `chat/completions` 非流式通过
- [ ] `chat/completions` 流式通过
- [ ] 错误码映射完整（rate_limit / auth / not_found / server_error）
- [ ] 路由选路正常（ability 表配置后可调度）
- [ ] 计费结算正常（usage 提取 + model_ratio 计算）
- [ ] 日志写入正常（ai.log 有记录）
- [ ] 渠道测速正常（/test endpoint 可用）
- [ ] curl 回归命令文档化

---

### Phase 4：可观测性与运维工程化

> **目标**：让网关运行态可被外部监控系统直接抓取
>
> **优先级**：P1

#### Step 4.1：Prometheus 指标导出

**具体任务**：

| # | 任务 | 说明 |
|---|---|---|
| 4.1.1 | 引入依赖 | `metrics` + `metrics-exporter-prometheus` |
| 4.1.2 | 暴露 `/metrics` endpoint | 在 plugin.rs 注册路由 |
| 4.1.3 | 请求维度指标 | `ai_request_total{provider,model,status,stream}` |
| 4.1.4 | 延迟指标 | `ai_request_duration_seconds{provider,model}` (histogram) |
| 4.1.5 | Token 维度指标 | `ai_tokens_total{direction=input/output,model}` |
| 4.1.6 | 路由指标 | `ai_route_fallback_total{provider}`, `ai_route_retry_total` |
| 4.1.7 | 限流命中指标 | `ai_rate_limit_hit_total{type=rpm/tpm/concurrency}` |
| 4.1.8 | 计费指标 | `ai_billing_refund_total`, `ai_billing_settlement_failure_total` |
| 4.1.9 | 渠道健康指标 | `ai_channel_status{id,name,status}` (gauge) |

**涉及文件**：
- 新增：`hub/src/metrics.rs`
- 修改：`hub/src/plugin.rs` (注册 /metrics)
- 修改：`hub/Cargo.toml` (metrics deps)
- 修改：relay 各模块 (埋点)

#### Step 4.2：OpenTelemetry Tracing

**具体任务**：

| # | 任务 | 说明 |
|---|---|---|
| 4.2.1 | 引入依赖 | `opentelemetry` + `tracing-opentelemetry` |
| 4.2.2 | 配置 OTLP exporter | gRPC/HTTP → Jaeger/Tempo |
| 4.2.3 | 请求级 Span | 每个 relay 请求生成 span（trace_id, span_id） |
| 4.2.4 | 上游调用 Span | reqwest 请求作为 child span |
| 4.2.5 | 错误标记 | 失败/重试/回退在 span 上标记 |
| 4.2.6 | x-request-id 关联 | 客户端传入的 request-id 与 trace-id 关联 |

#### Step 4.3：运维 Dashboard 增强

**具体任务**：

| # | 任务 | 说明 |
|---|---|---|
| 4.3.1 | provider 维度健康聚合 | 按 provider type 聚合成功率/延迟/错误分布 |
| 4.3.2 | channel 维度实时状态 | 当前启用/禁用/恢复中渠道数 |
| 4.3.3 | token 维度用量趋势 | 按 token 维度的请求量/token 消耗趋势 |
| 4.3.4 | 限流命中热点 | 哪些 token/model 触发限流最多 |
| 4.3.5 | 重试/回退/补偿统计 | 各渠道的重试次数、回退次数分布 |

---

### Phase 5：Realtime 协议支持

> **目标**：实现 `/v1/realtime` WebSocket 代理
>
> **优先级**：P1

#### Step 5.1：WebSocket 基础设施

**具体任务**：

| # | 任务 | 说明 |
|---|---|---|
| 5.1.1 | 引入依赖 | `axum` 已内置 WebSocket 支持 (`ws` feature) |
| 5.1.2 | WebSocket 升级握手 | `GET /v1/realtime` → 101 Upgrade |
| 5.1.3 | 双向流桥接 | 客户端 ↔ 网关 ↔ 上游 WebSocket |
| 5.1.4 | 连接生命周期管理 | 心跳、超时、优雅关闭 |

#### Step 5.2：Realtime 鉴权与路由

**具体任务**：

| # | 任务 | 说明 |
|---|---|---|
| 5.2.1 | Token 验证 (WebSocket) | query param 或首帧 Bearer token |
| 5.2.2 | 渠道路由 | 按模型选择上游 WebSocket endpoint |
| 5.2.3 | 资源亲和 | 同一 session 绑定同一渠道 |

#### Step 5.3：Realtime Usage 与日志

**具体任务**：

| # | 任务 | 说明 |
|---|---|---|
| 5.3.1 | 音频 token 计量 | Realtime 按音频时长/token 计费 |
| 5.3.2 | 会话日志 | 记录 Realtime 会话的起止、用量 |
| 5.3.3 | 错误映射 | 上游 Realtime 错误 → 标准化下发 |

**涉及文件**：
- 新增：`hub/src/router/openai_realtime.rs`
- 新增：`hub/src/relay/websocket.rs`
- 修改：`hub/src/plugin.rs` (注册路由)

---

### Phase 6：路由健康引擎成熟化

> **目标**：从当前的"能用"升级到"成熟自愈引擎"
>
> **优先级**：P1

#### Step 6.1：细颗粒度惩罚策略

| # | 任务 | 说明 |
|---|---|---|
| 6.1.1 | 区分错误类型权重 | rate_limit 轻惩罚，auth 错误重惩罚，5xx 中等 |
| 6.1.2 | account 级惩罚 | 同一 channel 下不同 account 独立惩罚 |
| 6.1.3 | 衰减策略 | 指数衰减替代简单 TTL |

#### Step 6.2：熔断器模式

| # | 任务 | 说明 |
|---|---|---|
| 6.2.1 | 三态熔断器 | Closed → Open → Half-Open |
| 6.2.2 | 渠道自动禁用阈值可配 | 连续失败 N 次 → AutoDisabled |
| 6.2.3 | 恢复探针增强 | 不同 endpoint_scope 独立探针 |

#### Step 6.3：与限流/计费联动

| # | 任务 | 说明 |
|---|---|---|
| 6.3.1 | rate_limit 命中计入健康评分 | 被限流 = 短期不可用 |
| 6.3.2 | 结算失败计入健康 | post_consume 失败 → 降低优先级 |
| 6.3.3 | health 指标可视化 | runtime API 展示每个渠道的健康评分曲线 |

---

### Phase 7：模型管理与 Provider 模板

> **目标**：让模型配置和 provider 接入更接近产品体验
>
> **优先级**：P1

#### Step 7.1：/v1/models 增强

| # | 任务 | 说明 |
|---|---|---|
| 7.1.1 | models 与 ability 一致性 | 返回的模型列表 = 当前 token 可调度的模型 |
| 7.1.2 | 模型能力标签 | `vision`, `tool_call`, `reasoning`, `json_mode` |
| 7.1.3 | 模型元信息 | context_window, max_output_tokens |

#### Step 7.2：Provider 预置模板

| # | 任务 | 说明 |
|---|---|---|
| 7.2.1 | 内置 provider 元数据 | 每种 provider type 的默认 base_url、支持的 endpoint_scopes |
| 7.2.2 | 模型同步 | 从上游 `/v1/models` 自动发现可用模型 |
| 7.2.3 | 渠道创建向导 | 选 provider → 自动填充 base_url、模型列表、endpoint_scopes |

**涉及文件**：
- 新增 / 修改：`hub/src/service/model.rs`
- 新增 SQL：`sql/ai/vendor.sql` (已有 `ai.vendor` 表)
- 修改：`hub/src/router/openai.rs` (/v1/models 增强)

---

### Phase 8：用户平台能力

> **目标**：从"开发者工具"变成"可运营平台"
>
> **优先级**：P2

#### Step 8.1：用户体系

| # | 任务 | 说明 |
|---|---|---|
| 8.1.1 | 用户注册/登录 | 基于 `sys.user` 的用户系统 |
| 8.1.2 | SSO / OIDC 接入 | Google / GitHub / 企业微信 |
| 8.1.3 | API Key 自助管理 | 用户自行创建/删除 Token |
| 8.1.4 | 用量查看 | 用户自行查看消费日志和配额 |

**SQL 基础**：已有 `ai.organization`, `ai.project`, `ai.team` 等表

#### Step 8.2：分组与配额策略

| # | 任务 | 说明 |
|---|---|---|
| 8.2.1 | 分组倍率精细化 | 不同分组不同定价 |
| 8.2.2 | 套餐 / 配额策略 | 免费/基础/专业 套餐 |
| 8.2.3 | 配额自动过期 | 按月/按季度配额重置 |

#### Step 8.3：充值与支付

| # | 任务 | 说明 |
|---|---|---|
| 8.3.1 | 钱包系统 | `ai.user_quota` 余额管理 |
| 8.3.2 | 充值接口 | 对接第三方支付 |
| 8.3.3 | 兑换码 | 批量生成、核销 |
| 8.3.4 | 账单导出 | 月度账单 CSV/PDF |

**SQL 基础**：已有 `ai.subscription_plan`, `ai.order`, `ai.transaction`, `ai.discount`, `ai.referral` 等表

---

### Phase 9：企业级治理

> **目标**：面向企业私有化部署场景
>
> **优先级**：P2

#### Step 9.1：多租户

| # | 任务 | 说明 |
|---|---|---|
| 9.1.1 | Organization / Team / Project | 三级资源隔离 |
| 9.1.2 | 资源归属 | Channel / Token / Model 按 org/project 隔离 |
| 9.1.3 | 配额级联 | Org 总配额 → Team 子配额 → Project 子配额 |

**SQL 基础**：已有完整的 `ai.organization`, `ai.team`, `ai.project`, `ai.*_membership` 表

#### Step 9.2：RBAC 与审计

| # | 任务 | 说明 |
|---|---|---|
| 9.2.1 | 细粒度 RBAC | `ai.rbac_policy` + `ai.rbac_policy_version` |
| 9.2.2 | 审计日志 | `ai.audit_log` 记录管理操作 |
| 9.2.3 | 敏感操作二次确认 | 删除渠道、修改密钥等 |

#### Step 9.3：Guardrail / 内容治理

| # | 任务 | 说明 |
|---|---|---|
| 9.3.1 | 输入过滤 | `ai.guardrail_rule` 定义过滤规则 |
| 9.3.2 | 输出过滤 | 响应内容扫描 |
| 9.3.3 | 违规审计 | `ai.guardrail_violation` 记录 |
| 9.3.4 | 策略指标 | `ai.guardrail_metric_daily` 日报 |

**SQL 基础**：已有 `ai.guardrail_config`, `ai.guardrail_rule`, `ai.guardrail_violation`, `ai.guardrail_metric_daily`, `ai.prompt_protection_rule` 等表

#### Step 9.4：高级运维

| # | 任务 | 说明 |
|---|---|---|
| 9.4.1 | 死信队列 | `ai.dead_letter_queue` 处理失败请求 |
| 9.4.2 | 重试管理 | `ai.retry_attempt` 追踪重试历史 |
| 9.4.3 | 调度出箱 | `ai.scheduler_outbox` 可靠异步任务 |
| 9.4.4 | 告警规则 | `ai.alert_rule` / `ai.alert_event` / `ai.alert_silence` |

---

### Phase 10：管理后台 UI

> **目标**：提供可视化管理界面
>
> **优先级**：P2 — 当前明确暂缓，后端 API 优先

#### Step 10.1：核心管理页面

| # | 页面 | 对应后端 API |
|---|---|---|
| 10.1.1 | 渠道管理 | `/api/ai/channel/*` |
| 10.1.2 | 渠道账号管理 | `/api/ai/channel-account/*` |
| 10.1.3 | Token 管理 | `/api/ai/token/*` |
| 10.1.4 | 模型配置 | `/api/ai/model-config/*` |
| 10.1.5 | 日志查询 | `/api/ai/log/*` |
| 10.1.6 | Dashboard 仪表盘 | `/api/ai/dashboard/*` |

#### Step 10.2：运维页面

| # | 页面 | 说明 |
|---|---|---|
| 10.2.1 | Runtime 健康总览 | 渠道状态、路由健康评分、活跃连接 |
| 10.2.2 | 渠道测速 | 一键测速所有渠道 |
| 10.2.3 | 实时指标 (Grafana 嵌入) | Prometheus 数据可视化 |

---

## 六、数据库 Schema 演进路线

### 6.1 当前已落地 (sql/ai/)

已有 77 个 DDL 文件，覆盖：

| 域 | 表 | 说明 |
|---|---|---|
| **核心 Relay** | channel, channel_account, ability, token, user_quota, model_config, group_ratio, log | 路由 + 计费 + 日志主线 |
| **运营控制** | vendor, request, request_execution | 请求追踪 |
| **多租户** | organization, team, project, *_membership | 三级资源隔离 |
| **身份** | service_account, invitation, sso_config, scim_config, rbac_policy | 企业身份 |
| **治理** | guardrail_config, guardrail_rule, guardrail_violation, prompt_protection_rule | 内容护栏 |
| **商业化** | subscription_plan, order, transaction, discount, referral, payment_method | 支付账务 |
| **运维** | dead_letter_queue, retry_attempt, scheduler_outbox, alert_rule, alert_event | 可靠性 |
| **应用链路** | session, thread, conversation, message, trace, trace_span | 会话追踪 |
| **存储** | file, managed_object, vector_store, vector_store_file, data_storage | 对象管理 |
| **平台配置** | config_entry, routing_rule, routing_target, plugin, plugin_binding | 扩展能力 |
| **治理预算** | governance_budget, governance_rate_limit | 预算限流 |

### 6.2 Schema 演进原则

1. **当前以 Entity 为驱动** — 只有代码用到的表才生成 Entity
2. **表已提前设计好** — 后续 Phase 需要时直接生成 Entity 即可
3. **不改表结构** — 除非发现设计缺陷，否则不做 DDL 变更
4. **迁移管理** — 每次变更通过 SQL migration 文件追踪

---

## 七、测试与验收策略

### 7.1 每个 Phase 的通用验收

- [ ] `cargo check --workspace` 零错误
- [ ] `cargo test --workspace` 全部通过
- [ ] `cargo clippy --workspace` 无 warning
- [ ] curl 回归测试通过（见 [curl-regression.md](summer-ai-hub-curl-regression.md)）
- [ ] 日志写入正确（ai.log 有记录）
- [ ] 配额扣减正确（token.remain_quota 一致性）

### 7.2 测试层次

| 层 | 覆盖范围 | 工具 |
|---|---|---|
| 单元测试 | Provider 协议转换、计费计算 | `#[test]` |
| 集成测试 | 端到端 relay 链路 | mock upstream + real DB |
| curl 回归 | 每个接口手动验证 | curl 脚本 |
| 负载测试 | 并发性能基线 | k6 / wrk |

### 7.3 Provider 特定验收矩阵

每新增一个 provider / 每个 Phase 完成后，需要跑通：

| 测试项 | 说明 |
|---|---|
| 非流式 chat | 基本功能 |
| 流式 chat | SSE 完整性 |
| 大 prompt | 触发上游 context limit |
| 错误场景 | 无效 key / 模型不存在 / rate limit |
| 多模型路由 | 同 provider 不同 model |
| failover | 首选渠道失败 → 自动切换 |
| 计费验证 | usage 提取 + quota 扣减 + 日志 |

---

## 八、风险与技术决策

### 8.1 已确定的技术决策

| 决策点 | 选择 | 理由 |
|---|---|---|
| LLM 调用方式 | reqwest 原生 HTTP | 行业共识，所有参考项目均如此 |
| SSE 消费 | reqwest-eventsource + async_stream | 比 eventsource-stream 集成度更高 |
| 数据库 | PostgreSQL + SeaORM | 对齐项目现有技术栈 |
| 配置方式 | 全走数据库 | 运行时可动态调整 |
| Provider trait | 三段式 | 对标 one-api Adaptor，最适合中转场景 |
| 路由算法 | priority 分组 + weight 加权随机 + failover | one-api 验证过的成熟方案 |
| 计费模型 | 预扣 + 后结算 | one-api 验证过的成熟方案 |
| Provider 编号 | 对齐 one-api channeltype | 降低迁移成本 |
| Extension 体系 | 暂不引入 | 避免过度设计，按需再加 |
| Anthropic 流转换 | 状态机 + StreamState | crabllm 验证过的方案 |
| Realtime 协议 | Axum 内置 WebSocket | 不引入额外依赖 |
| 指标导出 | metrics crate + Prometheus | Rust 生态标准 |
| 分布式追踪 | OpenTelemetry | 行业标准 |

### 8.2 风险项

| 风险 | 影响 | 缓解措施 |
|---|---|---|
| Provider API 变更频繁 | adapter 需要持续跟进 | serde(flatten) 透传 + 宽松解析 |
| Bedrock 签名复杂 | 实现成本高 | 使用 AWS 官方 SDK |
| Realtime 协议不稳定 | OpenAI Realtime 仍在快速演进 | 先做最小可用，后续跟进 |
| 上游 SSE 格式不一致 | 流式解析可能失败 | 每个 provider 独立 stream parser |
| 数据库表过多 (77 张) | Entity 维护成本 | 按需生成，不一次性全部实现 |
| Redis 单点 | 限流/缓存故障影响全局 | 降级策略：Redis 不可用时放行 |

### 8.3 未来可能的架构演进

| 方向 | 触发条件 | 说明 |
|---|---|---|
| crabllm 风格 Extension 体系 | 计费/限流/日志需要插件化时 | 5 个钩子点：cache/request/response/chunk/error |
| 配置文件 + 数据库混合 | 需要 GitOps 部署时 | 静态基础配置 + 动态运行时覆盖 |
| 多实例部署 | 流量超过单机上限时 | Redis 已保证分布式一致性 |
| gRPC 上游支持 | 接入 VertexAI / Bedrock streaming | 引入 tonic |
| 插件市场 | 社区贡献 provider / 治理规则时 | 基于 `ai.plugin` + `ai.plugin_binding` |

---

## 附录

### A. 关键文档索引

| 文档 | 路径 | 内容 |
|---|---|---|
| 项目规划 | [summer-ai-hub-plan.md](summer-ai-hub-plan.md) | 调研基础 + 设计决策 |
| 实施方案 | [summer-ai-hub-impl.md](summer-ai-hub-impl.md) | 可执行的实施方案 |
| 差距分析 | [summer-ai-hub-gap-analysis.md](summer-ai-hub-gap-analysis.md) | 对标 new-api/one-api |
| 接口矩阵 | [summer-ai-hub-endpoint-matrix.md](summer-ai-hub-endpoint-matrix.md) | OpenAI 兼容面覆盖 |
| 配置验证 | [summer-ai-hub-config-validation.md](summer-ai-hub-config-validation.md) | 数据库配置检查 |
| curl 回归 | [summer-ai-hub-curl-regression.md](summer-ai-hub-curl-regression.md) | 端到端验证命令 |
| Schema V2 | [research/summer-ai-gateway-schema-v2.md](research/summer-ai-gateway-schema-v2.md) | 数据库设计文档 |
| Rust 参考 | [research/rust-llm-gateway-local-repos.md](research/rust-llm-gateway-local-repos.md) | Rust 项目源码分析 |
| 非 Rust 参考 | [research/non-rust-llm-gateway-local-repos.md](research/non-rust-llm-gateway-local-repos.md) | 多语言项目分析 |

### B. 参考项目仓库清单

完整列表见 [docs/relay/repos.json](relay/repos.json)，共 30 个项目：

- **Rust (13 个)**：ai-gateway, anthropic-proxy-rs, claude-code-mux, crabllm, hadrian, hub, llm-connector, llm-providers, llmg, lunaroute, model-gateway-rs, ultrafast-ai-gateway, unigateway
- **Go (8 个)**：APIPark, axonhub, bifrost, CLIProxyAPI, new-api, one-api, one-hub, proxify
- **Python (5 个)**：litellm, crewAI, llm-router-api, llamaxing, claude-code-api
- **TypeScript (3 个)**：portkey-gateway, llmgateway, openai-gateway
- **Java (1 个)**：solon-ai

### C. 当前执行顺序快速参考

```
当前优先级排序：

P0 ─── Phase 2: Provider-Native Runtime 深化 ──────────── 当前做
       Phase 3: Provider 覆盖扩展 ──────────────────────── 当前做

P1 ─── Phase 4: 可观测性 (Prometheus / OTel)
       Phase 5: Realtime 协议
       Phase 6: 路由健康成熟化
       Phase 7: 模型管理与 Provider 模板

P2 ─── Phase 8: 用户平台能力
       Phase 9: 企业级治理
       Phase 10: 管理后台 UI ─────────────────────────── 明确暂缓
```
