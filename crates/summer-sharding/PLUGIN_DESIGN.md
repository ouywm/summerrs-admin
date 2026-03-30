# SQL 改写插件系统设计文档

> **版本**: v1.0
> **日期**: 2026年3月30日
> **状态**: 设计稿

---

## 一、背景与目标

`summer-sharding` 已内置了完整的 SQL 改写能力（表名替换、租户注入、LIMIT 膨胀、聚合改写、加密改写等）。但随着业务发展，外部使用者需要在 SQL Pipeline 中注入自定义逻辑，例如：

- **数据权限过滤** — 按用户/部门/角色自动追加 `WHERE` 条件
- **字段级权限** — 从 `SELECT` 中移除无权访问的列
- **动态表名替换** — 按业务规则将逻辑表名映射到不同的物理表
- **SQL 审计标记** — 向 SQL 注入追踪注释
- **自定义条件注入** — 任意 `WHERE` / `JOIN` / 子查询改写

本方案的目标是在 `DefaultSqlRewriter` 中开放一个 **通用插件入口**，让外部用户能够以类型安全、可组合的方式注册自定义 SQL 改写策略。

---

## 二、总体架构

### 2.1 Pipeline 中的位置

插件链运行在内置改写之后、SQL 渲染之前：

```
用户 SQL
  │
  ▼
Parse (sqlparser)
  │
  ▼
Analyze (StatementContext)
  │
  ▼
Route (RoutePlan)
  │
  ▼
Built-in Rewrite
  │  ├── table_rewrite    (逻辑表 → 物理表)
  │  ├── schema_rewrite   (schema 替换)
  │  ├── limit_rewrite    (LIMIT/OFFSET 膨胀)
  │  ├── aggregate_rewrite(聚合函数拆分)
  │  ├── tenant_rewrite   (多租户条件注入)
  │  └── encrypt_rewrite  (透明加密)
  │
  ▼
Plugin Rewrite Pipeline  ◀── 本方案新增
  │  plugin_1 (order=10)
  │  plugin_2 (order=20)
  │  plugin_3 (order=30)
  │  ...
  │
  ▼
SQL Render (ast.to_string())
  │
  ▼
Execute (Scatter-Gather)
  │
  ▼
Merge
```

**设计原则**：插件拿到的 AST 已完成表名替换和租户注入，是最终物理 SQL 的 AST。插件对其进行修改后，直接渲染为字符串发送给数据库。

### 2.2 模块结构

新增模块结构（注意：`extensions.rs` 为独立顶层模块，`rewrite_plugin/` 为改写插件目录，避免与现有 `plugin.rs`（Summer 框架 Plugin）冲突）：

```
src/
├── extensions.rs             // TypeMap 类型安全扩展容器（已实现 ✅）
└── rewrite_plugin/           // SQL 改写插件系统
    ├── mod.rs                // 模块入口，导出公共类型
    ├── context.rs            // RewriteContext 定义
    ├── registry.rs           // PluginRegistry（注册、排序、执行链）
    └── helpers.rs            // 高层 SQL 操作 helper 函数
```

---

## 三、核心类型设计

### 3.1 TypeMap 扩展容器（`src/extensions.rs`）

采用与 `http::Extensions` 完全一致的 TypeMap 模式，已实现并通过测试。

**源码**: [`src/extensions.rs`](src/extensions.rs)

#### 设计要点

| 特性 | 实现方式 |
|------|---------|
| **类型安全** | `TypeId` 做 key，泛型方法自动匹配，编译期保证 |
| **零成本哈希** | 自定义 `IdHasher`，`TypeId` 本身就是哈希值，`finish()` 直接返回 `u64` |
| **懒初始化** | 内部 `Option<Box<AnyMap>>`，未插入数据时仅占 1 word（8 bytes） |
| **可 Clone** | 通过 `AnyClone` trait 实现，所有存入类型需满足 `T: Clone + Send + Sync + 'static` |
| **Debug 输出类型名** | 调试时显示存储的具体类型名称，而非仅数量 |

#### 核心 API

