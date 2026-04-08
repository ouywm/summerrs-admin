# 05 — summer-ai 整体 DDD 架构

## 一、三 Crate 职责全景

```
summer-ai (伞 crate, 仅 re-export)
├── summer-ai-core    ─── 基础设施层的「工具箱」
│   ├── provider/     ← Provider 适配器 (OpenAI/Anthropic/Gemini/Azure)
│   ├── types/        ← OpenAI 兼容 API 类型
│   ├── stream/       ← SSE 流处理
│   └── convert/      ← 协议转换工具
│
├── summer-ai-model   ─── 共享持久化模型 + 管理 DTO
│   ├── entity/       ← SeaORM 数据库实体
│   ├── dto/          ← 管理后台 DTO
│   └── vo/           ← 视图/值对象
│
└── summer-ai-hub     ─── DDD 业务核心
    ├── domain/       ← 领域层（纯业务规则）
    ├── application/  ← 应用层（用例编排）
    ├── infrastructure/ ← 基础设施层（技术实现）
    └── interfaces/   ← 接口层（HTTP 端点）
```

## 二、各 Crate 在 DDD 中的角色

```
┌──────────────────────────────────────────────────────────────┐
│                         summer-ai-hub                        │
│  ┌─────────────────────────────────────────────────────────┐ │
│  │                    interfaces/                           │ │
│  │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  │ │
│  │  │ HTTP 管理 API │  │ OpenAI Relay │  │ Passthrough  │  │ │
│  │  │  (CRUD)      │  │ (Chat/Embed) │  │  (透传)      │  │ │
│  │  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘  │ │
│  └─────────┼─────────────────┼─────────────────┼──────────┘ │
│            │                 │                 │             │
│  ┌─────────▼─────────────────▼─────────────────▼──────────┐ │
│  │                    application/                          │ │
│  │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  │ │
│  │  │ 管理 CRUD    │  │ Relay 用例   │  │ 运维/监控    │  │ │
│  │  │ Services     │  │ (路由/计费/  │  │ Services     │  │ │
│  │  │              │  │  日志/限流)  │  │              │  │ │
│  │  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘  │ │
│  └─────────┼─────────────────┼─────────────────┼──────────┘ │
│            │                 │                 │             │
│  ┌─────────▼─────────────────▼─────────────────▼──────────┐ │
│  │                      domain/                            │ │
│  │  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌────────┐ │ │
│  │  │ Channel  │  │ Request  │  │ Billing  │  │ Alert  │ │ │
│  │  │ 聚合     │  │ 聚合     │  │ 聚合     │  │ 聚合   │ │ │
│  │  ├──────────┤  ├──────────┤  ├──────────┤  ├────────┤ │ │
│  │  │Repository│  │Repository│  │Repository│  │Repo    │ │ │
│  │  │  trait   │  │  trait   │  │  trait   │  │ trait  │ │ │
│  │  └──────────┘  └──────────┘  └──────────┘  └────────┘ │ │
│  └──────────────────────┬──────────────────────────────────┘ │
│                         │ impl trait                         │
│  ┌──────────────────────▼──────────────────────────────────┐ │
│  │                  infrastructure/                         │ │
│  │  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌────────┐ │ │
│  │  │ SeaORM   │  │ Redis    │  │ HTTP     │  │ Job    │ │ │
│  │  │ 仓储实现  │  │ 缓存     │  │ Client   │  │ 调度   │ │ │
│  │  └──────────┘  └──────────┘  └──────────┘  └────────┘ │ │
│  └─────────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────────┘
         │ uses                              │ uses
         ▼                                   ▼
┌──────────────────┐                ┌──────────────────┐
│  summer-ai-model │                │  summer-ai-core  │
│  ├── entity/     │                │  ├── provider/   │
│  ├── dto/        │                │  ├── types/      │
│  └── vo/         │                │  ├── stream/     │
│                  │                │  └── convert/    │
│ (持久化 + 管理DTO)│                │ (AI 协议适配)    │
└──────────────────┘                └──────────────────┘
```

## 三、依赖规则

### Crate 级别
```
summer-ai-hub → summer-ai-core     ✅ hub 使用 core 的 provider 和 types
summer-ai-hub → summer-ai-model    ✅ hub 使用 model 的 entity 和 dto
summer-ai-core ✗→ summer-ai-hub    ❌ core 不依赖 hub
summer-ai-core ✗→ summer-ai-model  ❌ core 不依赖 model
summer-ai-model ✗→ summer-ai-hub   ❌ model 不依赖 hub
summer-ai-model ✗→ summer-ai-core  ❌ model 不依赖 core
```

