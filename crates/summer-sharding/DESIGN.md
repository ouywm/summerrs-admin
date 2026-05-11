# summer-sharding 设计文档

> 单机分库分表 + 多租户 SQL 中间件，基于 `summer-sql-rewrite` 通用改写 pipeline。

---

## 一、定位

`summer-sharding` 是建立在 `summer-sql-rewrite` 之上的业务层 SQL 中间件，提供两类核心能力：

1. **分库分表路由**：按规则把逻辑 SQL 路由到一个或多个物理数据源、物理表
2. **多租户隔离**：支持 4 种隔离级别，从轻量行级共享到独立数据库

中间件的执行轨迹是：

```
应用层 SeaORM 查询
        │
        ▼
ShardingConnection.execute_raw / query_*_raw
        │
        ▼
StatementContext  ← 解析 SQL 抽取表 / WHERE 条件 / hint / tenant
        │
        ▼
DefaultSqlRouter  ← 决策路由：单分片 / 跨分片 / 广播
        │
        ▼
SqlRewriter + PluginRegistry  ← 改写 SQL：表名替换 / 租户注入 / 用户插件
        │
        ▼
ScatterGatherExecutor  ← 单/多目标执行
        │
        ▼
DefaultResultMerger  ← 跨分片结果合并（如有）
        │
        ▼
QueryResult 返回上层
```

---

## 二、分层关系

```
┌─────────────────────────────────────────────┐
│  应用代码（business code）                  │
│  - sys_tenant_service                       │
│  - 业务 Repo / Service                      │
└─────────────────────────────────────────────┘
                  ▲ 依赖
┌─────────────────────────────────────────────┐
│  summer-sharding（分片层）                  │
│  - 分库分表路由 / 多租户隔离                │
│  - 内置表名改写、租户条件注入               │
│  - TableShardingPlugin（order=30）          │
└─────────────────────────────────────────────┘
                  ▲ 依赖
┌─────────────────────────────────────────────┐
│  summer-sql-rewrite（插件层）               │
│  - SqlRewritePlugin trait / PluginRegistry  │
│  - RewriteConnection（非分片查询入口）      │
│  - 内置插件：OptimisticLock / AutoFill /    │
│    DataScope（order 50-70）                 │
│  - SQL 解析 / AST 操作辅助                  │
└─────────────────────────────────────────────┘
                  ▲ 依赖
┌─────────────────────────────────────────────┐
│  sea-orm / sqlx                             │
└─────────────────────────────────────────────┘
```

**关键原则**：

- `summer-sql-rewrite` 不感知"分片 / 租户"等业务概念，只提供 pipeline + 内置通用插件
- `summer-sharding` 专注分片路由和多租户，不暴露插件注册入口（由 sql-rewrite 层负责）
- `TableShardingPlugin` 因依赖 `ShardingRouteInfo` 留在 summer-sharding，由 `SummerShardingPlugin` 自动注册
- 用户在 `main.rs` 通过 `sql_rewrite_configure` 注册插件，两个 plugin 共享同一个 `PluginRegistry`

### 2.1 执行顺序

**`ShardingConnection` 路径（分片查询）：**

```
1. DefaultSqlRouter::route()          ← 分片路由决策
2. DefaultSqlRewriter::rewrite()      ← sharding 内部改写（每个目标分片）：
   a. rewrite_table_names()           ← 逻辑表 → 物理表
   b. apply_schema_rewrite()          ← schema 限定
   c. inflate_limit_for_fanout()      ← LIMIT 放大（fanout 时）
   d. apply_tenant_rewrite()          ← 租户条件注入
   e. registry.rewrite_all()          ← 插件管道（summer-sql-rewrite）：
      - TableShardingPlugin (order=30) ← 写分片路由注释
      - OptimisticLockPlugin (order=50)
      - AutoFillPlugin (order=60)
      - DataScopePlugin (order=70)
      - 用户自定义插件 (order=100+)
3. ScatterGatherExecutor              ← 并行执行
4. DefaultResultMerger                ← 跨分片合并
```

**`RewriteConnection` 路径（非分片查询，sys 表等）：**

```
1. registry.rewrite_all()             ← 插件管道（summer-sql-rewrite）：
   - OptimisticLockPlugin (order=50)
   - AutoFillPlugin (order=60)
   - DataScopePlugin (order=70)
   - 用户自定义插件 (order=100+)
2. Execute
```