```rust
#[derive(Clone, Default)]
pub struct Extensions { /* ... */ }

impl Extensions {
    pub fn new() -> Self;

    // 基础 CRUD
    pub fn insert<T: Clone + Send + Sync + 'static>(&mut self, val: T) -> Option<T>;
    pub fn get<T: Send + Sync + 'static>(&self) -> Option<&T>;
    pub fn get_mut<T: Send + Sync + 'static>(&mut self) -> Option<&mut T>;
    pub fn remove<T: Send + Sync + 'static>(&mut self) -> Option<T>;
    pub fn contains<T: Send + Sync + 'static>(&self) -> bool;

    // 便捷方法
    pub fn get_or_insert<T: Clone + Send + Sync + 'static>(&mut self, value: T) -> &mut T;
    pub fn get_or_insert_with<T, F>(&mut self, f: F) -> &mut T;
    pub fn get_or_insert_default<T: Default + Clone + Send + Sync + 'static>(&mut self) -> &mut T;

    // 集合操作
    pub fn extend(&mut self, other: Self);
    pub fn clear(&mut self);
    pub fn is_empty(&self) -> bool;
    pub fn len(&self) -> usize;
}
```

#### 使用示例

```rust
use summer_sharding::extensions::Extensions;

// 定义类型安全的上下文数据
#[derive(Clone)]
struct CurrentUserId(pub i64);

#[derive(Clone)]
struct CurrentDeptId(pub i64);

let mut ext = Extensions::new();
ext.insert(CurrentUserId(42));
ext.insert(CurrentDeptId(7));

// 取值 — 编译期类型安全，不可能拼错 key
let user_id = ext.get::<CurrentUserId>().unwrap().0;  // 42
let dept_id = ext.get::<CurrentDeptId>().unwrap().0;   // 7

// Clone 是独立的
let ext2 = ext.clone();
ext.insert(CurrentUserId(100));
assert_eq!(ext2.get::<CurrentUserId>().unwrap().0, 42); // 不受影响

// 合并
let mut other = Extensions::new();
other.insert(true);
ext.extend(other);
```

### 3.2 ShardingAccessContext 扩展

在现有 `ShardingAccessContext` 上新增 `extensions` 字段：

```rust
// src/connector/hint.rs

use crate::plugin::Extensions;

#[derive(Debug, Default)]
pub struct ShardingAccessContext {
    // --- 现有字段保持不变 ---
    pub roles: Vec<String>,
    pub permissions: Vec<String>,
    pub allow_skip_masking: bool,

    // --- 新增 ---
    /// 类型安全的扩展数据容器。
    /// 外部使用者可通过自定义结构体存入请求级上下文数据。
    pub extensions: Extensions,
}
```

> **注意**：由于 `Extensions` 内部包含 `Box<dyn Any>`，`ShardingAccessContext` 将不再自动 derive `Clone`、`PartialEq`、`Eq`、`Serialize`、`Deserialize`。需要手动实现必要的 trait，或在序列化时跳过 `extensions` 字段。
>
> **兼容方案**：保留 `Serialize/Deserialize` 时，为 `extensions` 加 `#[serde(skip)]`。

### 3.3 SqlRewritePlugin trait

```rust
// src/plugin/mod.rs

use crate::error::Result;
use super::context::RewriteContext;

/// SQL 改写插件 trait。
///
/// 实现此 trait 后通过 `ShardingConfig::plugin()` 注册，
/// 即可在 SQL Pipeline 中自动执行自定义改写逻辑。
pub trait SqlRewritePlugin: Send + Sync + 'static {
    /// 插件名称，用于日志输出和调试追踪
    fn name(&self) -> &str;

    /// 执行优先级。数字越小越先执行，默认 100。
    /// 建议范围：
    ///   - 0~49:   基础设施级插件（审计、追踪）
    ///   - 50~99:  安全类插件（数据权限、字段过滤）
    ///   - 100~199: 业务类插件（自定义条件、表名映射）
    ///   - 200+:   后处理插件（SQL 注释注入等）
    fn order(&self) -> i32 {
        100
    }

    /// 判断是否需要对当前 SQL 执行改写。
    /// 返回 `false` 则跳过该插件，不调用 `rewrite`。
    ///
    /// 典型用法：按 SQL 操作类型（SELECT/INSERT 等）、
    /// 目标表名、或 extensions 中的上下文数据来判断。
    fn matches(&self, ctx: &RewriteContext) -> bool;

    /// 执行改写。通过 `ctx.statement` 直接修改 AST，
    /// 或使用 `helpers` 模块提供的便捷函数操作。
    fn rewrite(&self, ctx: &mut RewriteContext) -> Result<()>;
}
```

