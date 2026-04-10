# 06 — Hub DDD 模块清单与优先级

## 一、从 Backup 恢复的完整模块清单

基于 `_backup/2026-04-05-pre-ddd/src/` 的分析：

### Service 层 (旧) → 39 个模块

```
service/
├── alert.rs                    ← 告警服务
├── billing.rs                  ← 计费服务
├── channel.rs                  ← 渠道管理 (含 health_logic, tests)
├── channel_account.rs          ← 渠道账号管理
├── channel_model_price.rs      ← 模型价格管理
├── circuit_breaker.rs          ← 熔断器
├── conversation.rs             ← 会话管理
├── file_storage.rs             ← 文件存储
├── guardrail.rs                ← 护栏规则
├── log.rs                      ← 日志查询 (含 tests)
├── log_batch.rs                ← 批量日志
├── metrics.rs                  ← 指标统计
├── model.rs                    ← 模型配置
├── multi_tenant.rs             ← 多租户
├── openai_audio_multipart_relay.rs   ← Audio 多部分中继
├── openai_audio_speech_relay.rs      ← Audio 语音中继
├── openai_chat_relay.rs              ← Chat 中继 (核心)
├── openai_completions_relay.rs       ← Completions 中继
├── openai_embeddings_relay.rs        ← Embeddings 中继
├── openai_http.rs                    ← HTTP 客户端封装
├── openai_image_multipart_relay.rs   ← Image 多部分中继
├── openai_images_relay.rs            ← Image 中继
├── openai_moderations_relay.rs       ← Moderations 中继
├── openai_passthrough_relay.rs       ← 透传中继
├── openai_relay_support.rs           ← 中继共享逻辑
├── openai_rerank_relay.rs            ← Rerank 中继
├── openai_responses_relay.rs         ← Responses 中继
├── openai_responses_stream.rs        ← Responses 流处理
├── openai_tracking.rs                ← 请求追踪
├── platform_config.rs                ← 平台配置
├── request.rs                        ← 请求管理
├── resource_affinity.rs              ← 资源亲和
├── response_bridge.rs                ← 响应桥接
├── route_health.rs                   ← 路由健康检查
├── runtime.rs                        ← 运行时管理
├── runtime_cache.rs                  ← 运行时缓存
├── runtime_ops.rs                    ← 运行时操作
├── token.rs                          ← Token 管理
└── vendor.rs                         ← 供应商管理
```

### Router 层 (旧) → HTTP 端点

```
router/
├── openai/                     ← OpenAI 兼容 API
│   ├── audio.rs
│   ├── audio_transcribe.rs
│   ├── completions.rs
│   ├── files.rs
│   ├── image_multipart.rs
│   ├── images.rs
│   ├── moderations.rs
│   └── rerank.rs
├── openai_passthrough/         ← 透传 API
│   ├── assistants_threads.rs
│   ├── batches.rs
│   ├── fine_tuning.rs
│   ├── responses.rs
│   ├── resource.rs
│   ├── uploads_models.rs
│   └── vector_stores.rs
└── management/                 ← 管理后台 API
    ├── channel/
    ├── config/
    ├── ops/
    └── tenant/
```

### Relay 层 (旧) → 基础设施

```
relay/
├── billing.rs                  ← 计费中间件
├── channel_router.rs           ← 路由选择 (含 tests)
├── http_client.rs              ← 上游 HTTP 客户端
├── rate_limit.rs               ← 限流
├── routing_strategy.rs         ← 路由策略
└── stream.rs                   ← SSE 流转发
```

## 二、DDD 归类

### 核心域：Relay（AI 请求路由）

**优先级：P0 — 最先实施**

