# 08 — AI 缓存总设计

> 状态：设计中，未实现
> 优先级结论：先完成文档与能力边界设计，后续实现从 Provider Prompt Cache 开始
> 范围：`summer-ai-core` + `summer-ai-relay` + `summer-ai-billing` 的统一缓存能力设计

---

## 一、背景与目标

`summer-ai` 当前已经具备统一中转、计费、日志、追踪、流式处理等核心能力，但在“AI 缓存”这一层仍然缺少完整设计。

这里所说的“AI 缓存”不是普通热点数据缓存，也不是 `token` / `route` / `runtime config` 的读路径缓存，而是会直接影响以下结果的 LLM 请求缓存能力：

- 用户是否真正享受到上游 Provider 的 Prompt Cache 降本
- 同一上下文重复请求时，是否能够明显降低延迟与成本
- 对“是否官 Key / 是否偷 Token / 是否真的命中官方缓存”的实验结论是否可信
- 平台后续是否能引入“完全相同请求缓存”和“语义相似缓存”

本设计文档的目标不是立即实现全部缓存能力，而是先把总架构定清楚，避免后续分别开发 Prompt Cache、Exact Cache、Semantic Cache 时出现：

- 日志字段设计冲突
- 计费口径不一致
- `trace` / `request` / `request_execution` 观测粒度不足
- “平台自建缓存命中”与“Provider 原生缓存命中”混淆
- 路由调度与缓存命中率相互割裂

---

## 二、设计范围与非目标

### 2.1 本文范围

本文覆盖三类 AI 缓存能力：

1. `Provider Prompt Cache`
   - Provider 侧原生提示缓存
   - 典型代表：Anthropic Prompt Caching
   - 核心价值：降低长前缀上下文的输入成本与首包延迟

2. `Relay Exact Response Cache`
   - Relay 侧对完全相同请求的响应复用
   - 核心价值：重复请求直接复用结果，避免重复推理

3. `Relay Semantic Cache`
   - Relay 侧对语义相近请求的响应复用
   - 核心价值：自然语言相似问题可以降低推理与成本

同时覆盖：

- 缓存观测与计费口径
- 与路由/账号调度的关系
- 与 `request` / `request_execution` / `trace` / `trace_span` / `ai.log` 的落点关系
- 分阶段实现路线

### 2.2 非目标

本文不讨论以下缓存：

- `token` 鉴权缓存
- 路由选择结果缓存
- 配置与健康检查缓存
- UI / 管理端查询缓存

这些缓存仍然重要，但不属于“AI 缓存”主设计。

---

## 三、当前现状与缺口

### 3.1 已有能力

当前 `summer-ai` 已具备以下与缓存相关的基础能力：

- 已能从部分上游响应中归一化 `cached_tokens`
  - 例如 `Anthropic` 的 `cache_read_input_tokens` / `cache_creation_input_tokens`
- `ai.log` 已有 `cached_tokens` 字段，可承接摘要级消费统计
- `request` / `request_execution` 已保留 `request_body` / `response_body`
- `trace` / `trace_span` 已有 `metadata` JSON，可承接细粒度缓存观测
- `daily_stats` / `alert` 已能消费 `cached_tokens`

### 3.2 当前明确缺口

当前系统仍缺少以下关键能力：

1. 缺少统一的请求侧 `cache_control` 建模
   - 当前只能“读到响应中的 `cached_tokens`”
   - 不能显式表达“哪些 message/tool/content block 希望参与 Prompt Cache”

2. 缺少 Prompt Cache 的 Provider 翻译层
   - 不能把统一请求意图翻译为 Anthropic/Bedrock/Vertex 等上游格式

3. 缺少缓存来源区分
   - 目前无法区分：
     - `provider_prompt`
     - `relay_exact`
     - `relay_semantic`
     - `miss`

4. 缺少“缓存命中增强型路由”
   - 对 Prompt Cache 来说，是否落到同一上游账号/渠道，往往直接影响命中率
   - 当前路由与缓存命中策略没有统一设计

5. 缺少 Exact / Semantic Cache 的统一边界
   - 哪些接口支持
   - 哪些请求可缓存
   - 多租户隔离如何做
   - 流式是否支持回放
   - 命中后的成本口径如何定义

6. 缺少统一的缓存实验与验证方案
   - 目前没有“长前缀二次请求命中 Prompt Cache”的 E2E 套件
   - 没有“缓存命中来源不混淆”的观测契约