### 3.4 RewriteContext

插件拿到的上下文对象，既提供 AST 完整控制权，也携带足够的元信息：

```rust
// src/plugin/context.rs

use sqlparser::ast::Statement;
use crate::connector::statement::StatementContext;
use crate::connector::ShardingAccessContext;
use crate::router::{RoutePlan, RouteTarget, SqlOperation};

/// 插件改写上下文。
///
/// 每个 RouteTarget（即每个物理分片）会生成一个独立的 RewriteContext，
/// 插件对其修改只影响该分片的 SQL。
pub struct RewriteContext<'a> {
    /// AST 本体（可变引用），插件可直接修改
    pub statement: &'a mut Statement,

    /// SQL 解析出的元信息（操作类型、涉及的表名、列名、条件等）
    pub analysis: &'a StatementContext,

    /// 完整路由计划
    pub route: &'a RoutePlan,

    /// 当前正在处理的路由目标（物理分片）
    pub target: &'a RouteTarget,

    /// 请求级上下文（用户角色、权限、extensions 等）
    /// 如果调用方未设置则为 None
    pub access_ctx: Option<&'a ShardingAccessContext>,
}

impl<'a> RewriteContext<'a> {
    /// 当前 SQL 操作类型
    pub fn operation(&self) -> SqlOperation {
        self.analysis.operation
    }

    /// 当前目标数据源名称
    pub fn datasource(&self) -> &str {
        &self.target.datasource
    }

    /// 当前目标的主物理表名（取第一个 table_rewrite 的 actual_table）
    pub fn current_table(&self) -> Option<&str> {
        self.target
            .table_rewrites
            .first()
            .map(|rw| rw.actual_table.table.as_str())
    }

    /// 当前目标的主逻辑表名
    pub fn logic_table(&self) -> Option<&str> {
        self.target
            .table_rewrites
            .first()
            .map(|rw| rw.logic_table.table.as_str())
    }

    /// 便捷方法：从 extensions 中获取指定类型的数据
    pub fn extension<T: Send + Sync + 'static>(&self) -> Option<&T> {
        self.access_ctx.and_then(|ctx| ctx.extensions.get::<T>())
    }

    /// 是否为 SELECT 查询
    pub fn is_select(&self) -> bool {
        self.analysis.operation == SqlOperation::Select
    }

    /// 是否为写操作（INSERT/UPDATE/DELETE）
    pub fn is_write(&self) -> bool {
        matches!(
            self.analysis.operation,
            SqlOperation::Insert | SqlOperation::Update | SqlOperation::Delete
        )
    }

    /// 是否为多分片扇出查询
    pub fn is_fanout(&self) -> bool {
        self.route.targets.len() > 1
    }
}
```

### 3.5 PluginRegistry

负责插件的注册、排序和链式执行：

```rust
// src/plugin/registry.rs

use std::sync::Arc;
use crate::error::Result;
use super::{SqlRewritePlugin, context::RewriteContext};

/// 插件注册表。
///
/// 持有所有已注册的 `SqlRewritePlugin`，按 `order()` 升序排列。
/// 在 SQL Pipeline 中由 `DefaultSqlRewriter` 调用 `rewrite_all`。
pub struct PluginRegistry {
    plugins: Vec<Arc<dyn SqlRewritePlugin>>,
}

impl PluginRegistry {
    /// 创建空注册表
    pub fn new() -> Self {
        Self {
            plugins: Vec::new(),
        }
    }

    /// 注册一个插件
    pub fn register(&mut self, plugin: impl SqlRewritePlugin) {
        self.plugins.push(Arc::new(plugin));
        // 注册后立即排序，保证执行顺序
        self.plugins.sort_by_key(|p| p.order());
    }

    /// 批量注册
    pub fn register_all(&mut self, plugins: Vec<Arc<dyn SqlRewritePlugin>>) {
        self.plugins.extend(plugins);
        self.plugins.sort_by_key(|p| p.order());
    }

    /// 按优先级顺序执行所有匹配的插件
    pub fn rewrite_all(&self, ctx: &mut RewriteContext) -> Result<()> {
        for plugin in &self.plugins {
            if plugin.matches(ctx) {
                tracing::debug!(
                    plugin = plugin.name(),
                    order = plugin.order(),
                    "applying rewrite plugin"
                );
                plugin.rewrite(ctx)?;
            }
        }
        Ok(())
    }

    /// 已注册的插件数量
    pub fn len(&self) -> usize {
        self.plugins.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for PluginRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginRegistry")
            .field(
                "plugins",
                &self
                    .plugins
                    .iter()
                    .map(|p| format!("{}(order={})", p.name(), p.order()))
                    .collect::<Vec<_>>(),
            )
            .finish()
    }
}
```

