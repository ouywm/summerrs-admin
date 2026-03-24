---
name: summerrs-admin
description: >
  Use when working inside the Summerrs Admin workspace, especially for defining
  routes, services, plugins, components, SeaORM 2.0 entities, DTO/VO contracts,
  MCP generators, auth flows, Socket.IO runtime behavior, or project-specific
  module boundaries. This skill teaches the repo's implementation patterns, not
  just its architecture.
---

# Summerrs Admin

当任务是在这个仓库里“按项目约定直接写代码、接口、实体、插件、生成代码”时，使用这个 skill。

它不是项目介绍，而是开发手册。

## Quick Start

- 写系统模块接口、路由、分页、校验：读 `references/route-service.md`
- 写插件、组件、配置、启动装配：读 `references/plugin-component.md`
- 写 SeaORM 2.0 实体、DTO、VO、schema sync：读 `references/data-model.md`
- 用 MCP 生成 entity / CRUD / 前端 bundle：读 `references/mcp-generator.md`
- 改登录、在线用户、踢下线、Socket.IO：读 `references/auth-realtime.md`

## Project Map

- `crates/app`：应用装配根，只负责注册插件和启动
- `crates/summer-system`：system 的 `router/service/plugins/socketio/job`
- `crates/summer-system-model`：system 的 `dto/vo/entity`，其中 `entity_gen` 是可覆盖生成层，`entity` 是稳定扩展层
- `crates/summer-ai-model`：AI 领域的表结构与共享契约
- `crates/summer-ai-hub`：AI 网关运行时与业务实现
- `crates/summer-mcp`：schema 工具、生成器、菜单/字典业务工具
- `crates/summer-auth`：token、session、登录态、在线用户基础能力
- `crates/summer-plugins`：通用插件（S3、任务、日志采集等）

## Task Recipes

### 1. 给 system 模块补接口或 CRUD

先读 `references/route-service.md`，优先对照：

- `crates/summer-system/src/router/sys_user.rs`
- `crates/summer-system/src/service/sys_user_service.rs`
- `crates/summer-system/src/router/auth.rs`

默认流程：先补 DTO / VO，再写 route，再把查询、事务、聚合逻辑下沉到 service。

### 2. 新增或调整 SeaORM 实体

先读 `references/data-model.md`。本仓库当前约定：

- 使用 SeaORM 2.0 entity-first 风格
- system 模型放 `crates/summer-system-model`
- `src/entity_gen` 放可覆盖的原始实体源码
- `src/entity` 是稳定入口和扩展层
- 不使用数据库外键；逻辑关系用 `belongs_to + skip_fk`

不要把业务扩展直接写进 `entity_gen`。

### 3. 新增插件、组件或配置

先读 `references/plugin-component.md`，优先对照：

- `crates/summer-system/src/plugins/perm_bitmap.rs`
- `crates/summer-system/src/plugins/schema_sync.rs`
- `crates/summer-system/src/plugins/socket_gateway.rs`
- `crates/summer-mcp/src/plugin.rs`
- `crates/app/src/main.rs`

### 4. 用 MCP 生成代码

先读 `references/mcp-generator.md`。默认顺序：

1. 先发现 schema
2. 先生成 raw entity / CRUD 骨架
3. 再把业务语义补到 `service`、`entity` 扩展层、菜单、字典里

生成器优先解决重复劳动，不要一上来手搓标准 CRUD。

### 5. 改认证、在线用户或实时推送

先读 `references/auth-realtime.md`，优先对照：

- `crates/summer-system/src/router/auth.rs`
- `crates/summer-system/src/service/auth_service.rs`
- `crates/summer-system/src/service/online_service.rs`
- `crates/summer-system/src/socketio/*`

这类需求通常跨 `router + service + socketio + summer-auth`，不要只改一个 handler。

## Core Rules

- `router` 薄，`service` 厚
- system 业务代码默认放在 `crates/summer-system`，不是 `crates/app`
- `crates/app` 主要是装配根，不堆业务逻辑
- system 数据契约默认放 `crates/summer-system-model`
- `entity_gen` 可覆盖，`entity` 稳定；应用代码只依赖 `entity`
- 不使用数据库外键；关系字段默认 `belongs_to + skip_fk`
- `schema sync` 负责补结构，不负责危险变更
- 字段真实重命名必须显式用 `renamed_from`
- 路由返回优先 `summer_common::response::Json<T>`
- `#[log]` 必须放在路由宏上方
- 菜单、按钮、字典优先走 MCP 业务工具，不要直接手写裸 SQL

## Commit Checks

- 每次提交前必须先跑 `./build-tools/taplofmt.sh --check`
- 如果 `taplo` 检查失败，先跑 `./build-tools/taplofmt.sh --fix`，再重新执行 `--check`
- 如果本次改动涉及 Rust 代码，提交前至少补跑一次受影响 crate 的 `cargo check`
- 推荐在本地执行一次 `./build-tools/install-git-hooks.sh`，启用项目内置的 `pre-commit` hook
- `pre-commit` hook 当前会按改动类型自动执行 `./build-tools/taplofmt.sh --check`、`./build-tools/rustfmt.sh --check`、`./build-tools/rustcheck.sh check`、`./build-tools/rustcheck.sh clippy`、`./build-tools/rustcheck.sh test-compile`
- Git hook 入口统一使用 `build-tools/pre-commit`，不要再额外维护一份 `.githooks/pre-commit`
- Git hook 只能约束本地提交，不能替代 CI

## Canonical Files

- 应用装配：`crates/app/src/main.rs`
- 标准 CRUD route：`crates/summer-system/src/router/sys_user.rs`
- 认证 route：`crates/summer-system/src/router/auth.rs`
- 标准 service：`crates/summer-system/src/service/sys_user_service.rs`
- 在线用户：`crates/summer-system/src/service/online_service.rs`
- schema sync 插件：`crates/summer-system/src/plugins/schema_sync.rs`
- system entity 扩展入口：`crates/summer-system-model/src/entity/sys_user.rs`
- system raw entity：`crates/summer-system-model/src/entity_gen/sys_user.rs`
- system DTO：`crates/summer-system-model/src/dto/sys_user.rs`
- system VO：`crates/summer-system-model/src/vo/sys_user.rs`
- MCP 实现入口：`crates/summer-mcp/src/tools/*`

## Local Source Docs

这份 skill 主要提炼自本地 Summer 文档，并结合本仓库实际写法做了 AI 化整理。原始文档在：

- `/Volumes/990pro/code/rust/summer-docs/site/zh/docs/getting-started/quick-start.md`
- `/Volumes/990pro/code/rust/summer-docs/site/zh/docs/getting-started/di.md`
- `/Volumes/990pro/code/rust/summer-docs/site/zh/docs/getting-started/config.md`
- `/Volumes/990pro/code/rust/summer-docs/site/zh/docs/getting-started/component.md`
- `/Volumes/990pro/code/rust/summer-docs/site/zh/docs/plugins/summer-web.md`
- `/Volumes/990pro/code/rust/summer-docs/site/zh/docs/plugins/summer-sea-orm.md`
- `/Volumes/990pro/code/rust/summer-docs/site/zh/docs/plugins/plugin-by-self.md`