| 旧模块 | DDD 层 | 新位置 | 说明 |
|--------|--------|--------|------|
| `service/openai_chat_relay` | Application | `application/relay/chat.rs` | Chat 中继用例 |
| `service/openai_completions_relay` | Application | `application/relay/completions.rs` | Completions 中继用例 |
| `service/openai_embeddings_relay` | Application | `application/relay/embeddings.rs` | Embeddings 中继用例 |
| `service/openai_responses_relay` | Application | `application/relay/responses.rs` | Responses 中继用例 |
| `service/openai_responses_stream` | Application | `application/relay/responses_stream.rs` | Responses 流处理 |
| `service/openai_audio_*_relay` | Application | `application/relay/audio.rs` | Audio 中继用例 |
| `service/openai_images_relay` | Application | `application/relay/images.rs` | Image 中继用例 |
| `service/openai_image_multipart_relay` | Application | `application/relay/image_multipart.rs` | Image multipart |
| `service/openai_moderations_relay` | Application | `application/relay/moderations.rs` | Moderations 中继 |
| `service/openai_rerank_relay` | Application | `application/relay/rerank.rs` | Rerank 中继 |
| `service/openai_passthrough_relay` | Application | `application/relay/passthrough.rs` | 透传中继 |
| `service/openai_relay_support` | Application | `application/relay/support.rs` | 共享中继逻辑 |
| `service/openai_http` | Infrastructure | `infrastructure/relay/http_client.rs` | HTTP 客户端 |
| `service/openai_tracking` | Application | `application/relay/tracking.rs` | 请求追踪 |
| `service/response_bridge` | Application | `application/relay/response_bridge.rs` | 响应桥接 |
| `relay/channel_router` | Infrastructure | `infrastructure/relay/channel_router.rs` | 路由选择 |
| `relay/http_client` | Infrastructure | `infrastructure/relay/http_client.rs` | (合并) |
| `relay/stream` | Infrastructure | `infrastructure/relay/stream.rs` | SSE 流转发 |
| `relay/routing_strategy` | Domain | `domain/relay/routing_policy.rs` | 路由策略 |
| `relay/billing` | Infrastructure | `infrastructure/relay/billing.rs` | 计费中间件 |
| `relay/rate_limit` | Infrastructure | `infrastructure/relay/rate_limit.rs` | 限流 |
| `router/openai/*` | Interfaces | `interfaces/http/openai/*` | OpenAI API 端点 |
| `router/openai_passthrough/*` | Interfaces | `interfaces/http/passthrough/*` | 透传端点 |

**Domain 聚合根**：
```rust
// domain/relay/mod.rs
pub mod routing_policy;  // RoutingPolicy 值对象
pub mod channel;         // Channel 聚合根 (只包含路由相关属性)
pub mod token;           // Token 实体 (只包含鉴权相关属性)
pub mod request;         // Request 聚合根 (只包含追踪相关属性)
```

### 核心域：计费 (Billing)

**优先级：P0**

| 旧模块 | DDD 层 | 新位置 |
|--------|--------|--------|
| `service/billing` | Application | `application/billing.rs` |
| `relay/billing` | Infrastructure | `infrastructure/billing.rs` |

### 支撑域：渠道管理 (Channel Management)

**优先级：P1**

| 旧模块 | DDD 层 | 新位置 |
|--------|--------|--------|
| `service/channel` | Domain + Application | `domain/channel.rs` + `application/channel.rs` |
| `service/channel_account` | Application | `application/channel_account.rs` |
| `service/channel_model_price` | Application | `application/channel_model_price.rs` |
| `service/circuit_breaker` | Infrastructure | `infrastructure/circuit_breaker.rs` |
| `service/route_health` | Infrastructure | `infrastructure/route_health.rs` |
| `router/management/channel/*` | Interfaces | `interfaces/http/management/channel/*` |

### 支撑域：运维监控 (Operations)

**优先级：P1**

| 旧模块 | DDD 层 | 新位置 |
|--------|--------|--------|
| `service/alert` | Application | `application/alert.rs` |
| `service/log` | Application | `application/log.rs` |
| `service/log_batch` | Application | `application/log_batch.rs` |
| `service/metrics` | Application | `application/metrics.rs` |
| `service/request` | Application | `application/request.rs` |
| `service/runtime*` | Application | `application/runtime.rs` |
| `router/management/ops/*` | Interfaces | `interfaces/http/management/ops/*` |

### 支撑域：安全护栏 (Guardrail)

**优先级：P1 — 已有 DDD 样板 (guardrail_config)**

| 旧模块 | DDD 层 | 新位置 |
|--------|--------|--------|
| `service/guardrail` | Domain + Application | `domain/guardrail.rs` + `application/guardrail.rs` |
| `router/management/config/guardrail` | Interfaces | `interfaces/http/management/guardrail.rs` |

### 通用域：配置管理 (Configuration)

**优先级：P2 — 简单 CRUD，不需要完整 DDD**

| 旧模块 | DDD 层 | 新位置 |
|--------|--------|--------|
| `service/model` | Application | `application/model_config.rs` |
| `service/platform_config` | Application | `application/platform_config.rs` |
| `service/vendor` | Application | `application/vendor.rs` |
| `service/file_storage` | Application | `application/file_storage.rs` |

### 通用域：租户管理 (Tenant)

**优先级：P2**

| 旧模块 | DDD 层 | 新位置 |
|--------|--------|--------|
| `service/multi_tenant` | Application | `application/multi_tenant.rs` |
| `service/token` | Application | `application/token.rs` |
| `service/conversation` | Application | `application/conversation.rs` |

### 基础设施：认证与作业

| 旧模块 | DDD 层 | 新位置 |
|--------|--------|--------|
| `auth/*` | Infrastructure | `infrastructure/auth/*` |
| `job/*` | Infrastructure | `infrastructure/job/*` |

## 三、目标 Hub 目录结构

