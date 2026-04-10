# summerrs-admin

基于 Rust 的企业级后台管理系统，采用插件化架构构建。集成 AI 多模型网关、分库分表与多租户、SQL 改写引擎、MCP 代码生成、实时通信等能力，约 **180,000 行** Rust 代码。

## 技术栈

| 层级 | 技术 | 说明 |
|------|------|------|
| Web 框架 | Axum + [Summer](https://crates.io/crates/summer) 0.5 | 插件化应用框架，自动配置、组件注册 |
| ORM | SeaORM 2.0 | PostgreSQL，支持 Schema Sync 自动建表 |
| 认证 | JWT (HS256/RS256) + Redis Session | 多设备并发登录、Token 刷新、QR Code 登录 |
| 权限 | Bitmap 权限映射 + 过程宏 | `#[has_perm]` / `#[has_role]` 声明式鉴权 |
| 实时通信 | Socket.IO + Redis 网关 | 多房间推送、会话管理 |
| AI 网关 | 自研多 Provider 中继 | OpenAI / Anthropic / Gemini / Azure，流式 SSE |
| LLM 框架 | [Rig](https://crates.io/crates/rig-core) 0.33 | 多 Provider 注册，统一调用接口 |
| MCP 服务 | [RMCP](https://crates.io/crates/rmcp) 1.2 (Streamable HTTP) | Schema 发现、通用表工具、代码生成 |
| 数据分片 | 自研 summer-sharding | 分库分表、读写分离、CDC、加密、脱敏、在线 DDL |
| SQL 改写 | 自研 summer-sql-rewrite | 插件化 SQL 改写管道，透明接入 SeaORM |
| 对象存储 | AWS SDK S3 | 兼容 MinIO/RustFS，分片上传、预签名 URL |
| 异步运行时 | Tokio | 全异步，graceful shutdown |

## 项目结构

```
summerrs-admin/
├── crates/
│   ├── app/                        # 应用装配入口（仅做 Plugin 注册）
│   │
│   ├── summer-system/              # 系统管理模块
│   │   ├── router/                 #   HTTP 路由（用户/角色/菜单/字典/配置/文件/日志/监控…）
│   │   ├── service/                #   业务服务层
│   │   ├── socketio/               #   Socket.IO 实时通信网关
│   │   ├── plugins/                #   Bitmap 权限插件、Socket 网关插件
│   │   └── job/                    #   定时任务（S3 清理、Socket 会话 GC）
│   ├── summer-system-model/        # 系统数据模型（Entity / DTO / VO / Views）
│   │
│   ├── summer-auth/                # 认证授权
│   │   ├── token/                  #   JWT 签发与验证（HS256/RS256）
│   │   ├── session/                #   多设备会话管理（Admin/Business/Customer）
│   │   ├── bitmap/                 #   权限位图映射
│   │   ├── middleware/             #   Axum 认证中间件
│   │   └── path_auth/             #   路径级鉴权配置
│   │
│   ├── summer-ai/                  # AI 聚合包（DDD 架构）
│   │   ├── core/                   #   Provider 适配层（OpenAI/Anthropic/Gemini/Azure）
│   │   │   ├── provider/           #     各 Provider 客户端实现
│   │   │   └── types/              #     统一 AI 类型系统（Chat/Embedding/Image/Audio/Batch…）
│   │   ├── hub/                    #   AI 网关运行时（DDD 四层）
│   │   │   ├── interfaces/         #     HTTP 接口层
│   │   │   ├── application/        #     应用服务层
│   │   │   ├── domain/             #     领域模型层
│   │   │   └── infrastructure/     #     基础设施层
│   │   └── model/                  #   AI 数据模型（Entity/DTO/VO，70+ 张表）
│   │
│   ├── summer-sharding/            # 数据分片引擎（~25,000 行）
│   │   ├── algorithm/              #   分片算法（Hash/Range/Time/Tenant/Complex）
│   │   ├── router/                 #   SQL 路由（表路由/Schema 路由/读写路由/Hint 路由）
│   │   ├── rewrite/                #   SQL 改写（表名/聚合/Limit/加密/Schema）
│   │   ├── execute/                #   分布式执行（Scatter-Gather 并行执行器）
│   │   ├── merge/                  #   结果归并（排序/分组/Limit/流式归并）
│   │   ├── cdc/                    #   变更数据捕获（PG Logical → Postgres/ClickHouse Sink）
│   │   ├── encrypt/                #   字段加密（AES-GCM + 摘要）
│   │   ├── masking/                #   数据脱敏（手机/邮箱/IP/部分遮掩）
│   │   ├── tenant/                 #   多租户（Schema 隔离 / 行级隔离 / SQL 改写）
│   │   ├── ddl/                    #   在线 DDL（Ghost Table 无锁变更）
│   │   ├── migration/              #   数据迁移（重分片编排、归档清理）
│   │   ├── keygen/                 #   分布式主键（Snowflake / TSID）
│   │   ├── datasource/             #   数据源管理（连接池/健康检查/运行时指标）
│   │   ├── connector/              #   连接器（分片连接/Hint/两阶段事务）
│   │   ├── shadow/                 #   影子库（压测流量路由）
│   │   ├── audit/                  #   SQL 审计
│   │   └── lookup/                 #   查找表索引
│   │
│   ├── summer-sql-rewrite/         # SQL 改写引擎
│   │   ├── pipeline/               #   改写管道（插件链式执行）
│   │   ├── registry/               #   插件注册中心
│   │   ├── connection/             #   改写连接（透明代理 SeaORM DatabaseConnection）
│   │   └── web/                    #   Web 中间件（请求级上下文注入）
│   │
│   ├── summer-mcp/                 # MCP 服务器
│   │   ├── table_tools/            #   通用表工具（CRUD/Schema 发现/SQL 扫描）
│   │   └── tools/                  #   代码生成（Entity/CRUD 模块/前端 API+页面 Bundle）
│   │
│   ├── summer-rig/                 # Rig LLM 框架集成（多 Provider 注册）
│   ├── summer-domain/              # 领域模型（菜单树、字典同步）
│   ├── summer-model/               # 通用数据模型
│   ├── summer-common/              # 通用工具（加解密、响应封装、文件、UA 解析）
│   ├── summer-plugins/             # 通用插件
│   │   ├── s3/                     #   S3 对象存储（分片上传/预签名）
│   │   ├── background_task/        #   后台任务调度
│   │   ├── log_batch_collector/    #   日志批量采集
│   │   ├── ip2region/              #   IP 地理查询
│   │   └── entity_schema_sync/     #   SeaORM Entity ↔ DB Schema 自动同步
│   └── summer-admin-macros/        # 过程宏
│       ├── #[log]                  #   操作日志自动记录
│       ├── #[login]                #   登录校验
│       ├── #[has_perm]             #   权限校验（支持通配符）
│       ├── #[has_role]             #   角色校验
│       ├── #[has_perms]            #   多权限校验（AND/OR）
│       └── #[has_roles]            #   多角色校验（AND/OR）
│
├── config/                         # 应用配置
│   ├── app-dev.toml                #   开发环境（默认）
│   ├── app-prod.toml               #   生产环境
│   └── app-test.toml               #   测试环境
├── sql/                            # 数据库脚本
│   ├── sys/                        #   系统表（用户/角色/菜单/字典/配置/日志/租户…）
│   ├── ai/                         #   AI 网关表（Channel/Request/Trace/Alert/Guardrail…70+）
│   └── migration/                  #   迁移脚本
├── build-tools/                    # 构建与质检脚本
└── .github/workflows/              # CI（Rustfmt / Taplo / Check / Clippy / Test Compile）
```

## 核心特性

### 插件化架构

所有能力以 Summer Plugin 形式注册，`crates/app/src/main.rs` 仅做装配：

```rust
App::new()
    .add_plugin(WebPlugin)
    .add_plugin(SeaOrmPlugin)
    .add_plugin(RedisPlugin)
    .add_plugin(SummerAuthPlugin)
    .add_plugin(SummerShardingPlugin)
    .add_plugin(SummerSqlRewritePlugin)
    .add_plugin(SummerAiHubPlugin)
    .add_plugin(McpPlugin)
    .add_plugin(SummerRigPlugin)
    // ... 更多插件
    .run()
    .await;
```

### AI 多模型网关

- 多 Provider 统一适配（OpenAI / Anthropic / Gemini / Azure）
- 渠道管理、模型价格版本、负载均衡
- 流式 SSE 响应中继
- 请求追踪（Trace / Span）、审计日志
- 护栏规则（Guardrail）、告警（Alert）、死信队列
- DDD 分层架构（Interfaces → Application → Domain → Infrastructure）

### 数据分片与多租户

- **分片路由**：Hash Mod / Hash Range / Time Range / Tenant / Complex 组合算法
- **SQL 改写**：自动表名替换、聚合改写、Limit 改写、Schema 改写
- **分布式执行**：Scatter-Gather 并行执行 + 结果流式归并（排序/分组/Limit）
- **多租户**：Schema 隔离 / 行级隔离（SQL 自动注入 tenant_id 条件）
- **CDC**：PostgreSQL Logical Replication → Postgres / ClickHouse Sink
- **加密脱敏**：字段级 AES-GCM 加密、手机/邮箱/IP 部分脱敏
- **在线 DDL**：Ghost Table 无锁表结构变更
- **数据迁移**：重分片编排、数据归档
- **影子库**：压测流量自动路由到影子表
- **分布式主键**：Snowflake / TSID 生成器

### SQL 改写引擎

独立的 `summer-sql-rewrite` crate，提供插件化 SQL 改写管道：

- 透明代理 SeaORM `DatabaseConnection`，业务代码无感知
- 插件链式执行，支持自定义改写规则
- Web 中间件自动注入请求级上下文（如租户信息）

### MCP 代码生成

内嵌 MCP 服务器（Streamable HTTP），提供 AI 辅助开发能力：

- **Schema 发现**：`schema://tables`、`schema://table/{name}` 资源
- **通用表工具**：`table_get` / `table_query` / `table_insert` / `table_update` / `table_delete`
- **SQL 工具**：`sql_query_readonly`（复杂查询）、`sql_exec`（DDL/DML）
- **代码生成**：
  - `generate_entity_from_table` — SeaORM Entity（自动 Enum 语义提升）
  - `generate_admin_module_from_table` — 后端 CRUD 模块脚手架
  - `generate_frontend_bundle_from_table` — 前端 API + 类型 + 页面一键生成
- **业务工具**：`menu_tool` / `dict_tool`（菜单字典结构化管理）

### 认证授权

- JWT 签发（HS256 / RS256 可配置）
- 多用户类型（Admin / Business / Customer）
- 多设备并发登录控制，最大设备数限制
- Access Token + Refresh Token 双 Token 机制
- Redis Session 管理，在线用户踢下线
- Bitmap 权限映射，高性能权限校验
- 声明式过程宏：`#[login]`、`#[has_perm("system:user:list")]`、`#[has_roles(or("admin", "editor"))]`

### 系统管理

用户、角色、菜单、字典、配置、租户、文件、通知、日志（登录/操作）、在线用户监控等完整 RBAC 后台管理功能。

### 实时通信

Socket.IO + Redis 跨实例网关，支持多命名空间、多房间推送、会话 GC。

## 快速开始

### 环境要求

- Rust (latest stable)
- PostgreSQL 15+
- Redis 7+
- MinIO 或 S3 兼容存储（可选）

### 1. 初始化数据库

```bash
createdb summerrs-admin

# 初始化系统表
psql -d summerrs-admin -f sql/sys/*.sql

# 初始化 AI 网关表
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

服务默认监听 `http://localhost:8080`，API 前缀 `/api`。

- OpenAPI 文档：`http://localhost:8080/docs`
- MCP 端点：`http://localhost:8080/api/mcp`

### 配置文件

配置采用 TOML 格式，支持环境变量占位符 `${VAR:default}`：

| 文件 | 用途 |
|------|------|
| `config/app-dev.toml` | 开发环境（默认加载） |
| `config/app-prod.toml` | 生产环境 |
| `config/app-test.toml` | 测试环境 |

主要配置段：`[web]`、`[sea-orm]`、`[redis]`、`[auth]`、`[s3]`、`[mcp]`、`[rig]`、`[summer-sharding]`、`[socket_io]` 等。