---

## 四、参考项目结论

本设计不只参考 Rust 项目，而是综合了 Rust / Go / Python / TypeScript 多条实现路线。

### 4.1 `hadrian`（Rust）

最重要的参考样本。

它明确把 AI 缓存拆成三层：

- `Exact Match`
- `Semantic`
- `Prompt`

并且文档边界很清楚：

- `Prompt Cache` 是 Provider-side caching，不是 Gateway 自建缓存
- `Semantic Cache` 是 Exact Cache 之后的第二层命中
- `Exact Cache` 默认更适合 deterministic workload

对 `summer-ai` 的启发：

- 我们也应该先做“三层模型”，而不是把所有缓存混成一个 `CacheService`
- Prompt Cache 的观测必须与 Relay 自建缓存严格区分

### 4.2 `LiteLLM`（Python）

`LiteLLM` 在 Prompt Cache 兼容细节上非常成熟，尤其是：

- `cache_control` 在不同 Provider 上的保留、移除、转换
- Anthropic / Bedrock / OpenAI 兼容处理
- 对不支持的 Provider 显式剥离无效字段，避免污染请求
- Prompt Cache 相关成本计算和日志记录

对 `summer-ai` 的启发：

- Prompt Cache 不是“开关”，而是“请求模型 + Provider 转换 + Usage 解析 + 计费”的完整链路
- 对不支持的 Provider，必须是“安全忽略”而不是“错误透传”

### 4.3 `new-api`（Go）

`new-api` 的重点不是自建 AI response cache，而是：

- 对不同厂商返回的 `cached_tokens` 做强归一化
- 用 `channel_affinity` / `prompt_cache_key` 做粘滞调度

对 `summer-ai` 的启发：

- Prompt Cache 命中率不只是请求体问题，还与“是否回到同一上游账号/渠道”有关
- 应该把“affinity”视为 Prompt Cache 的增强器，而不是额外功能

### 4.4 `one-hub`（Go）

`one-hub` 同时体现了两件事：

- 聊天缓存被做成显式产品能力
- Prompt Cache 相关 token 读写字段被单独建模

对 `summer-ai` 的启发：

- 缓存不是内部技巧，而应当被视作正式能力
- `cached_tokens` 不够，最终需要区分 `read` / `write`

### 4.5 `AxonHub`（Go）

`AxonHub` 很强调 `trace affinity`：

- 同一 `Trace` 尽量优先落到同一上游渠道
- 这样可以显著提高 Provider-side Prompt Cache 命中率

并且它把缓存读写 token 作为正式成本项暴露。

对 `summer-ai` 的启发：

- Prompt Cache 设计不能脱离 `trace` / `thread` / `session`
- 路由层应该预留 cache-aware affinity 机制

### 4.6 `Bifrost`（Go）

`Bifrost` 的亮点是 Semantic Cache 的成本模型很清晰：

- `direct hit`：0 推理成本
- `semantic hit`：只有 embedding lookup 成本
- `miss`：正常推理成本 + embedding 成本

对 `summer-ai` 的启发：

- 语义缓存命中不能简单记为 0 成本
- 需要把“embedding lookup 成本”作为一等成本口径设计

### 4.7 `Portkey`（TypeScript）

`Portkey` 的亮点：

- Response Cache 作为显式服务存在
- `cacheStatus / cacheKey / cacheMode / cacheMaxAge` 都会进入日志与响应上下文
- Anthropic `cache_control` 透传路径清晰

对 `summer-ai` 的启发：

- 即使短期不暴露管理端缓存中心，观测字段也应该先设计好
- Prompt Cache 与 Exact Cache 都需要可解释日志

### 4.8 `LLM Gateway`（TypeScript）

`LLM Gateway` 对我们最有价值的是测试设计：

- 有 Prompt Cache 的 E2E
- 会挑出支持 Prompt Cache 的模型
- 发两次长 system prompt
- 验证第二次 `cached_tokens > 0`
- 同时验证日志里的 `cachedTokens / cachedInputCost`

对 `summer-ai` 的启发：

- Prompt Cache 实现之后必须有真实 E2E，而不是只看单元测试

### 4.9 `sub2api`（Go）

本地仓库里没有完整代码样本，但从公开信息能确认：

- 它依赖 Redis
- 它强调 sticky session / smart scheduling

对 `summer-ai` 的启发：