sharding 内部改写（步骤 2a-2d）先于插件管道执行，插件看到的是已替换物理表名、已注入租户条件的 SQL。

---

## 三、租户隔离

### 3.1 四种隔离级别

`TenantIsolationLevel`（`config/tenant.rs`）：

| 级别 | 含义 | 实现方式 | 适用场景 |
|---|---|---|---|
| `SharedRow` | 同表共享，行级隔离 | WHERE 注入 `tenant_id = ?` | 租户少、量小、SaaS 通用 |
| `SeparateTable` | 同库同 schema，表名后缀 | `table` → `table_t<tenant>` | 中等隔离强度，迁移容易 |
| `SeparateSchema` | 同库不同 schema | `public.table` → `tenant_<id>.table` | 强隔离，备份/恢复独立 |
| `SeparateDatabase` | 不同物理库 | 路由到该租户独立的 datasource | 最强隔离，合规场景 |

行级安全（PG RLS）已删除，不再支持。

### 3.2 租户上下文注入

由 `web/middleware.rs` 的 `TenantContextLayer` 从请求中提取 `TenantContext`，注入到请求 extension。
触发位置由 `TenantIdSource` 决定（`config/tenant.rs`）：

- `RequestExtension`（默认）：从上游 layer 注入的 `TenantContext` 读取
- `Header`：从 HTTP header 取 tenant_id_field
- `JwtClaim`：从 JWT claim 中读取
- `QueryParam`：从 URL query string 取
- `Context`：从自定义请求上下文取

业务代码通过 `ShardingConnection::with_tenant_context(ctx)` 显式绑定租户，触发改写。

### 3.3 租户元数据与生命周期

`TenantMetadataStore` 保存所有租户配置（隔离级别、schema 名、独立数据源连接信息等）：

- **加载**：启动期由 `TenantMetadataLoader` 从 `sys_tenant` 表读取
- **运行期变更**：`TenantMetadataListener`（PG NOTIFY）监听变更事件，热更新 `TenantMetadataStore`
- **CRUD**：`TenantLifecycleManager` 提供 onboard/offboard 流程
  - SeparateTable → 自动 `CREATE TABLE ... (LIKE base_table)`
  - SeparateSchema → 自动 `CREATE SCHEMA tenant_<id>`
  - SeparateDatabase → 由运维预先准备好数据库，配置 `db_uri`
  - 触发 `pg_notify` 让所有进程的 `TenantMetadataStore` 热更新

---

## 四、分片路由

### 4.1 路由决策流程

`DefaultSqlRouter::route()` 按以下顺序决策：

```
解析后的 StatementContext
   │
   ├─ 1. 无 primary_table → 路由到默认 datasource（系统表 / 元数据查询）
   │
   ├─ 2. primary_table 不在 sharding 规则里
   │     → 单分片直发，schema 路由决定 datasource
   │
   └─ 3. primary_table 命中 sharding 规则
          │
          ├─ 3a. 有 hint → 显式指定的目标
          │
          ├─ 3b. INSERT → 从 VALUES 提取 sharding_column 值 → 算法定位
          │
          └─ 3c. SELECT/UPDATE/DELETE
                 │
                 ├─ WHERE 有 sharding_column 精确条件 → 算法定位单/多分片
                 ├─ WHERE 有 sharding_column 范围条件 → 算法 do_range_sharding
                 └─ 都没有 → 广播（fan-out 到所有分片）
```

绑定表组（`binding_groups`）：JOIN 涉及多个分片表时，按主表的分片结果同步对齐其他表。

### 4.2 分片算法

`AlgorithmRegistry::build()`（`algorithm/mod.rs`）按规则 `algorithm` 字段建实例，内置 6 种：

| 名称 | 适用 | 行为 |
|---|---|---|
| `hash_mod` | 均匀打散 | `hash(sharding_value) % count` 取目标 |
| `inline` | 表达式分片 | 配置表达式 `t_user_${id % 4}` 直接渲染 |
| `tenant` | 租户路由 | 直接以 `tenant_id` 当目标 key |
| `time_range` | 时间归档 | 按月/日切分，自动按当前时间过滤近 N 个月分片 |
| `hash_range` | 哈希区间 | hash 落点的范围匹配 |
| `complex` | 复合算法 | 多列组合参与分片，规则可配置 |

