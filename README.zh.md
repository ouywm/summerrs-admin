<div align="center">

<img src="docs/static/logo.png" alt="Summerrs Admin Logo" width="200"/>

# Summerrs Admin

**中文** | [English](README.md)

> 全栈 Rust 后台管理系统 · 内置 LLM 中转网关、数据库分片、多租户隔离、MCP 服务、声明式宏

[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.93%2B-orange.svg?logo=rust&logoColor=white)](https://www.rust-lang.org)
[![Edition](https://img.shields.io/badge/edition-2024-orange.svg)](https://doc.rust-lang.org/edition-guide/rust-2024/index.html)
[![GitHub stars](https://img.shields.io/github/stars/ouywm/summerrs-admin?style=flat&color=yellow&logo=github)](https://github.com/ouywm/summerrs-admin/stargazers)
[![zread](https://img.shields.io/badge/Ask_Zread-_.svg?style=flat&color=00b0aa&labelColor=000000&logo=data%3Aimage%2Fsvg%2Bxml%3Bbase64%2CPHN2ZyB3aWR0aD0iMTYiIGhlaWdodD0iMTYiIHZpZXdCb3g9IjAgMCAxNiAxNiIgZmlsbD0ibm9uZSIgeG1sbnM9Imh0dHA6Ly93d3cudzMub3JnLzIwMDAvc3ZnIj4KPHBhdGggZD0iTTQuOTYxNTYgMS42MDAxSDIuMjQxNTZDMS44ODgxIDEuNjAwMSAxLjYwMTU2IDEuODg2NjQgMS42MDE1NiAyLjI0MDFWNC45NjAxQzEuNjAxNTYgNS4zMTM1NiAxLjg4ODEgNS42MDAxIDIuMjQxNTYgNS42MDAxSDQuOTYxNTZDNS4zMTUwMiA1LjYwMDEgNS42MDE1NiA1LjMxMzU2IDUuNjAxNTYgNC45NjAxVjIuMjQwMUM1LjYwMTU2IDEuODg2NjQgNS4zMTUwMiAxLjYwMDEgNC45NjE1NiAxLjYwMDFaIiBmaWxsPSIjZmZmIi8%2BCjxwYXRoIGQ9Ik00Ljk2MTU2IDEwLjM5OTlIMi4yNDE1NkMxLjg4ODEgMTAuMzk5OSAxLjYwMTU2IDEwLjY4NjQgMS42MDE1NiAxMS4wMzk5VjEzLjc1OTlDMS42MDE1NiAxNC4xMTM0IDEuODg4MSAxNC4zOTk5IDIuMjQxNTYgMTQuMzk5OUg0Ljk2MTU2QzUuMzE1MDIgMTQuMzk5OSA1LjYwMTU2IDE0LjExMzQgNS42MDE1NiAxMy43NTk5VjExLjAzOTlDNS42MDE1NiAxMC42ODY0IDUuMzE1MDIgMTAuMzk5OSA0Ljk2MTU2IDEwLjM5OTlaIiBmaWxsPSIjZmZmIi8%2BCjxwYXRoIGQ9Ik0xMy43NTg0IDEuNjAwMUgxMS4wMzg0QzEwLjY4NSAxLjYwMDEgMTAuMzk4NCAxLjg4NjY0IDEwLjM5ODQgMi4yNDAxVjQuOTYwMUMxMC4zOTg0IDUuMzEzNTYgMTAuNjg1IDUuNjAwMSAxMS4wMzg0IDUuNjAwMUgxMy43NTg0QzE0LjExMTkgNS42MDAxIDE0LjM5ODQgNS4zMTM1NiAxNC4zOTg0IDQuOTYwMVYyLjI0MDFDMTQuMzk4NCAxLjg4NjY0IDE0LjExMTkgMS42MDAxIDEzLjc1ODQgMS42MDAxWiIgZmlsbD0iI2ZmZiIvPgo8cGF0aCBkPSJNNCAxMkwxMiA0TDQgMTJaIiBmaWxsPSIjZmZmIi8%2BCjxwYXRoIGQ9Ik00IDEyTDEyIDQiIHN0cm9rZT0iI2ZmZiIgc3Ryb2tlLXdpZHRoPSIxLjUiIHN0cm9rZS1saW5lY2FwPSJyb3VuZCIvPgo8L3N2Zz4K&logoColor=ffffff)](https://zread.ai/ouywm/summerrs-admin)

[核心能力](#核心能力) · [架构概览](#架构概览) · [项目结构](#项目结构)

</div>

---

## 项目定位

`summerrs-admin` 是一套**完全用 Rust 写**的生产级后台管理系统，构建在 [Summer 框架](https://github.com/ouywm/spring-rs)（一个 Spring 风格的 Rust 应用骨架）之上。它把通常需要一整支后端团队才能拼齐的能力——身份鉴权、多租户、AI 网关、消息推送、对象存储、声明式审计——以**插件组合**的形式集成到一个二进制中，开箱即用，按需启用。

它不是一个 demo，也不是某个独立组件的展示——它是一个**完整、自洽、可部署**的后台底座。

---

## 与同类项目的差异

市面上的后台框架要么是**业务后台（CRUD 脚手架）**，要么是**AI 网关**，要么是**分片中间件**，但很少把这些能力放在同一个工程里。`summerrs-admin` 把四件事拧到了一起：

| 能力 | 通常情况 | 本项目 |
|---|---|---|
| **LLM 中转网关** | 单独一个项目（new-api、one-api、AxonHub） | 内嵌为 `summer-ai` crate，跟后台共用鉴权、计费、审计 |
| **数据库分片** | 接 ShardingSphere/Vitess 等独立中间件 | `summer-sharding` 在 SQL 层透明改写，无需改业务代码 |
| **MCP 服务** | 写一个独立的 MCP server 进程 | `summer-mcp` 直接和业务 schema 联动，AI 助手可生成 CRUD |
| **声明式审计与限流** | 中间件 + 手写代码 | `#[login]` `#[has_perm]` `#[rate_limit]` 单行属性搞定 |

不是每个项目都需要全部这些能力，但当你需要其中任意两个时，把它们装在同一个进程里**省一整层运维**。

---

## 架构概览

系统以**插件组合**为核心模式。`crates/app/src/main.rs` 是组装入口，把 17 个插件依次塞进 `App::new()`：

```
                    HTTP 8080
                       │
                       ▼
        ┌──────────────────────────────────┐
        │  Tower 中间件（CORS / 压缩 /     │
        │  panic 兜底 / 客户端 IP 提取）   │
        └──────────────┬───────────────────┘
                       │
        ┌──────────────┼──────────────────┐
        ▼              ▼                  ▼
   /api/* (JWT)    /v1/*  (API key)   default
   summer-system  summer-ai-relay     handler
   summer-ai-admin (OpenAI/Claude/    auto-grouped
                   Gemini 入口)
                       │
                       ▼
        ┌──────────────────────────────────┐
        │  声明式宏层                      │
        │  #[login] #[has_perm]            │
        │  #[has_role] #[rate_limit]       │
        │  #[operation_log]                │
        └──────────────┬───────────────────┘
                       │
                       ▼
        ┌──────────────────────────────────┐
        │  分片 / SQL 改写中间件           │
        │  租户上下文注入 / 加密 / 脱敏    │
        └──────┬─────────────┬─────────────┘
               ▼             ▼
          PostgreSQL 17    Redis 7
          (主存储)         (会话 / 缓存 / 限流)
                                │
                                ▼
                    Socket.IO / 后台任务 / S3
```

**插件清单（17 个）**：
`WebPlugin` · `SeaOrmPlugin` · `RedisPlugin` · `SummerShardingPlugin` · `SummerSqlRewritePlugin` · `JobPlugin` · `MailPlugin` · `SummerAuthPlugin` · `PermBitmapPlugin` · `SocketGatewayPlugin` · `Ip2RegionPlugin` · `S3Plugin` · `BackgroundTaskPlugin` · `LogBatchCollectorPlugin` · `McpPlugin` · `SummerAiRelayPlugin` · `SummerAiBillingPlugin`

---

## 核心能力

### 身份验证与授权
- **多算法 JWT** —— HS256 / RS256 / ES256 / EdDSA，支持密钥轮转
- **位图 RBAC** —— 权限按位运算，O(1) 检查
- **声明式宏** —— `#[login]` `#[has_perm("user:create")]` `#[has_role("admin")]` `#[public]`
- **会话治理** —— 并发登录控制、设备数限制、令牌刷新、强制下线

### 多租户与数据库
- **四级隔离**

  | 模式 | 适用 | 实现 |
  |---|---|---|
  | `shared_row` | 多数 SaaS | SQL 改写自动加 `tenant_id` 过滤 |
  | `separate_table` | 中等隔离需求 | `user_001` / `user_002` 物理分表 |
  | `separate_schema` | 强隔离 | PostgreSQL schema 隔离 |
  | `separate_database` | 完全独立 | 每个租户独立物理库 |

- **SQL 改写引擎** —— 透明注入租户上下文，业务代码无感
- **CDC 管道** —— 跨租户变更捕获
- **加密 / 脱敏 / 审计** —— 内置于分片层，落库前完成

### AI 网关（summer-ai）
- **三大入口协议**

  | 协议 | 路径 | 适配 |
  |---|---|---|
  | OpenAI | `/v1/chat/completions` `/v1/responses` `/v1/models` | 原生兼容 |
  | Claude | `/v1/messages` | 原生兼容 |
  | Gemini | `/v1beta/models/{target}` | 原生兼容 |

- **40+ 上游供应商** —— 用 ZST（零大小类型）适配器实现，零运行时开销
- **6 维动态路由** —— 协议家族 / Endpoint / 凭证 / 模型映射 / 额外 headers / 路由策略
- **三阶段计费** —— Reserve（预扣）→ Settle（结算）→ Refund（退款），原子操作
- **自动故障转移** —— 失败时按优先级重试其它渠道（流式不重试）
- **热更新** —— 配置在数据库里，无需重启
- **完整追踪** —— 全生命周期日志，含每次重试

### MCP 服务器集成
- **结构发现** —— AI 助手能查询数据库 schema
- **代码生成** —— 通过对话生成 CRUD 模块
- **菜单 / 字典自动部署** —— 提示词驱动落库
- **底层** —— 基于 [rmcp](https://github.com/modelcontextprotocol/rust-sdk)（Rust MCP 官方 SDK），支持 stdio 与 streamable-http 双传输

### 实时与后台处理
- **Socket.IO** —— 实时双向通信，会话状态走 Redis（多实例可水平扩展）
- **后台任务队列** —— 类型化任务、4 worker 默认、容量 4096
- **批量日志** —— 操作日志异步批写，主链路无阻塞
- **定时任务** —— `tokio-cron-scheduler` 驱动

### 存储与工具
- **S3 兼容存储** —— AWS S3 / MinIO / RustFS，分片上传支持 5GB 文件
- **IP 地理定位** —— IP2Region xdb 内嵌，登录日志自动归属
- **国际化** —— 编译期 i18n，目前中英文
- **限流** —— 5 种算法：固定窗口 / 滑动窗口 / 令牌桶 / 漏桶 / Lua 脚本
- **OpenAPI 文档** —— 路径 `/docs`，含 Swagger UI

---

## 项目结构

```
summerrs-admin/
├── crates/
│   ├── app/                          # 二进制入口，组装所有插件
│   ├── summer-admin-macros/          # 声明式宏（#[login] / #[has_perm] 等）
│   ├── summer-auth/                  # JWT 鉴权 + 路径策略
│   ├── summer-common/                # 通用类型与工具
│   ├── summer-domain/                # 领域模型（实体 / VO）
│   ├── summer-ai/                    # AI 网关（中转 + 计费 + 管理）
│   │   ├── core/                     # 协议核心
│   │   ├── model/                    # 数据模型
│   │   ├── relay/                    # 中转引擎
│   │   ├── admin/                    # 后台 API
│   │   └── billing/                  # 计费与结算
│   ├── summer-sharding/              # 分片 / 多租户中间件
│   ├── summer-sql-rewrite/           # SQL 改写引擎
│   ├── summer-mcp/                   # MCP 服务器
│   ├── summer-plugins/               # S3 / IP2Region / 后台任务等插件
│   └── summer-system/                # 系统业务（RBAC / 用户 / 菜单 / Socket.IO）
│       └── model/
├── config/                           # 多环境配置（dev / prod / test）
├── sql/                              # 数据库 source of truth
│   ├── sys/                          # 系统域（用户 / 菜单 / 权限 / 日志）
│   ├── tenant/                       # 租户控制面
│   ├── biz/                          # B/C 端业务
│   ├── ai/                           # AI 网关 schema
│   └── migration/                    # 一次性迁移脚本
├── doc/                              # 部署 / 迁移 / 技术指南
├── docs/                             # 调研、研究、参考资料
├── locales/                          # i18n 资源
├── build-tools/                      # fmt / clippy / pre-commit 脚本
├── docker-compose.yml                # 一键启动 postgres + redis + rustfs + app
└── Dockerfile                        # 多阶段构建
```

---

<div align="center">

如果这个项目对你有帮助，欢迎 Star 支持。

[报告问题](https://github.com/ouywm/summerrs-admin/issues) · [发起讨论](https://github.com/ouywm/summerrs-admin/discussions)

</div>
