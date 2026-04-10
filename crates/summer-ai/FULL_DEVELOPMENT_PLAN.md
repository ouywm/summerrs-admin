# summer-ai 全量业务开发蓝图 v2

更新日期：2026-04-03

---

## 一、当前进展与全局视图

### 1.1 已完成

| 模块 | 完成度 | 说明 |
|------|--------|------|
| 核心中转链路 | ✅ 100% | Token 鉴权 → 路由选择 → 请求转发 → 计费结算 → 日志记录 |
| Provider 适配 | ✅ 高 | OpenAI/Anthropic/Gemini/Azure + 12 家 OpenAI 兼容厂商 |
| 计费引擎 | ✅ | 三阶段原子计费 (Pending→Reserved→Settled)、退款、分组倍率 |
| 限流引擎 | ✅ | RPM/TPM/并发、Lua 原子操作、Redis 降级 |
| 路由系统 | ✅ | 优先级+权重+健康感知、批量加载、熔断器、策略框架 |
| 流式处理 | ✅ | SSE 解析（UTF-8 安全）、心跳保活、灵活结算 |
| 指标系统 | ✅ | 原子计数器、`/ai/runtime/metrics` API |
| Redis 降级 | ✅ | 分层降级、fail-close 鉴权 |

### 1.2 数据库表全景

**共 77 张 SQL 表**，已实现 Entity 8 张，剩余 69 张待开发。

```
功能域              表数   Entity   说明
────────────────────────────────────────────────────
核心中转             8      8/8     ✅ 完成
运营追踪             3      0/3     request/execution/trace
商业化计费           11     0/11    order/payment/subscription/price
多租户与权限         15     0/15    org/team/project/rbac/sso
内容治理             7      0/7     guardrail/prompt_protection
观测与告警           8      0/8     alert/trace_span/daily_stats
应用链路             5      0/5     conversation/message/thread
存储与资源           6      0/6     file/vector_store
平台配置             4      0/4     vendor/config/routing_rule/plugin
任务与可靠性         6      0/6     task/outbox/retry/idempotency
审计与财务           2      0/2     audit_log/preconsume_record
其他                 2      0/2     domain_verification/error_passthrough
```

---

## 二、功能域详细规划

### Phase A：运营追踪层 (Week 1-2)

**目标**：补全请求链路追踪，支撑排障和审计。

| 表 | 核心字段 | 用途 | 依赖 |
|----|---------|------|------|
| `request` | request_id, user_id, requested_model, status, elapsed_time | 客户端请求快照（比 log 更完整） | log |
| `request_execution` | request_id, execution_id, channel_id, attempt_seq, status_code | 每次上游尝试记录（含重试） | request, channel |
| `retry_attempt` | execution_id, attempt_no, error_code, latency_ms | 重试详情 | request_execution |

**开发内容**：
1. 生成 Entity (sea-orm-cli generate)
2. 在 `settle_usage_accounting` 和 `record_terminal_failure` 中写入 request/execution
3. 管理端查询 API：`GET /ai/request/{id}` + `GET /ai/request/{id}/executions`

**参考**：one-api 的 `/api/log/self` 接口，但我们的 request_execution 粒度更细（每次重试独立记录）。

---

### Phase B：渠道模型定价 (Week 2-3)

**目标**：从硬编码 model_config 升级为动态多版本定价。

| 表 | 核心字段 | 用途 |
|----|---------|------|
| `vendor` | code, name, logo_url, website, supported_scopes | 供应商元数据（UI 展示用） |
| `channel_model_price` | channel_id, model_name, price_config (JSONB), effective_at | 当前生效的采购价 |
| `channel_model_price_version` | price_id, version, price_config, created_at | 价格变更历史 |

**开发内容**：
1. `BillingEngine` 查价逻辑从 model_config 扩展，优先取 channel_model_price
2. 管理端 CRUD：渠道下挂载价格配置
3. 定价变更审计（自动创建 version 记录）

**参考**：one-api 的 `模型倍率 × 渠道倍率` 体系 + new-api 的按次计费。

---

### Phase C：观测与告警 (Week 3-5)

**目标**：从"有日志"升级到"可监控可告警"。

| 表 | 用途 |
|----|------|
| `daily_stats` | 从 log 聚合的日度统计（后台 Job） |
| `trace` + `trace_span` | OpenTelemetry 兼容的链路追踪存储 |
| `alert_rule` | 告警规则（如：渠道成功率 < 90%、配额 < 10%） |
| `alert_event` | 已触发的告警事件 |
| `alert_silence` | 告警沉默/抑制规则 |