### 4.3 跨分片执行与合并

- `ScatterGatherExecutor`：把改写后的多条 SQL 并行发到对应 datasource
- `DefaultResultMerger`：跨分片结果合并
  - `limit/order_by/post_process` 已实现
  - `group_by/stream` 已移除（暂不支持复杂聚合合并，后续按需补）

---

## 五、SQL 改写插件机制

这是这套中间件**最重要的对外能力**。所有业务改写都通过 `SqlRewritePlugin` 实现，不需要改框架代码。

### 5.1 插件接口

`summer_sql_rewrite::SqlRewritePlugin`：

```rust
pub trait SqlRewritePlugin: Send + Sync + 'static {
    /// 插件唯一名称（日志 / 调试用）
    fn name(&self) -> &str;

    /// 执行顺序，越小越先执行。默认 100
    /// 内置插件用 0-99，用户插件建议 100+
    fn order(&self) -> i32 { 100 }

    /// 是否对当前 SQL 生效（按表名 / 操作类型 / 上下文过滤）
    fn matches(&self, ctx: &SqlRewriteContext) -> bool;

    /// 改写 AST 或写注释
    fn rewrite(&self, ctx: &mut SqlRewriteContext) -> Result<()>;
}
```

### 5.2 SqlRewriteContext

插件能拿到的上下文：

```rust
pub struct SqlRewriteContext<'a> {
    pub statement: &'a mut AstStatement,    // sqlparser AST，可直接改
    pub operation: SqlOperation,            // Select/Insert/Update/Delete/Other
    pub tables: Vec<String>,                // SQL 涉及的所有表
    pub original_sql: &'a str,              // 原始 SQL（只读）
    pub extensions: &'a mut Extensions,     // 类型化上下文容器
    pub comments: Vec<String>,              // 写注释（appended 到最终 SQL）
}
```

`Extensions` 是类型化的请求级容器，插件之间通过它传递数据，例如：

- `ShardingRouteInfo`：sharding 写入，业务插件可读
- `CurrentUser`（业务自定义）：web 中间件写入，`AutoFillPlugin` / `DataScopePlugin` 读取
- `DepartmentTree`（业务自定义）：缓存部门树，`DataScopePlugin` 复用

### 5.3 内置 order 约定

为避免业务插件和系统插件冲突，约定执行顺序段位：

| order 段位 | 用途 | 示例 |
|---|---|---|
| 0-9 | 最早期 | 解析后的预处理 |
| 10-29 | 租户改写 | `TenantInjectPlugin`（注入 `tenant_id = ?`） |
| 30-49 | 分片改写 | `TableShardingPlugin`（逻辑表 → 物理表） |
| 50-99 | 安全增强 | `OptimisticLockPlugin` / `AutoFillPlugin` |
| 100+ | 业务定制 | `DataScopePlugin` 等用户插件 |

### 5.4 插件注册

通过 `ShardingRewriteConfigurator`（`rewrite_plugin/configurator.rs`）在 app 启动时注册：

```rust
use summer_sharding::ShardingRewriteConfigurator;

ShardingRewriteConfigurator::new()
    .register(OptimisticLockPlugin::new())
    .register(AutoFillPlugin::new(current_user_provider))
    .register(DataScopePlugin::new(dept_tree_provider))
    .configure(&mut app);
```

注册后所有 `ShardingConnection.execute_raw` 都会按 order 顺序应用全部 matched 插件。

---

## 六、内置插件清单（建议实现，尚未落地）

> 当前代码仅提供框架接口，下面是计划落地的内置示例插件，便于业务参考。

### 6.1 TenantInjectPlugin (`order=10`)

为 SharedRow 隔离的表自动注入 `WHERE tenant_id = ?`，避免业务代码每个 SELECT 都手动加。

### 6.2 OptimisticLockPlugin (`order=50`)

UPDATE 自动加 `version` 字段：

```sql
-- 原 SQL
UPDATE user SET nick_name = 'foo' WHERE id = 1
-- 改写后
UPDATE user SET nick_name = 'foo', version = version + 1
WHERE id = 1 AND version = <ctx 中传入的旧 version>
```

通过 `matches()` 判断目标表是否配置乐观锁字段。

### 6.3 AutoFillPlugin (`order=60`)

INSERT/UPDATE 自动填充审计字段：