- “同上下文尽量走同一上游”已经是订阅分发类网关的共识
- 这条思路适合作为 Prompt Cache 命中增强策略纳入总设计

---

## 五、统一分类模型

### 5.1 三层缓存模型

`summer-ai` 的 AI 缓存能力统一定义为三层：

| 层级 | 名称 | 所在位置 | 是否真正调用上游 | 典型收益 |
|------|------|----------|------------------|----------|
| L0 | Provider Prompt Cache | Provider 侧 | 是 | 降低前缀输入成本、降低首包延迟 |
| L1 | Relay Exact Response Cache | Relay 侧 | 否 | 相同请求直接复用结果 |
| L2 | Relay Semantic Cache | Relay 侧 + Vector Store | 否 | 相似问题复用结果 |

### 5.2 统一术语

- `cache_source`
  - `miss`
  - `provider_prompt`
  - `relay_exact`
  - `relay_semantic`

- `cache_hit`
  - 是否命中任意 AI 缓存层

- `cache_affinity`
  - 为提高 Provider Prompt Cache 命中率而进行的粘滞路由偏好

- `cache_observation`
  - 请求完成后对本次缓存行为的统一观测结果

### 5.3 设计关键原则

1. **先区分来源，再统计命中**
   - `provider_prompt` 与 `relay_exact` 不能混为一个“hit”

2. **先观测，再计费优化**
   - Prompt Cache 可以先做到“观测正确”，再逐步补齐 write/read 的精细成本模型

3. **缓存不是路由替代品**
   - Prompt Cache 高命中往往要求 affinity，但 affinity 只是增强器，不是缓存本身

4. **缓存能力必须可关闭、可绕过、可解释**
   - 特别是 Exact / Semantic Cache，必须支持明确 bypass

5. **语义缓存不能污染真实性实验**
   - 测“是否官 Key / 是否偷 Token / 是否上游真命中”时，必须能关闭 Relay 自建缓存

6. **优先复用现有 JSON 承载面**
   - 短期优先用 `request` / `trace` / `trace_span.metadata`
   - 不急着给 `ai.log` 加一堆列

---

## 六、统一观测模型

建议内部统一抽象出如下观测结构，供 `tracking`、`log`、`billing`、`trace` 复用：

```rust
pub struct CacheObservation {
    pub source: CacheSource,
    pub hit: bool,
    pub provider_supported: bool,
    pub affinity_applied: bool,
    pub affinity_key_hash: Option<String>,
    pub relay_cache_key_hash: Option<String>,
    pub prompt_cache_key: Option<String>,
    pub similarity_score: Option<f32>,
    pub ttl_secs: Option<i32>,
    pub read_tokens: i32,
    pub write_tokens: i32,
    pub cached_tokens: i32,
    pub write_ttl_variant: Option<String>,
    pub pricing_fallback: Option<String>,
}

pub enum CacheSource {
    Miss,
    ProviderPrompt,
    RelayExact,
    RelaySemantic,
}
```

### 6.1 与现有记录面的落点关系

#### `ai.log`

保留摘要级字段：

- `cached_tokens`
- `cost_total`
- `price_reference`

不建议在第一阶段就为以下字段大量加列：

- `cache_source`
- `cache_similarity`
- `cache_write_tokens`
- `affinity_key_hash`

原因：

- `ai.log` 是账务/审计摘要，应该尽量保持稳定
- Prompt Cache 第一阶段更适合把细节放进 `request` / `trace` 的 JSON 面

#### `request`

适合存：

- 客户端请求是否表达了缓存意图
- 最终缓存来源摘要
- 对客户端可见的缓存效果摘要

推荐写入 `request.response_body` 摘要或新增 `request` 扩展元数据时承接。

#### `request_execution`

适合存：

- 实际发给上游的请求体
- 是否携带了 Provider 原生 `cache_control`
- 上游返回的缓存相关 usage 原始字段

#### `trace.metadata`

适合存：

- `cache_source`
- `cache_hit`
- `prompt_cache_key`
- `affinity_applied`
- `cached_tokens`

#### `trace_span.metadata`

适合细分到阶段：

- router span：`affinity_applied`、命中目标账号/渠道
- provider span：`provider_supported`、原始 usage cache 字段
- relay-cache span：exact/semantic lookup 命中结果、相似度、ttl 等

---

## 七、目标架构