**开发内容**：
1. `DailyStatsJob`：每日凌晨从 log 聚合写入 daily_stats
2. `AlertEngine`：定时扫描规则，匹配条件时创建 event
3. 告警通知：webhook / 邮件 / 企业微信（通过 summer-plugins 扩展）
4. 管理端 API：规则 CRUD + 事件查询 + 沉默管理

**参考**：one-hub 的 Telegram Bot 告警 + Prometheus 集成。

---

### Phase D：内容治理与合规 (Week 5-7)

**目标**：输入/输出安全过滤，合规审计。

| 表 | 用途 |
|----|------|
| `guardrail_config` | 治理配置容器（绑定到 org/project/token） |
| `guardrail_rule` | 过滤规则（phase: request_input/response_output/file_upload） |
| `prompt_protection_rule` | 提示词注入防护规则 |
| `guardrail_violation` | 违规审计记录 |
| `guardrail_metric_daily` | 违规日报统计 |

**开发内容**：
1. `GuardrailMiddleware`：请求前/响应后过滤（接入阶段 8 的 RelayMiddleware trait）
2. 规则引擎：正则匹配 / 关键词 / Aho-Corasick 多模式匹配
3. PII 脱敏：识别并替换敏感信息（手机号、身份证、邮箱等）
4. 管理端 API：规则 CRUD + 违规查询 + 日报

**参考**：portkey-gateway 的 40+ Guardrails + lunaroute 的 PII 脱敏 (Aho-Corasick)。

---

### Phase E：应用链路 — 对话与会话 (Week 7-9)

**目标**：支持 Assistants API / Threads / 对话历史。

| 表 | 用途 |
|----|------|
| `conversation` | 对话主表（title, message_count, metadata） |
| `message` | 消息详情（role, content, tool_calls, token_count） |
| `session` | 会话生命周期管理 |
| `thread` | Assistants API 线程 |
| `prompt_template` | 提示词模板库（变量替换） |

**开发内容**：
1. 对话 CRUD API（兼容 OpenAI Assistants API 格式）
2. 消息存储与检索
3. 提示词模板管理 + 变量注入
4. Thread → Channel 资源亲和（已有 ResourceAffinityService）

**参考**：crewAI 的多智能体对话编排 + solon-ai 的 ReAct 模式。

---

### Phase F：文件与向量存储 (Week 9-11)

**目标**：完整的 `/v1/files` + `/v1/vector_stores` API。

| 表 | 用途 |
|----|------|
| `file` | 上传文件元数据（S3 对象键、大小、类型） |
| `vector_store` | 向量存储库（embeddings 索引） |
| `vector_store_file` | 向量库内的文件关联 |
| `managed_object` | 通用对象管理 |
| `data_storage` | 数据留存策略 |
| `usage_cleanup_task` | 过期数据清理任务 |

**开发内容**：
1. `/v1/files` CRUD（上传走 S3 via summer-plugins/s3）
2. `/v1/vector_stores` CRUD
3. 文件 → Channel 资源亲和路由
4. 定时清理过期文件 Job

---

### Phase G：多租户与访问控制 (Week 11-14)

**目标**：企业级多租户隔离与权限管理。

| 子域 | 表 | 用途 |
|------|-----|------|
| **组织体系** | org, team, project, *_membership | 三级层次：组织→团队→项目 |
| **身份认证** | service_account, invitation, sso_config, scim_config | 服务账号 + SSO/SCIM |
| **权限管理** | rbac_policy, rbac_policy_version | 策略版本化 RBAC |
| **审计** | audit_log | 控制面操作审计 |

**开发内容**：
1. 组织 CRUD + 成员管理 API
2. 项目级资源隔离（token/channel/quota 绑定到 project）
3. RBAC 策略引擎（CEL 表达式或 JSON 条件）
4. SSO 集成（OIDC Provider）
5. 审计日志记录（所有管理操作自动记录 change_set）

**参考**：hadrian 的 CEL 表达式 RBAC + APIPark 的企业级审批流。

---

### Phase H：商业化计费 (Week 14-17)

**目标**：从"内部配额系统"升级为可运营的计费体系。

