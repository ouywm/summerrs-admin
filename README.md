# summerrs-admin

基于 Rust 的企业级后台管理系统框架，采用 Summer 框架构建，集成 AI 网关、实时通信、MCP 代码生成等能力。

## 技术栈

| 层级 | 技术 |
|------|------|
| Web 框架 | Axum + Summer |
| ORM | SeaORM 2.0（PostgreSQL） |
| 认证 | JWT（HS256/RS256）+ Redis Session |
| 实时通信 | Socket.IO |
| AI 网关 | 自研多 Provider 中继（OpenAI / Deepseek / Ollama） |
| LLM 框架 | Rig 0.33 |
| MCP 服务 | RMCP（Streamable HTTP） |
| 对象存储 | AWS SDK S3 |
| 异步运行时 | Tokio |

## 项目结构

```
summerrs-admin/
├── crates/
│   ├── app/                      # 应用装配入口
│   ├── summer-system/            # 系统模块（路由、服务、插件）
│   │   └── model/                # summer-system-model 子 crate（Entity/DTO/VO）
│   ├── summer-auth/              # 认证授权（JWT、多设备、Token 刷新）
│   ├── summer-ai/                # AI 聚合包
│   │   ├── core/                 # AI Provider 适配层
│   │   ├── hub/                  # AI 网关运行时
│   │   └── model/                # AI 数据模型
│   ├── summer-mcp/               # MCP 服务器（Schema 工具、代码生成）
│   ├── summer-rig/               # Rig LLM 框架集成
│   ├── summer-domain/            # 领域模型
│   ├── summer-common/            # 通用工具（校验、密码、Response）
│   ├── summer-plugins/           # 通用插件（S3、任务、日志、IP 查询）
│   ├── summer-sharding/          # 数据分片
│   └── summer-admin-macros/      # 过程宏
├── config/                       # 应用配置（dev/prod/test）
├── sql/                          # 数据库脚本
│   ├── sys/                      # 系统表（用户、角色、菜单、字典…）
│   ├── biz/                      # 业务表（预留）
│   ├── ai/                       # AI 网关表（Channel、Request、Price…）
│   └── migration/                # 迁移脚本
├── build-tools/                  # 构建与质检脚本
├── skills/                       # Claude Code 开发手册
└── .github/workflows/            # CI/CD
```

## 核心特性

- **插件化架构** — 所有能力以 Plugin 形式注册，`crates/app` 仅做装配
- **Schema Sync** — SeaORM 自动补结构，无需手写 DDL 迁移
- **AI 中继网关** — 多渠道、价格版本、限流、流式响应
- **MCP 代码生成** — 从数据库表一键生成 Entity、CRUD 模块、前端代码
- **实时通信** — Socket.IO + Redis 网关，支持多房间推送
- **多设备认证** — JWT + Redis，并发登录控制、Token 刷新

## 快速开始

### 环境要求

- Rust (latest stable)
- PostgreSQL
- Redis
- MinIO 或 S3 兼容存储（可选）

### 1. 初始化数据库

按顺序执行 SQL 脚本：

```bash
# 创建数据库
createdb summerrs-admin

# 按顺序初始化
psql -d summerrs-admin -f sql/sys/*.sql
psql -d summerrs-admin -f sql/ai/*.sql

# 可选：导入菜单种子数据
psql -d summerrs-admin -f sql/sys/menu_data_all.sql
```

### 2. 配置环境变量

创建 `.env` 文件：

```env
DATABASE_URL=postgres://admin:123456@localhost/summerrs-admin?options=-c%20TimeZone%3DAsia%2FShanghai
JWT_SECRET=your-jwt-secret
S3_ACCESS_KEY=your-access-key
S3_SECRET_KEY=your-secret-key
RIG_OPENAI_API_KEY=your-api-key
```

### 3. 启动服务

```bash
cargo run -p app
```

### 代码质检

```bash
# 安装 Git pre-commit hook
./build-tools/install-git-hooks.sh

# 手动运行检查
./build-tools/taplofmt.sh --check   # TOML 格式
./build-tools/rustfmt.sh --check    # Rust 格式
./build-tools/rustcheck.sh check    # 编译检查
./build-tools/rustcheck.sh clippy   # Lint
./build-tools/rustcheck.sh test-compile  # 测试编译
```