- INSERT：`create_time` / `create_by` / `update_time` / `update_by`
- UPDATE：`update_time` / `update_by`

数据从 `ctx.extension::<CurrentUser>()` 读取，需要 web 中间件先注入。

### 6.4 DataScopePlugin (`order=70`)

数据范围控制，从 `ctx.extension::<DataScope>()` 读取当前用户的可见范围：

```rust
pub enum DataScope {
    Self_,                  // creator_id = current_user_id
    Dept,                   // dept_id = current_user.dept_id
    DeptAndChildren,        // dept_id IN (sub-tree)
    Custom(Vec<i64>),       // dept_id IN (...)
    All,                    // 不加条件
}
```

`DeptAndChildren` 的子树查询结果应在中间件层缓存（不是每条 SQL 都查 DB）。
部门表尚未存在时，先实现 `Self_` / `All` 两种，等部门表落地后补全。

---

## 七、配置示例

```toml
[summer-sharding]
enabled = true

# 数据源：可配多个，分别独立连接池
[summer-sharding.datasources.ds_main]
uri = "postgres://user:pass@localhost:5432/main"
schema = "public"
role = "primary"

[summer-sharding.datasources.ds_archive]
uri = "postgres://user:pass@localhost:5432/archive"
schema = "public"
role = "primary"

# 租户配置
[summer-sharding.tenant]
enabled = true
default_isolation = "shared_row"
shared_tables = ["sys_dict", "sys_config"]   # 跨租户共享的表
tenant_id_source = "request_extension"
tenant_id_field = "x-tenant-id"

[summer-sharding.tenant.row_level]
column_name = "tenant_id"
strategy = "sql_rewrite"

# 分片规则：按 tenant_id 哈希取模分 4 张表
[[summer-sharding.sharding.tables]]
logic_table = "ai.request"
actual_tables = ["ai.request_0", "ai.request_1", "ai.request_2", "ai.request_3"]
sharding_column = "tenant_id"
algorithm = "hash_mod"
  [summer-sharding.sharding.tables.algorithm_props]
  count = 4

# 按时间归档：日志表按月切
[[summer-sharding.sharding.tables]]
logic_table = "ai.log"
actual_tables = "ai.log_${yyyyMM}"
sharding_column = "create_time"
algorithm = "time_range"
  [summer-sharding.sharding.tables.algorithm_props]
  granularity = "month"
  retention_months = 12

# 绑定表组：JOIN 时同步路由
[[summer-sharding.sharding.binding_groups]]
tables = ["ai.request", "ai.request_execution"]
sharding_column = "tenant_id"
```

---

## 八、模块文件清单