| 子域 | 表 | 用途 |
|------|-----|------|
| **订单** | order, transaction | 统一订单 + 账务明细 |
| **支付** | payment_method, topup | 支付方式 + 充值 |
| **订阅** | subscription_plan, user_subscription | 套餐 + 用户订阅 |
| **优惠** | discount, redemption, referral | 折扣券/兑换码/推荐返利 |
| **去重** | usage_billing_dedup | 计费幂等 |

**开发内容**：
1. 套餐管理 API（创建/修改/上下架）
2. 订单生命周期（创建→支付→完成/退款）
3. 充值 + 支付集成（支付宝/微信 via 回调）
4. 兑换码核销
5. 推荐返利结算

**参考**：one-api 的 topup 体系 + new-api 的 Stripe/支付宝集成。

---

### Phase I：平台配置与扩展 (Week 17-18)

| 表 | 用途 |
|----|------|
| `config_entry` | 全局配置键值对（运行时热更新） |
| `routing_rule` + `routing_target` | 高级路由规则（基于 header/model/user 的条件路由） |
| `plugin` + `plugin_binding` | 插件注册与绑定 |
| `error_passthrough_rule` | 错误透传规则（哪些上游错误直接返回客户端） |

---

### Phase J：可靠性基础设施 (持续)

| 表 | 用途 |
|----|------|
| `task` | 异步任务队列（替代裸 tokio::spawn） |
| `scheduler_outbox` | Outbox Pattern 可靠消息 |
| `dead_letter_queue` | 最终失败请求队列 |
| `idempotency_record` | 幂等键记录 |
| `channel_probe` | 渠道探针结果记录 |
| `domain_verification` | 域名验证 |

---

## 三、实施优先级总表

```
优先级    阶段              周期        核心交付物                      前置条件
──────────────────────────────────────────────────────────────────────────────
P0       Phase A 运营追踪   Week 1-2    request/execution 链路追踪      无
P0       Phase B 模型定价   Week 2-3    动态定价 + 价格版本管理         无
P1       Phase C 观测告警   Week 3-5    daily_stats + 告警引擎          Phase A
P1       Phase D 内容治理   Week 5-7    Guardrail 中间件 + PII 脱敏     无
P1       Phase E 对话链路   Week 7-9    Conversations + Threads API     无
P2       Phase F 文件存储   Week 9-11   /v1/files + /v1/vector_stores   Phase E
P2       Phase G 多租户     Week 11-14  org/project/RBAC/SSO            无
P2       Phase H 商业化     Week 14-17  订单/支付/订阅/兑换码           Phase G
P3       Phase I 平台配置   Week 17-18  热更新配置 + 高级路由规则       无
持续     Phase J 可靠性     全程        Outbox/DLQ/幂等                 无
```

---

## 四、每个 Phase 的 Entity 生成清单

### Phase A (3 Entity)
```bash
# 需要新建的 Entity 文件
model/src/entity/_entity/request.rs
model/src/entity/_entity/request_execution.rs
model/src/entity/_entity/retry_attempt.rs
```

### Phase B (3 Entity)
```bash
model/src/entity/_entity/vendor.rs
model/src/entity/_entity/channel_model_price.rs
model/src/entity/_entity/channel_model_price_version.rs
```

### Phase C (5 Entity)
```bash
model/src/entity/_entity/daily_stats.rs
model/src/entity/_entity/trace.rs
model/src/entity/_entity/trace_span.rs
model/src/entity/_entity/alert_rule.rs
model/src/entity/_entity/alert_event.rs
model/src/entity/_entity/alert_silence.rs
```

### Phase D (5 Entity)
```bash
model/src/entity/_entity/guardrail_config.rs
model/src/entity/_entity/guardrail_rule.rs
model/src/entity/_entity/prompt_protection_rule.rs
model/src/entity/_entity/guardrail_violation.rs
model/src/entity/_entity/guardrail_metric_daily.rs
```

### Phase E (5 Entity)
```bash
model/src/entity/_entity/conversation.rs
model/src/entity/_entity/message.rs
model/src/entity/_entity/session.rs
model/src/entity/_entity/thread.rs
model/src/entity/_entity/prompt_template.rs
```

### Phase F (6 Entity)
```bash
model/src/entity/_entity/file.rs
model/src/entity/_entity/vector_store.rs
model/src/entity/_entity/vector_store_file.rs
model/src/entity/_entity/managed_object.rs
model/src/entity/_entity/data_storage.rs
model/src/entity/_entity/usage_cleanup_task.rs
```