---

## 四、Helper 函数（`helpers.rs`）

为不需要直接操作 AST 的使用者提供高层便捷 API：

```rust
// src/plugin/helpers.rs

use sqlparser::ast::*;

/// 在 SELECT/UPDATE/DELETE 语句的 WHERE 子句后追加 AND 条件。
/// 如果原 SQL 没有 WHERE，则创建 WHERE 子句。
///
/// # 示例
/// ```rust
/// let condition = helpers::build_eq_expr("create_by", "123");
/// helpers::append_where(statement, condition);
/// // SELECT * FROM t → SELECT * FROM t WHERE create_by = '123'
/// // SELECT * FROM t WHERE x = 1 → SELECT * FROM t WHERE x = 1 AND create_by = '123'
/// ```
pub fn append_where(statement: &mut Statement, condition: Expr);

/// 构建 `column = 'value'` 表达式
pub fn build_eq_expr(column: &str, value: &str) -> Expr;

/// 构建 `column = number` 表达式（数值类型）
pub fn build_eq_int_expr(column: &str, value: i64) -> Expr;

/// 构建 `column IN ('v1', 'v2', ...)` 表达式
pub fn build_in_expr(column: &str, values: &[&str]) -> Expr;

/// 构建 `column IN (1, 2, ...)` 表达式（数值类型）
pub fn build_in_int_expr(column: &str, values: &[i64]) -> Expr;

/// 构建 `column IS NULL` 表达式
pub fn build_is_null_expr(column: &str) -> Expr;

/// 构建 `column IS NOT NULL` 表达式
pub fn build_is_not_null_expr(column: &str) -> Expr;

/// 构建 `column BETWEEN low AND high` 表达式
pub fn build_between_expr(column: &str, low: &str, high: &str) -> Expr;

/// 构建 `EXISTS (subquery)` 表达式
pub fn build_exists_expr(subquery: Query) -> Expr;

/// 替换 FROM 子句中的表名。
/// 将所有出现的 `from_table` 替换为 `to_table`。
pub fn replace_table(statement: &mut Statement, from_table: &str, to_table: &str);

/// 从 SELECT 投影列表中移除指定列。
/// 常用于字段级权限控制。
pub fn remove_columns(statement: &mut Statement, columns: &[&str]);

/// 将原始 SQL 包裹为子查询。
/// `SELECT * FROM t WHERE x = 1` → `SELECT * FROM (SELECT * FROM t WHERE x = 1) AS alias`
pub fn wrap_subquery(statement: &mut Statement, alias: &str);

/// 向 SQL 添加审计注释（用于审计追踪）。
/// 由于 sqlparser AST 不支持任意注释节点，通过 `RewriteContext::append_comment()` 收集注释，
/// 由 `DefaultSqlRewriter` 在 `to_string()` 后以 `/* comment */` 形式拼接到 SQL 末尾。
///
/// 插件中使用方式：`ctx.append_comment("user_id=42");`
///
/// 底层渲染函数：
pub fn format_with_comments(sql: &str, comments: &[String]) -> String;

/// 两个 Expr 用 AND 连接
pub fn and(left: Expr, right: Expr) -> Expr;

