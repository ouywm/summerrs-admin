<div align="center">

<img src="docs/static/logo.png" alt="Summerrs Admin Logo" width="200"/>

# Summerrs Admin

**中文** | [English](README.md)

[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.85%2B-orange.svg)](https://www.rust-lang.org)
[![Edition](https://img.shields.io/badge/edition-2024-orange.svg)](https://doc.rust-lang.org/edition-guide/rust-2024/index.html)
[![zread](https://img.shields.io/badge/Ask_Zread-_.svg?style=flat&color=00b0aa&labelColor=000000&logo=data%3Aimage%2Fsvg%2Bxml%3Bbase64%2CPHN2ZyB3aWR0aD0iMTYiIGhlaWdodD0iMTYiIHZpZXdCb3g9IjAgMCAxNiAxNiIgZmlsbD0ibm9uZSIgeG1sbnM9Imh0dHA6Ly93d3cudzMub3JnLzIwMDAvc3ZnIj4KPHBhdGggZD0iTTQuOTYxNTYgMS42MDAxSDIuMjQxNTZDMS44ODgxIDEuNjAwMSAxLjYwMTU2IDEuODg2NjQgMS42MDE1NiAyLjI0MDFWNC45NjAxQzEuNjAxNTYgNS4zMTM1NiAxLjg4ODEgNS42MDAxIDIuMjQxNTYgNS42MDAxSDQuOTYxNTZDNS4zMTUwMiA1LjYwMDEgNS42MDE1NiA1LjMxMzU2IDUuNjAxNTYgNC45NjAxVjIuMjQwMUM1LjYwMTU2IDEuODg2NjQgNS4zMTUwMiAxLjYwMDEgNC45NjE1NiAxLjYwMDFaIiBmaWxsPSIjZmZmIi8%2BCjxwYXRoIGQ9Ik00Ljk2MTU2IDEwLjM5OTlIMi4yNDE1NkMxLjg4ODEgMTAuMzk5OSAxLjYwMTU2IDEwLjY4NjQgMS42MDE1NiAxMS4wMzk5VjEzLjc1OTlDMS42MDE1NiAxNC4xMTM0IDEuODg4MSAxNC4zOTk5IDIuMjQxNTYgMTQuMzk5OUg0Ljk2MTU2QzUuMzE1MDIgMTQuMzk5OSA1LjYwMTU2IDE0LjExMzQgNS42MDE1NiAxMy43NTk5VjExLjAzOTlDNS42MDE1NiAxMC42ODY0IDUuMzE1MDIgMTAuMzk5OSA0Ljk2MTU2IDEwLjM5OTlaIiBmaWxsPSIjZmZmIi8%2BCjxwYXRoIGQ9Ik0xMy43NTg0IDEuNjAwMUgxMS4wMzg0QzEwLjY4NSAxLjYwMDEgMTAuMzk4NCAxLjg4NjY0IDEwLjM5ODQgMi4yNDAxVjQuOTYwMUMxMC4zOTg0IDUuMzEzNTYgMTAuNjg1IDUuNjAwMSAxMS4wMzg0IDUuNjAwMUgxMy43NTg0QzE0LjExMTkgNS42MDAxIDE0LjM5ODQgNS4zMTM1NiAxNC4zOTg0IDQuOTYwMVYyLjI0MDFDMTQuMzk4NCAxLjg4NjY0IDE0LjExMTkgMS42MDAxIDEzLjc1ODQgMS42MDAxWiIgZmlsbD0iI2ZmZiIvPgo8cGF0aCBkPSJNNCAxMkwxMiA0TDQgMTJaIiBmaWxsPSIjZmZmIi8%2BCjxwYXRoIGQ9Ik00IDEyTDEyIDQiIHN0cm9rZT0iI2ZmZiIgc3Ryb2tlLXdpZHRoPSIxLjUiIHN0cm9rZS1saW5lY2FwPSJyb3VuZCIvPgo8L3N2Zz4K&logoColor=ffffff)](https://zread.ai/ouywm/summerrs-admin)

</div>

---

完全使用 Rust 构建的生产就绪全栈后台管理系统，基于 Summer 框架。提供 JWT 身份验证、RBAC 授权、数据库分片、多租户隔离、实时 Socket.IO 通信、AI 网关（LLM Relay）、MCP 服务器集成以及声明式代码生成——所有这些都通过模块化插件架构组合而成。

## 项目独特之处

Summerrs-admin 集成了四种功能：

1. **LLM 中转网关** - 统一入口代理多家 AI 供应商，支持 OpenAI/Claude/Gemini 原生协议，自动故障转移与计费
2. **数据库分片中间件** - SQL 解析、路由、改写及跨分片结果合并
3. **MCP 服务器** - AI 编程助手可发现数据库结构、生成 CRUD 模块、部署菜单和字典
4. **声明式宏系统** - 将鉴权检查、操作日志记录和限流降维成单行属性

结合 Socket.IO 实时消息、S3 文件存储、后台任务调度，提供完整的后台系统功能。

## 架构概览

系统遵循插件组合模式。`crates/app/src/main.rs` 中的二进制入口点将 15 个插件组装到单个 App 实例中，每个插件负责一个垂直领域。请求流量在到达 Axum 路由之前，会经过 Tower 中间件层（CORS、压缩、异常处理、客户端 IP 提取），在路由层，声明式宏在处理函数级别强制执行鉴权和日志记录，而分片/SQL 改写中间件则透明地拦截数据库调用。

## 核心特性

### 身份验证与授权
- **JWT 支持** - HS256/RS256/ES256/EdDSA 算法与会话管理
- **RBAC** - 基于角色的访问控制与权限位图
- **声明式宏** - `#[login]`、`#[has_perm]`、`#[has_role]`、`#[public]`
- **会话管理** - 并发登录控制、设备限制、令牌刷新

### 数据库与多租户
- **数据库分片** - SQL 解析、路由和跨分片合并
- **四种隔离级别**：
  - **共享行** - 所有租户共享表；通过 SQL 改写利用 `tenant_id` 列过滤
  - **独立表** - 每个租户拥有自己的表（例如 `user_001`、`user_002`）
  - **独立 Schema** - 每个租户拥有自己的 PostgreSQL schema
  - **独立数据库** - 每个租户拥有自己的物理数据库
- **SQL 改写** - 透明租户上下文注入
- **CDC 管道** - 跨租户变更数据捕获
- **加密/脱敏/审计** - 内置于分片层

### 实时与后台处理
- **Socket.IO** - 实时通信，会话状态存储在 Redis
- **后台任务** - 带类型的异步任务调度
- **批量日志收集** - 异步操作日志持久化

### AI 网关（summer-ai）
- **协议适配器** - 40+ 供应商的 ZST（零大小类型）适配器
- **动态上游路由** - 6 个维度的运行时决策：协议家族、Endpoint、凭证、模型映射、额外 headers、路由策略
- **多入口协议** - OpenAI、Claude、Gemini 原生端点
- **三阶段计费** - Reserve → Settle → Refund 原子扣费
- **自动故障转移** - 失败时自动重试其他渠道
- **热更新配置** - 数据库驱动，无需重启
- **流式处理** - SSE 实时响应（流式不重试）
- **请求追踪** - 完整生命周期日志与重试记录

### MCP 服务器集成
- **结构发现** - AI 可以发现数据库结构
- **代码生成** - 通过 AI 工具生成 CRUD 模块
- **菜单/字典工具** - 通过提示词部署菜单和字典
- **Rig LLM 框架** - 支持 OpenAI、DeepSeek、Ollama

### 存储与工具
- **S3 存储** - 支持大文件分片上传（AWS S3、MinIO）
- **IP 地理定位** - IP2Region 用于登录日志
- **国际化** - 编译时 i18n（中英文）
- **限流** - 5 种算法：固定窗口、滑动窗口、令牌桶、漏桶、Lua 脚本

## 项目结构

```
summerrs-admin/
├── crates/
│   ├── app/                          # 应用入口
│   ├── summer-system/                # 业务模块：RBAC、CRUD、Socket.IO
│   ├── summer-auth/                  # JWT 认证与授权
│   ├── summer-ai/                    # AI 网关（LLM Relay）
│   ├── summer-sharding/              # 数据库分片中间件
│   ├── summer-sql-rewrite/           # SQL 改写引擎
│   ├── summer-mcp/                   # MCP 服务器
│   └── summer-plugins/               # 插件实现
├── config/                           # 环境配置
├── sql/                              # 数据库模式
└── docs/                             # 项目文档
```



## 开源协议

详见 [LICENSE](LICENSE)。