### Phase G (12 Entity)
```bash
model/src/entity/_entity/organization.rs
model/src/entity/_entity/team.rs
model/src/entity/_entity/project.rs
model/src/entity/_entity/org_membership.rs
model/src/entity/_entity/team_membership.rs
model/src/entity/_entity/project_membership.rs
model/src/entity/_entity/service_account.rs
model/src/entity/_entity/invitation.rs
model/src/entity/_entity/org_sso_config.rs
model/src/entity/_entity/org_scim_config.rs
model/src/entity/_entity/rbac_policy.rs
model/src/entity/_entity/rbac_policy_version.rs
model/src/entity/_entity/audit_log.rs
```

### Phase H (8 Entity)
```bash
model/src/entity/_entity/order.rs
model/src/entity/_entity/payment_method.rs
model/src/entity/_entity/transaction.rs
model/src/entity/_entity/subscription_plan.rs
model/src/entity/_entity/user_subscription.rs
model/src/entity/_entity/discount.rs
model/src/entity/_entity/redemption.rs
model/src/entity/_entity/referral.rs
model/src/entity/_entity/topup.rs
model/src/entity/_entity/usage_billing_dedup.rs
model/src/entity/_entity/subscription_preconsume_record.rs
```

### Phase I + J (10 Entity)
```bash
model/src/entity/_entity/config_entry.rs
model/src/entity/_entity/routing_rule.rs
model/src/entity/_entity/routing_target.rs
model/src/entity/_entity/plugin.rs
model/src/entity/_entity/plugin_binding.rs
model/src/entity/_entity/error_passthrough_rule.rs
model/src/entity/_entity/task.rs
model/src/entity/_entity/scheduler_outbox.rs
model/src/entity/_entity/dead_letter_queue.rs
model/src/entity/_entity/idempotency_record.rs
model/src/entity/_entity/channel_probe.rs
model/src/entity/_entity/domain_verification.rs
```

---

## 五、参考项目功能对照

| 功能 | one-api | new-api | hadrian | litellm | summer-ai |
|------|---------|---------|---------|---------|-----------|
| 核心中转 | ✅ | ✅ | ✅ | ✅ | ✅ |
| Provider 数量 | 40+ | 50+ | 5-10 | 100+ | 16 (可扩展) |
| 计费体系 | 倍率制 | 倍率+按次 | 配额 | Pass-through | 倍率+分组 ✅ |
| 用户系统 | ✅ | ✅ | SSO | — | ❌ Phase G |
| 支付集成 | WePay | Stripe+Ali | — | — | ❌ Phase H |
| 多租户 | 简单 | 简单 | 企业级 | — | ❌ Phase G |
| RBAC | 基础 | 基础 | CEL | — | ❌ Phase G |
| 告警 | ❌ | Telegram | ❌ | Slack | ❌ Phase C |
| Guardrails | ❌ | ❌ | Feature Flag | ❌ | ❌ Phase D |
| 对话历史 | ❌ | ❌ | ❌ | ❌ | ❌ Phase E |
| 文件/向量 | ❌ | ❌ | ❌ | ❌ | ❌ Phase F |
| Prometheus | ❌ | ✅ | ❌ | ✅ | ✅ 基础 |
| WebSocket | ❌ | ❌ | ❌ | ❌ | ❌ 未来 |
| 熔断器 | ❌ | ❌ | ❌ | ❌ | ✅ |
| 路由策略 | 固定 | 固定 | 固定 | 多策略 | ✅ 多策略 |

---

## 六、技术决策记录

| 决策 | 选择 | 理由 |
|------|------|------|
| ORM | SeaORM | 已采用，生态成熟 |
| 缓存 | Redis + 内存降级 | 已实现分层降级 |
| 文件存储 | S3 via summer-plugins | 已有 S3 插件 |
| 异步任务 | tokio::spawn + outbox | 短期 spawn，长期 outbox |
| 搜索 | PostgreSQL full-text | 不引入 ES，减少依赖 |
| 向量 | pgvector | PostgreSQL 原生扩展 |
| 告警通知 | Webhook | 通用性最好 |
| 权限 | RBAC + JSON 条件 | 不引入 CEL，降低复杂度 |
| PII 脱敏 | Aho-Corasick | 参考 lunaroute，高性能 |