/// 两个 Expr 用 OR 连接（自动加括号）
pub fn or(left: Expr, right: Expr) -> Expr;
```

---

## 五、集成方案

### 5.1 设计原则

插件的 **定义** 在 `summer-sharding` 库内部（trait + registry），
插件的 **注册** 在应用层 `main.rs` 中完成，
与 `SummerAuthConfigurator` / `.auth_configure(...)` 完全同一模式。

**库内部不硬编码任何具体插件**，只提供机制。

### 5.2 应用层使用方式（main.rs）

```rust
use summer::App;
use summer_sharding::{SummerShardingPlugin, ShardingRewriteConfigurator};

// 应用层定义的自定义插件
use crate::plugins::{DataScopePlugin, AuditCommentPlugin};

#[tokio::main]
async fn main() {
    App::new()
        .add_plugin(SummerShardingPlugin)
        // 注册 SQL 改写插件 — 和 .auth_configure(...) 同一风格
        .sharding_rewrite_configure(|registry| {
            registry
                .register(DataScopePlugin::new())
                .register(AuditCommentPlugin::new())
        })
        // ... 其他插件 ...
        .run()
        .await;
}
```

### 5.3 `ShardingRewriteConfigurator` trait

定义在 `summer-sharding` 库中，为 `AppBuilder` 扩展链式方法：

```rust
// summer-sharding/src/rewrite_plugin/configurator.rs

use summer::app::AppBuilder;
use summer::plugin::MutableComponentRegistry;
use super::registry::PluginRegistry;

/// 扩展 `AppBuilder`，提供 SQL 改写插件注册入口。
///
/// 与 `SummerAuthConfigurator` 同一模式：
/// 在应用层 `main.rs` 中通过链式调用注册插件，
/// 而非修改 `summer-sharding` 库内部代码。
pub trait ShardingRewriteConfigurator {
    /// 注册 SQL 改写插件。
    ///
    /// 接收一个闭包，闭包参数为 `&mut PluginRegistry`，
    /// 在闭包内通过 `registry.register(...)` 注册插件。
    ///
    /// # 示例
    ///
    /// ```rust
    /// app.sharding_rewrite_configure(|registry| {
    ///     registry
    ///         .register(MyPlugin1::new())
    ///         .register(MyPlugin2::new())
    /// });
    /// ```
    fn sharding_rewrite_configure<F>(&mut self, f: F) -> &mut Self
    where
        F: FnOnce(&mut PluginRegistry) -> &mut PluginRegistry;
}

impl ShardingRewriteConfigurator for AppBuilder {
    fn sharding_rewrite_configure<F>(&mut self, f: F) -> &mut Self
    where
        F: FnOnce(&mut PluginRegistry) -> &mut PluginRegistry,
    {
        // 从 AppBuilder 中取出已有的 registry（如果有的话），
        // 没有则创建新的
        let mut registry = self
            .get_component::<PluginRegistry>()
            .unwrap_or_default();

        f(&mut registry);

        // 存回 AppBuilder，供 SummerShardingPlugin::build() 取用
        self.add_component(registry)
    }
}
```

### 5.4 `SummerShardingPlugin` 取用 PluginRegistry

在 `SummerShardingPlugin::build()` 中，从 `AppBuilder` 取出 `PluginRegistry` 并注入到 `ShardingConnection`：

```rust
// summer-sharding/src/plugin.rs (现有文件，修改 build 方法)

#[async_trait]
impl Plugin for SummerShardingPlugin {
    async fn build(&self, app: &mut AppBuilder) {
        // ... 现有逻辑不变 ...

        let connection = ShardingConnection::build(/* ... */).await.unwrap();

        // 新增：从 AppBuilder 取出用户注册的插件（如果有）
        let plugin_registry = app
            .get_component::<PluginRegistry>()
            .unwrap_or_default();

        if !plugin_registry.is_empty() {
            connection.set_plugin_registry(Arc::new(plugin_registry));
            tracing::info!(
                "sharding rewrite plugins registered: {}",
                connection.plugin_summary()
            );
        }

        // ... 后续逻辑不变 ...
        app.add_component(connection);
    }
}
```

### 5.5 数据流总览

```
main.rs                          summer-sharding 库
─────────                        ──────────────────
                                 