```text
Client Request
    |
    v
Cache Intent Analyzer
    |
    +--> Prompt Cache Intent
    +--> Exact Cache Eligibility
    +--> Semantic Cache Eligibility
    |
    v
Affinity Resolver
    |
    v
Relay Exact Cache Lookup      (Phase 2)
    |
    +--> Hit -> return cached response
    |
    v
Relay Semantic Cache Lookup   (Phase 3)
    |
    +--> Hit -> return cached response
    |
    v
Provider Request Builder
    |
    +--> translate prompt cache markers
    +--> inject prompt cache key / provider headers if supported
    |
    v
Upstream Provider
    |
    v
Usage / Cache Observation Normalizer
    |
    +--> Billing
    +--> Tracking
    +--> ai.log
    +--> trace / trace_span
```

这个架构有一个明确结论：

- `Prompt Cache` 不在 Relay 前面 short-circuit
- `Exact` / `Semantic` 才会在 Relay 前面 short-circuit
- `Affinity` 是“发送前增强”，不是缓存层

---

## 八、Phase 1：Provider Prompt Cache 设计

### 8.1 目标

第一阶段只做 Provider Prompt Cache 设计与后续实现预留，目标是：

- 明确表达客户端缓存意图
- 正确翻译到支持的上游 Provider
- 正确解析 usage 中的缓存读写信息
- 让路由层尽量提高命中率
- 让日志、追踪、计费看得见缓存效果

### 8.2 为什么先做它

相比 Exact / Semantic Cache，Prompt Cache 更适合作为第一阶段：

- 它更接近“官方能力”
- 它不需要我们先建大块缓存基础设施
- 它对长 system prompt、文档注入、工具定义复用的收益最大
- 它直接影响“是否像官 API”

### 8.3 统一请求模型设计

建议在统一类型系统里新增“Prompt Cache 控制”能力。