```
summer-sharding/src/
├── lib.rs                  导出整套对外 API
├── plugin.rs               SummerShardingPlugin（注册到 summer app）
├── error.rs                ShardingError / Result
├── extensions.rs           类型化扩展容器（与 sql-rewrite 共用）
│
├── config/                 配置解析
│   ├── datasource.rs       数据源 / 读写分离配置
│   ├── tenant.rs           租户隔离配置
│   └── rule/               sharding 规则
│       ├── sharding.rs     表规则 / 绑定组 / 全局配置
│       └── runtime.rs      运行时配置（含 SummerShardingConfig）
│
├── algorithm/              分片算法
│   ├── mod.rs              ShardingAlgorithm trait + Registry
│   ├── hash_mod.rs
│   ├── hash_range.rs
│   ├── inline.rs
│   ├── tenant.rs
│   ├── time_range.rs
│   └── complex.rs
│
├── router/                 路由决策
│   ├── mod.rs              SqlRouter trait + DefaultSqlRouter
│   ├── hint_router.rs      hint 显式指定
│   ├── schema_router.rs    按 schema 选 datasource
│   └── table_router.rs     按规则展开物理表
│
├── tenant/                 租户体系
│   ├── mod.rs
│   ├── context.rs          TenantContext
│   ├── router.rs           TenantRouter（按隔离级别调整路由结果）
│   ├── rewrite.rs          apply_tenant_rewrite（注入条件 / 改表名）
│   ├── metadata.rs         TenantMetadataStore / Loader
│   ├── lifecycle.rs        onboard/offboard SQL 生成
│   └── listener.rs         PG NOTIFY 热更新
│
├── rewrite/                SQL 改写
│   ├── mod.rs              SqlRewriter trait + DefaultSqlRewriter
│   ├── table_rewrite.rs    逻辑表 → 物理表 AST 改写
│   ├── schema_rewrite.rs   schema 限定
│   ├── limit_rewrite.rs    跨分片 LIMIT 放大（取 N → 取 N+OFFSET）
│   ├── aggregate_rewrite.rs 跨分片聚合改写
│   └── encrypt_rewrite.rs  占位（encrypt 模块已删，函数 no-op）
│
├── rewrite_plugin/         插件注册体系
│   ├── mod.rs              re-export
│   ├── registry.rs         re-export summer-sql-rewrite::PluginRegistry
│   ├── context.rs          ShardingRouteInfo / TableRewritePair
│   ├── configurator.rs     ShardingRewriteConfigurator（用户注册入口）
│   └── helpers.rs          工具函数
│
├── connector/              连接抽象
│   ├── mod.rs              ShardingHint / ShardingAccessContext / 工具函数
│   ├── connection.rs       ShardingConnection（对外 API 主对象）
│   ├── connection/exec.rs  execute_with_raw / query_*_with_raw 实现
│   ├── connection/audit.rs 慢查询 / fanout 指标记录
│   ├── connection/overrides.rs with_hint / with_tenant_context
│   ├── connection/metadata.rs 租户元数据刷新
│   ├── statement.rs        StatementContext（SQL 分析结果）
│   └── transaction.rs      ShardingTransaction / 两阶段提交
│
├── datasource/             连接池
│   ├── pool.rs             DataSourcePool（多 datasource 集中管理）
│   ├── runtime.rs          运行时指标（fanout / slow_query / shard_hit）
│   ├── health.rs           健康检查
│   └── discovery.rs        启动期数据源发现
│
├── execute/                执行层
│   ├── mod.rs              Executor trait
│   ├── simple.rs           单目标直发
│   └── scatter_gather.rs   多目标并行
│
├── merge/                  跨分片合并
│   ├── mod.rs              ResultMerger trait + DefaultResultMerger
│   ├── row.rs              QueryResult 工具
│   ├── limit.rs            分页合并
│   ├── order_by.rs         排序合并
│   └── post_process.rs     占位（encrypt/masking 已删，函数 no-op）
│
└── web/                    web 集成（feature = "web"）
    ├── extractor.rs        CurrentTenant / OptionalCurrentTenant
    └── middleware.rs       TenantContextLayer + TenantShardingConnection
```

---

## 九、迁移与扩展

### 9.1 新增分片表

1. 配置文件加 `[[summer-sharding.sharding.tables]]` 条目
2. 创建对应物理表（按 `actual_tables` 模板）
3. 重启服务

### 9.2 新增分片算法

1. 在 `algorithm/` 下新增模块，实现 `ShardingAlgorithm` trait
2. 在 `AlgorithmRegistry::build` 加分支
3. 配置 `algorithm = "your_name"` 使用

### 9.3 新增业务插件

1. 在业务 crate 实现 `SqlRewritePlugin` trait
2. `app/main.rs` 启动期通过 `ShardingRewriteConfigurator::register` 注册
3. 选合适的 `order` 值，避免与内置插件冲突

### 9.4 接入新租户

1. INSERT `sys_tenant` 记录（指定隔离级别 / schema / 数据源）
2. 调用 `TenantLifecycleManager::plan_onboard()` 拿到 DDL 列表
3. 应用 DDL（创建表 / schema / 数据库）
4. PG NOTIFY 自动广播，所有进程的 `TenantMetadataStore` 热更新

---

## 十、当前限制 / 后续 TODO

| 限制 | 影响 | 应对 |
|---|---|---|
| 跨分片聚合 / 流式合并未实现 | GROUP BY 跨分片结果不准 | 业务侧避免跨分片聚合，或后续补 `merge/group_by` |
| 加密 / 脱敏 / 审计 / 读写分离 / 影子库已删 | 这些场景需要时通过插件实现 | 用 `SqlRewritePlugin` 自行扩展 |
| 内置插件示例（5 个）尚未落地 | 没现成的乐观锁等可用 | 按 §6 描述自行实现，或后续补 |
| 行级安全 (RLS) 已删 | 无法用 PG RLS 做行级隔离 | 使用 SqlRewrite 策略，或在表层做 |
| 分布式 ID 生成已删 | 应用层自行解决（snowflake/UUID 库） | 用 uuid crate 等 |