### Hub 内部模块级别
```
interfaces → application → domain ← infrastructure

domain 不依赖任何其他模块 ✅
infrastructure 实现 domain 的 trait ✅
application 组装 domain 和 infrastructure ✅
interfaces 只调用 application ✅
```

## 四、Hub 内部的两大业务线

### 业务线 1: AI Gateway (Relay)

这是核心！把上游 AI 请求路由到合适的 provider。

```
用户请求 (OpenAI 格式)
    │
    ▼
interfaces/http/openai/      ← 接收 /v1/chat/completions 等
    │
    ▼
application/relay/            ← Relay 用例编排
    │
    ├── 1. 认证 (Token 校验)
    ├── 2. 路由 (选择 Channel + Model)
    ├── 3. 限流 (Rate Limit)
    ├── 4. 计费 (配额检查)
    ├── 5. 请求转发 (调用 core adapter)
    ├── 6. 响应处理 (流/非流)
    ├── 7. 日志记录 (Request + Execution)
    └── 8. 指标采集 (Metrics)
    │
    ▼
domain/relay/                 ← 领域模型
    ├── Channel (渠道聚合根)
    ├── Token (令牌实体)
    ├── Request (请求记录聚合根)
    └── RoutingPolicy (路由策略值对象)
    │
    ▼
infrastructure/relay/         ← 基础设施
    ├── ChannelRouter (路由策略实现)
    ├── HttpClient (上游调用 via core adapter)
    ├── RateLimiter (限流实现)
    └── StreamAdapter (SSE 流转发)
```

### 业务线 2: 管理后台 (Management)

CRUD 管理 Channel、Token、配置等。

```
管理员请求 (REST API)
    │
    ▼
interfaces/http/management/   ← 管理 API 路由
    ├── channel/
    ├── config/
    ├── ops/
    └── tenant/
    │
    ▼
application/management/       ← 管理用例
    ├── channel_service
    ├── config_service
    └── tenant_service
    │
    ▼
domain/management/            ← 管理领域模型
    ├── Channel 聚合根 (含 Account, ModelPrice)
    ├── GuardrailConfig 聚合根
    ├── Token 聚合根
    └── Organization/Project 聚合根
    │
    ▼
infrastructure/management/    ← SeaORM 仓储实现
```

## 五、核心域 vs 支撑域 vs 通用域

按 DDD 子域分类：

### 核心域 (Core Domain) — 值得做完整 DDD

| 子域 | 聚合根 | 说明 |
|------|--------|------|
| **Relay 路由** | Channel, Token, RoutingPolicy | 核心业务：请求路由到合适的上游 |
| **请求追踪** | Request, RequestExecution | 核心业务：记录每次 AI 调用 |
| **计费** | Billing, UserQuota | 核心业务：费用计算和配额 |

### 支撑域 (Supporting Domain) — 简化 DDD

| 子域 | 聚合根 | 说明 |
|------|--------|------|
| **告警** | AlertRule, AlertEvent | 运维支撑：异常检测和通知 |
| **护栏** | GuardrailConfig, GuardrailRule | 安全支撑：内容审核和防护 |
| **会话** | Conversation, Message | 功能支撑：对话管理 |

### 通用域 (Generic Domain) — 简单 CRUD 即可

| 子域 | 说明 |
|------|------|
| **模型配置** | 模型列表、价格配置，基本 CRUD |
| **平台配置** | 全局设置，简单 CRUD |
| **文件存储** | 文件上传/下载管理 |
| **Vendor 管理** | 供应商信息维护 |
| **多租户** | 组织/项目管理 |

## 六、DDD 分层对齐 backup 中的旧模块

从 `_backup/2026-04-05-pre-ddd/src/` 的旧代码对齐到新 DDD 结构：

```
旧结构                              →  新 DDD 结构
========                               ===========

src/service/channel.rs               → domain/channel.rs
                                       application/channel_service.rs
                                       infrastructure/channel_repository.rs

src/service/openai_chat_relay.rs     → application/relay/chat_relay.rs
src/service/openai_relay_support.rs  → application/relay/support.rs

src/relay/channel_router.rs          → infrastructure/relay/channel_router.rs
src/relay/http_client.rs             → infrastructure/relay/http_client.rs
src/relay/stream.rs                  → infrastructure/relay/stream.rs
src/relay/billing.rs                 → infrastructure/relay/billing.rs
src/relay/rate_limit.rs              → infrastructure/relay/rate_limit.rs

src/router/openai/                   → interfaces/http/openai/
src/router/openai_passthrough/       → interfaces/http/openai_passthrough/
src/router/management/               → interfaces/http/management/

src/auth/                            → infrastructure/auth/
src/job/                             → infrastructure/job/
```