App::new()                       
  │                              
  ├── .add_plugin(SummerShardingPlugin)
  │                              
  ├── .sharding_rewrite_configure(|registry| {
  │       registry                     
  │         .register(Plugin1)   ──→  PluginRegistry
  │         .register(Plugin2)        存入 AppBuilder
  │   })                              (as Component)
  │                              
  ├── .run()                     
  │   └── SummerShardingPlugin::build()
  │       ├── ShardingConnection::build(config)
  │       ├── app.get_component::<PluginRegistry>()  ◀── 取出
  │       ├── connection.set_plugin_registry(registry)
  │       └── app.add_component(connection)
  │                              
  ▼                              
请求到达时：                       
  SQL → Parse → Route → Rewrite  
                           │     
                           ├── 内置改写（表名、租户、LIMIT等）
                           │     
                           └── PluginRegistry.rewrite_all()
                                 ├── Plugin1.rewrite()
                                 └── Plugin2.rewrite()
```

### 5.6 `ShardingConnectionInner` 持有 PluginRegistry

```rust
pub(crate) struct ShardingConnectionInner {
    config: Arc<ShardingConfig>,
    router: Arc<dyn SqlRouter>,
    rewriter: Arc<DefaultSqlRewriter>,
    plugin_registry: Option<Arc<PluginRegistry>>,  // 新增（可选）
    // ... 其他字段
}

impl ShardingConnection {
    /// 设置 SQL 改写插件注册表（由 SummerShardingPlugin 调用）
    pub(crate) fn set_plugin_registry(&self, registry: Arc<PluginRegistry>) {
        // 通过内部可变性设置
        self.inner.plugin_registry.store(Some(registry));
    }

    /// 获取已注册插件的摘要信息（用于日志）
    pub fn plugin_summary(&self) -> String {
        self.inner
            .plugin_registry
            .as_ref()
            .map(|r| format!("{:?}", r))
            .unwrap_or_else(|| "none".to_string())
    }
}
```

### 5.7 DefaultSqlRewriter 集成

在 `DefaultSqlRewriter` 中，内置改写完成后调用插件链：

```rust
// 在 rewrite 方法中，内置改写完成后
impl DefaultSqlRewriter {
    fn rewrite_statement(
        &self,
        parsed: &mut sqlparser::ast::Statement,
        analysis: &StatementContext,
        plan: &RoutePlan,
        target: &RouteTarget,
        plugin_registry: Option<&PluginRegistry>,
    ) -> Result<()> {
        // 1. 内置改写（保持不变）
        for rewrite in &target.table_rewrites {
            rewrite_table_names(parsed, &rewrite.logic_table, &rewrite.actual_table);
            apply_schema_rewrite(parsed, &rewrite.logic_table, &rewrite.actual_table);
        }
        // ... 其他内置改写 ...

        // 2. 插件链改写（新增）
        if let Some(registry) = plugin_registry {
            let mut ctx = RewriteContext {
                statement: parsed,
                analysis,
                route: plan,
                target,
                access_ctx: analysis.access_context.as_ref(),
            };
            registry.rewrite_all(&mut ctx)?;
        }

        Ok(())
    }
}
```

---

## 六、现有文件改动清单

| 文件 | 改动内容 | 状态 |
|------|---------|------|
| `src/lib.rs` | 新增 `pub mod extensions;` 和 `pub mod rewrite_plugin;`；新增公开 re-exports | ✅ 已完成 |
| `src/extensions.rs` | TypeMap 扩展容器（独立顶层模块） | ✅ 已实现，16 项测试全部通过 |
| `src/connector/hint.rs` | `ShardingAccessContext` 新增 `extensions: Extensions` 字段；为 `extensions` 加 `#[serde(skip)]`；新增 `with_extension<T>` 和 `extension<T>` 方法 | ✅ 已完成 |
| `src/connector/connection.rs` | `ShardingConnectionInner` 新增 `plugin_registry: Option<Arc<PluginRegistry>>` 字段；新增 `set_plugin_registry()` 方法；`prepare_statement` 传递 `plugin_registry` 到 rewriter | ✅ 已完成 |
| `src/plugin.rs` | `SummerShardingPlugin::build()` 中从 `AppBuilder` 取出 `PluginRegistry` 注入到 `ShardingConnection` | ✅ 已完成 |
| `src/rewrite/mod.rs` | `SqlRewriter` trait 增加 `plugin_registry` 参数；`DefaultSqlRewriter` 在内置改写后调用 `registry.rewrite_all()` | ✅ 已完成 |
| `src/error.rs` | `ShardingError` 新增 `Plugin { plugin, message }` 变体 | ✅ 已完成 |
| `Cargo.toml` | 无新增依赖（`std::any::TypeId` 属标准库） | ✅ 无需改动 |