#### 建议抽象

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PromptCacheControl {
    pub cache: bool,
    pub ttl_hint: Option<String>,
}
```

可以挂载到以下位置：

- chat message content block
- message 级别
- tool / function definition
- responses API 的输入块

#### 设计原则

1. 内部统一表达，不直接等同某个 Provider 的 JSON
2. `ttl_hint` 只是 hint，不保证所有 Provider 支持
3. 不支持的 Provider 可以安全忽略

### 8.4 Provider 兼容策略

#### Anthropic

第一优先支持。

行为：

- 对支持 Prompt Cache 的内容透传 `cache_control`
- 保留 `ephemeral` 模式
- 如果未来支持 TTL 变体，则由 `ttl_hint` 转换为 Anthropic/Bedrock 兼容格式

#### Bedrock Claude / Vertex Claude

设计上应预留“转换模式”：

- 统一请求模型保留 `PromptCacheControl`
- Provider builder 负责转换为对应上游格式

#### OpenAI / Azure / OpenAI-compatible

设计上分两类：

1. Provider 自身支持自动 prompt cache，但不要求显式 `cache_control`
   - 请求不透传显式标记
   - 重点放在 usage 观测、`cached_tokens` 解析、路由 affinity

2. OpenAI-compatible 厂商提供额外字段
   - 如 `prompt_cache_key`
   - 可以作为扩展能力透传，但不纳入统一协议必填项

#### 不支持的 Provider

要求：

- 安全剥离缓存控制字段
- 不因 `cache_control` 存在而直接失败
- 在观测里标注 `provider_supported=false`

### 8.5 Prompt Cache Affinity 设计

Prompt Cache 的命中率通常受“是否复用同一上游账号/渠道”影响，因此建议设计 `cache-aware affinity`。

#### 关键思路

优先级建议：

1. 客户端显式提供的 `prompt_cache_key`
2. `trace_id` / `thread_id`
3. 标记为 cacheable 的前缀内容指纹

#### 建议抽象

```rust
pub struct PromptCacheAffinity {
    pub key_hash: String,
    pub scope: String,
    pub ttl_secs: i32,
}
```

#### 路由语义

- 不是强绑定，只是软偏好
- 只在同一 `token/group/model family/provider capability` 内复用
- 如果 affinity 命中的账号失败，允许降级到其他账号
- 降级成功后可更新 affinity

#### 为什么必须纳入总设计

因为跨项目经验已经说明：

- `new-api`：`channel_affinity`
- `AxonHub`：`trace affinity`
- `sub2api`：sticky session / smart scheduling

这些都不是巧合，而是 Prompt Cache 命中率与调度一致性的直接关联。

### 8.6 Prompt Cache Usage 归一化

最终要能统一承接如下信息：

- `cached_tokens`
- `cache_read_tokens`
- `cache_write_tokens`
- `cache_write_tokens_5m`
- `cache_write_tokens_1h`

#### 统一口径建议

- `cached_tokens`
  - 面向兼容 OpenAI usage 的统一摘要字段
- `cache_read_tokens`
  - Provider 真正的 cache read token
- `cache_write_tokens`
  - Provider 真正的 cache write token

若 Provider 只返回聚合字段：

- 先保证 `cached_tokens` 正确
- 再在 `trace_span.metadata` 保留原始 payload，供后续补齐

### 8.7 Prompt Cache 计费设计

当前系统已有 `cached_input_ratio` 能力，但完整 Prompt Cache 成本模型仍需扩展。

#### 目标价格项

建议未来价格模型支持以下独立价格项：

- `prompt_tokens`
- `completion_tokens`
- `prompt_cached_tokens`
- `prompt_write_cached_tokens`
- `prompt_write_cached_tokens_5m`
- `prompt_write_cached_tokens_1h`
- `reasoning_tokens`

#### 价格回退链

在价格项缺失时，建议回退顺序如下：

1. 使用明确配置的缓存读/写价格
2. 若无缓存读价格，则回退到当前 `cached_input_ratio`
3. 若仍无，则回退到标准输入价格

并在 `CacheObservation.pricing_fallback` 中记录回退原因。

#### 为什么不建议 Phase 1 先大改计费表

因为第一阶段的首要目标应是：

- 表达正确
- 透传正确
- usage 归一化正确
- 路由 affinity 正确

计费精细化可以在 Prompt Cache 跑通后再收口。

### 8.8 Prompt Cache 观测与测试

#### 最低观测要求

- 第二次请求 `cached_tokens > 0`
- `request_execution.request_body` 中能看到 Provider 侧缓存控制已正确翻译
- `trace_span.metadata` 能看到本次命中来源为 `provider_prompt`
- `ai.log.cached_tokens` 正确落库

#### E2E 测试基线

应参考 `LLM Gateway`：

1. 选择支持 Prompt Cache 的模型
2. 构造长 system prompt
3. 发第一次请求，期望写入缓存
4. 发第二次相同请求，期望 `cached_tokens > 0`
5. 校验日志与追踪摘要

#### 失败场景

必须覆盖：

- Provider 不支持 Prompt Cache
- Provider 支持但字段被错误剥离
- cacheable 前缀太短
- affinity 命中账号不可用后降级
- 流式与非流式 usage 完整度差异

---

## 九、Phase 2：Relay Exact Response Cache 设计

### 9.1 目标

对完全相同请求直接复用结果，减少重复推理。

### 9.2 适用范围

建议按风险从低到高逐步开放：

1. `embeddings`
2. `chat non-stream`
3. `responses non-stream`
4. `chat stream replay`
5. `responses stream replay`

### 9.3 缓存资格判断

建议默认只缓存：

- deterministic 或近 deterministic 请求
- 无时间敏感依赖
- 无用户强个性化差异
- 无安全/合规禁止标记

建议默认排除：

- 高随机性请求
- 强时效请求
- 带工具副作用的请求
- 多模态复杂写入类请求

### 9.4 Cache Key 设计

建议 Cache Key 至少包含：

- tenant/token/project scope
- endpoint
- normalized model
- normalized request body
- temperature
- tools / tool_choice
- response format
- system prompt
- streaming mode

并保留：

- `cache_namespace`
- `cache_key_hash`

### 9.5 存储设计

推荐目标架构：

- L1：本地 `moka`
- L2：Redis
- 不建议第一版落数据库做 Exact Response Cache

原因：

- 这是高频热路径
- Redis 更适合 TTL 和命中管理
- 数据库更适合审计，不适合作为主缓存平面

### 9.6 流式支持策略

最终目标应支持“缓存回放为 SSE”，但不建议第一阶段就实现。

建议分两步：

- Phase 2a：只支持非流式 exact cache
- Phase 2b：把完整响应快照重构为流式 SSE 回放

### 9.7 成本口径

对 `relay_exact` 命中：

- 不发生上游推理成本
- 可选记录缓存存储/保留成本
- `cache_source=relay_exact`

---

## 十、Phase 3：Relay Semantic Cache 设计

### 10.1 目标

让语义相近的问题可以复用历史答案，降低推理成本。

### 10.2 使用边界

语义缓存风险显著高于 Exact Cache，因此必须：

- 显式开关
- 默认关闭
- 默认不参与真实性实验流量
- 默认不用于高风险场景

### 10.3 查询流程

建议固定顺序：

1. Exact Cache Lookup
2. Semantic Cache Lookup
3. Upstream Provider

### 10.4 向量后端

建议设计支持：

- `pgvector`
- `qdrant`

### 10.5 相似度与隔离

语义缓存必须至少按以下维度隔离：

- tenant / project
- provider family
- normalized model family
- endpoint

并要求：

- 初始阈值建议 `0.95`
- 不支持跨租户复用
- 不支持跨能力差异大的模型复用

### 10.6 计费口径

参考 `Bifrost`：

- direct exact hit：0 推理成本
- semantic hit：仅 embedding lookup 成本
- miss：embedding lookup + 正常 LLM 成本

### 10.7 风险

语义缓存最需要防的不是“没命中”，而是“错误命中”。

必须考虑：

- 问题看似相似但上下文不同
- tool / function schema 不一致
- 结构化输出契约不同
- 与实时信息混用

因此它不应早于 Prompt Cache 和 Exact Cache。

---

## 十一、统一管理与可观测性设计

### 11.1 推荐最小管理面

即使短期不做完整管理端，也建议预留以下能力：

- 查看 cache capability config
- 查看 affinity 配置
- 查看 exact/semantic cache 命中率
- 清理 exact cache namespace
- 查看 Prompt Cache 观测统计

### 11.2 指标建议

建议至少有：

- `ai_cache_requests_total{source=...}`
- `ai_cache_hits_total{source=...}`
- `ai_cache_hit_ratio{source=...}`
- `ai_cache_read_tokens_total`
- `ai_cache_write_tokens_total`
- `ai_cache_affinity_applied_total`
- `ai_cache_affinity_fallback_total`

### 11.3 响应与日志建议

未来若需要对客户端显式暴露，可考虑附加：

- `x-summer-cache-source`
- `x-summer-cache-status`

但第一阶段不建议默认暴露给所有客户端，避免兼容性噪音。

---

## 十二、阶段路线图

### Phase 0：当前阶段

- 只完成设计文档
- 不改请求模型
- 不加缓存实现
- 不修改数据库 schema

### Phase 1：Prompt Cache

优先实现：

- 统一 `PromptCacheControl` 请求建模
- Provider 翻译层
- usage 归一化
- affinity 设计落地
- `trace` / `request_execution` 观测
- Prompt Cache E2E

暂不强求：

- 全量价格项精细化
- 管理端界面
- Exact / Semantic Cache

### Phase 2：Exact Response Cache

- 先做 `embeddings + non-stream`
- 后续再做 `stream replay`

### Phase 3：Semantic Cache

- 在 Exact Cache 稳定、观测成熟后再做

---

## 十三、推荐结论

`summer-ai` 的 AI 缓存不应被理解为一个“缓存开关”，而应被定义为三层能力体系：

1. `Provider Prompt Cache`
2. `Relay Exact Response Cache`
3. `Relay Semantic Cache`

其中：

- **Prompt Cache 必须最先做**
  - 因为它最接近官方能力
  - 对“像不像官 API”最关键
  - 对长文档/长 system prompt 场景收益最大

- **Exact Cache 是第二阶段**
  - 风险可控
  - 收益明确
  - 适合从 non-stream 开始

- **Semantic Cache 是第三阶段**
  - 风险最高
  - 需要最强观测与边界控制

同时必须承认一个现实：

> Prompt Cache 命中率不仅是请求模型问题，还是调度问题。

因此 `trace/thread/prompt_cache_key` 驱动的 affinity，不应被视为可选锦上添花，而应视为 Prompt Cache 设计的一部分。

---

## 十四、当前实施建议

当前不建议马上编码实现。

更合理的顺序是：

1. 先把主功能域继续收口
2. 保留本文作为 AI 缓存总设计基线
3. 真正开始做缓存时，从 `Phase 1: Prompt Cache` 开始
4. 完成 Prompt Cache 后，再根据真实观测决定 Exact / Semantic 的优先级

这能保证：

- 文档先行，边界清晰
- 不因过早实现而污染主线
- 后续实现时不会推翻现有日志、计费、追踪设计