```
hub/src/
├── lib.rs
├── plugin.rs
│
├── domain/
│   ├── mod.rs
│   ├── relay/                      ← P0 核心域
│   │   ├── mod.rs
│   │   ├── routing_policy.rs       # 路由策略值对象
│   │   ├── channel.rs              # Channel 聚合根（路由视角）
│   │   ├── token.rs                # Token 实体（鉴权视角）
│   │   └── request.rs              # Request 聚合根（追踪视角）
│   ├── billing/                    ← P0 核心域
│   │   ├── mod.rs
│   │   └── quota.rs                # UserQuota 聚合根
│   ├── channel/                    ← P1 支撑域
│   │   ├── mod.rs
│   │   └── channel.rs              # Channel 聚合根（管理视角）
│   ├── guardrail/                  ← P1 支撑域
│   │   ├── mod.rs
│   │   ├── config.rs               # GuardrailConfig 聚合根
│   │   └── rule.rs                 # GuardrailRule 实体
│   └── alert/                      ← P1 支撑域
│       ├── mod.rs
│       └── rule.rs                 # AlertRule 聚合根
│
├── application/
│   ├── mod.rs
│   ├── relay/                      ← P0
│   │   ├── mod.rs
│   │   ├── chat.rs                 # Chat 中继用例
│   │   ├── completions.rs
│   │   ├── embeddings.rs
│   │   ├── responses.rs
│   │   ├── audio.rs
│   │   ├── images.rs
│   │   ├── moderations.rs
│   │   ├── rerank.rs
│   │   ├── passthrough.rs
│   │   ├── support.rs              # 共享中继逻辑
│   │   ├── tracking.rs             # 请求追踪
│   │   └── response_bridge.rs      # 响应桥接
│   ├── billing.rs                  ← P0
│   ├── channel.rs                  ← P1
│   ├── channel_account.rs          ← P1
│   ├── channel_model_price.rs      ← P1
│   ├── guardrail.rs                ← P1 (已有样板)
│   ├── alert.rs                    ← P1
│   ├── log.rs                      ← P1
│   ├── metrics.rs                  ← P1
│   ├── runtime.rs                  ← P1
│   ├── model_config.rs             ← P2
│   ├── platform_config.rs          ← P2
│   ├── vendor.rs                   ← P2
│   ├── file_storage.rs             ← P2
│   ├── multi_tenant.rs             ← P2
│   ├── token.rs                    ← P2
│   └── conversation.rs             ← P2
│
├── infrastructure/
│   ├── mod.rs
│   ├── auth/                       ← P0
│   │   ├── mod.rs
│   │   ├── middleware.rs
│   │   └── extractor.rs
│   ├── relay/                      ← P0
│   │   ├── mod.rs
│   │   ├── channel_router.rs       # 路由选择
│   │   ├── http_client.rs          # 上游 HTTP
│   │   ├── stream.rs               # SSE 流转发
│   │   ├── rate_limit.rs           # 限流
│   │   └── billing.rs              # 计费中间件
│   ├── repository/                 ← P1
│   │   ├── mod.rs
│   │   ├── channel.rs              # SeaORM Channel 仓储
│   │   ├── guardrail_config.rs     # (已有)
│   │   └── ...
│   ├── job/                        ← P1
│   │   ├── mod.rs
│   │   └── channel_recovery.rs
│   └── cache/                      ← P1
│       ├── mod.rs
│       └── runtime_cache.rs
│
├── interfaces/
│   ├── mod.rs
│   └── http/
│       ├── mod.rs
│       ├── openai/                 ← P0
│       │   ├── mod.rs
│       │   ├── chat.rs             # POST /v1/chat/completions
│       │   ├── completions.rs
│       │   ├── embeddings.rs       # POST /v1/embeddings
│       │   ├── responses.rs        # POST /v1/responses
│       │   ├── audio.rs
│       │   ├── images.rs
│       │   ├── moderations.rs
│       │   ├── rerank.rs
│       │   └── files.rs
│       ├── passthrough/            ← P0
│       │   ├── mod.rs
│       │   ├── assistants_threads.rs
│       │   ├── batches.rs
│       │   ├── fine_tuning.rs
│       │   ├── responses.rs
│       │   ├── uploads_models.rs
│       │   └── vector_stores.rs
│       ├── management/             ← P1-P2
│       │   ├── mod.rs
│       │   ├── channel/
│       │   ├── config/
│       │   ├── ops/
│       │   └── tenant/
│       └── guardrail_config.rs     ← 已有 DDD 样板
│
└── tests/                          ← 集成测试
    ├── mod.rs
    └── ...
```

## 四、实施优先级总结

| 优先级 | 域 | 模块数 | 复杂度 | 说明 |
|--------|-----|--------|--------|------|
| **P0** | Relay + Billing + Auth | ~25 | 高 | 核心业务，必须最先完成 |
| **P1** | Channel + Alert + Guardrail + Ops | ~15 | 中 | 支撑功能，可逐步迁移 |
| **P2** | Config + Tenant + 其他 | ~10 | 低 | 简单 CRUD，最后迁移 |