新增插件模块文件（5 个）：

| 文件 | 内容 | 状态 |
|------|------|------|
| `src/rewrite_plugin/mod.rs` | 模块入口，导出 `SqlRewritePlugin`、`RewriteContext`、`PluginRegistry`、`helpers`、`ShardingRewriteConfigurator` | ✅ 已完成 |
| `src/rewrite_plugin/context.rs` | `RewriteContext` 结构体及其便捷方法 | ✅ 已完成 |
| `src/rewrite_plugin/registry.rs` | `PluginRegistry` 注册、排序与链式执行 | ✅ 已完成 |
| `src/rewrite_plugin/helpers.rs` | Helper 函数集（`append_where`、`build_eq_expr` 等，14 项测试全部通过） | ✅ 已完成 |
| `src/rewrite_plugin/configurator.rs` | `ShardingRewriteConfigurator` trait，为 `AppBuilder` 扩展 `.sharding_rewrite_configure()` 方法 | ✅ 已完成 |

---

## 七、使用示例

### 7.1 数据权限 — 仅查看本人数据

```rust
use summer_sharding::{SqlRewritePlugin, RewriteContext, rewrite_helpers as helpers};

/// 请求级上下文：当前登录用户
struct CurrentUser {
    pub id: i64,
    pub dept_id: i64,
    pub data_scope: DataScope,
}

#[derive(Clone, Copy)]
enum DataScope {
    All,            // 全部数据
    Dept,           // 仅本部门
    DeptAndBelow,   // 本部门及下级
    SelfOnly,       // 仅本人
}

/// 数据权限插件
struct DataScopePlugin;

impl SqlRewritePlugin for DataScopePlugin {
    fn name(&self) -> &str { "data_scope" }
    fn order(&self) -> i32 { 50 }

    fn matches(&self, ctx: &RewriteContext) -> bool {
        // 只对 SELECT 生效，且上下文中有 CurrentUser
        ctx.is_select() && ctx.extension::<CurrentUser>().is_some()
    }

    fn rewrite(&self, ctx: &mut RewriteContext) -> summer_sharding::error::Result<()> {
        let user = ctx.extension::<CurrentUser>().unwrap();

        match user.data_scope {
            DataScope::All => { /* 不追加任何条件 */ }
            DataScope::SelfOnly => {
                let condition = helpers::build_eq_int_expr("create_by", user.id);
                helpers::append_where(ctx.statement, condition);
            }
            DataScope::Dept => {
                let condition = helpers::build_eq_int_expr("dept_id", user.dept_id);
                helpers::append_where(ctx.statement, condition);
            }
            DataScope::DeptAndBelow => {
                // 假设 dept_ids 已提前查好
                let dept_ids = vec![user.dept_id, /* ...子部门 */];
                let condition = helpers::build_in_int_expr("dept_id", &dept_ids);
                helpers::append_where(ctx.statement, condition);
            }
        }
        Ok(())
    }
}
```

### 7.2 注册与使用

#### Step 1：在 main.rs 注册插件

```rust
use summer::App;
use summer_sharding::{SummerShardingPlugin, ShardingRewriteConfigurator};

#[tokio::main]
async fn main() {
    App::new()
        .add_plugin(SummerShardingPlugin)
        // 在应用层注册 — 不修改 summer-sharding 库代码
        .sharding_rewrite_configure(|registry| {
            registry.register(DataScopePlugin)
        })
        .run()
        .await;
}
```

#### Step 2：在业务接口中传入上下文

```rust
// 在 axum handler 中（或任何业务代码中）
async fn list_orders(conn: &ShardingConnection) -> Result<Vec<Order>> {
    let user = CurrentUser {
        id: 42,
        dept_id: 7,
        data_scope: DataScope::SelfOnly,
    };

    let access_ctx = ShardingAccessContext::default()
        .with_role("user")
        .with_extension(user);  // 类型安全地存入

    // 传入上下文
    let conn = with_access_context(&conn, access_ctx);
    let results = Entity::find().all(&conn).await?;
    // 实际执行的 SQL:
    // SELECT * FROM ai.log_202603 WHERE ... AND create_by = 42
    Ok(results)
}
```

### 7.3 SQL 审计注释插件

```rust
struct AuditCommentPlugin;

impl SqlRewritePlugin for AuditCommentPlugin {
    fn name(&self) -> &str { "audit_comment" }
    fn order(&self) -> i32 { 200 }  // 最后执行

    fn matches(&self, _ctx: &RewriteContext) -> bool {
        true  // 所有 SQL 都加注释
    }

    fn rewrite(&self, ctx: &mut RewriteContext) -> Result<()> {
        if let Some(user) = ctx.extension::<CurrentUser>() {
            ctx.append_comment(
                &format!("user_id={}, datasource={}", user.id, ctx.datasource()),
            );
        }
        Ok(())
    }
}
```

### 7.4 动态表名替换插件

```rust
struct DynamicTablePlugin {
    mapping: HashMap<String, String>,
}

impl SqlRewritePlugin for DynamicTablePlugin {
    fn name(&self) -> &str { "dynamic_table" }
    fn order(&self) -> i32 { 100 }

    fn matches(&self, ctx: &RewriteContext) -> bool {
        ctx.logic_table()
            .is_some_and(|t| self.mapping.contains_key(t))
    }

    fn rewrite(&self, ctx: &mut RewriteContext) -> Result<()> {
        if let Some(logic) = ctx.logic_table() {
            if let Some(target) = self.mapping.get(logic) {
                helpers::replace_table(ctx.statement, logic, target);
            }
        }
        Ok(())
    }
}
```

---

## 八、ShardingAccessContext builder 扩展方法

为保持 API 一致性，为 `ShardingAccessContext` 新增链式方法：

```rust
impl ShardingAccessContext {
    // --- 现有方法保持不变 ---

    /// 存入一个类型安全的扩展数据
    pub fn with_extension<T: Send + Sync + 'static>(mut self, val: T) -> Self {
        self.extensions.insert(val);
        self
    }

    /// 获取扩展数据的不可变引用
    pub fn extension<T: Send + Sync + 'static>(&self) -> Option<&T> {
        self.extensions.get::<T>()
    }
}
```

---

## 九、错误处理

`ShardingError` 新增 `Plugin` 变体：

```rust
#[derive(Debug, Error)]
pub enum ShardingError {
    // ... 现有变体 ...

    #[error("plugin `{plugin}` rewrite error: {message}")]
    Plugin {
        plugin: String,
        message: String,
    },
}
```

`PluginRegistry::rewrite_all` 在捕获到错误时，自动包装为 `ShardingError::Plugin`，附带插件名称，便于定位问题。

---

## 十、设计决策记录

| 决策 | 选项 | 结论 | 理由 |
|------|------|------|------|
| Extensions 类型 | HashMap\<String,String\> / 泛型\<E\> / TypeMap | **TypeMap** | 类型安全、零侵入、Rust 生态惯例（对齐 `http::Extensions`） |
| 插件编排方式 | 注册顺序 / 优先级 | **优先级 (order)** | 注册顺序不可控，优先级更明确 |
| 插件能力边界 | 仅追加条件 / 仅 AST / 两层都给 | **两层都给 (C)** | AST 给完整控制力，helpers 降低使用门槛 |
| 上下文传递 | 独立类型 / 复用 AccessContext | **复用 + extensions** | 不增加新概念，与现有 API 一致 |
| 插件注册位置 | ShardingConfig / ShardingConnection / **AppBuilder Configurator** | **AppBuilder Configurator** | 与 `SummerAuthConfigurator` 同一模式；插件在应用层注册，不修改库代码；trait object 不可序列化，不适合放 Config |
| 插件执行时机 | 内置改写之前 / 之后 | **之后** | 插件拿到稳定的物理 SQL，避免与内置逻辑冲突 |
