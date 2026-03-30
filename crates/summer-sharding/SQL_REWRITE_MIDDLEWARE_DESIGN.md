# SQL 改写中间件层设计文档

> **版本**: v1.0
> **日期**: 2026年3月30日
> **状态**: 设计稿
> **前置文档**: `PLUGIN_DESIGN.md`（当前已实现的 sharding 内嵌插件系统）

---

## 一、背景与动机

### 1.1 现状

当前的 SQL 改写插件系统（`rewrite_plugin`）**绑死在 `ShardingConnection` 管道内**：

```
ShardingConnection.execute_raw()
  → analyze_statement()      ← SQL 解析
  → router.route()           ← 分片路由
  → rewriter.rewrite()       ← 内置改写 + 插件链
  → executor.execute()       ← 散射执行
  → merger.merge()           ← 结果合并
```

这意味着**只有走 `ShardingConnection` 的查询才能享受插件改写**。对于不需要分片的普通业务表（如 `sys_user`、`sys_menu`），要么强制走 `ShardingConnection`（概念不匹配），要么放弃插件能力。

### 1.2 目标

将 SQL 改写插件抽取为**独立的中间件层**，使其：

1. **脱离分片依赖** — 不需要 `RoutePlan`、`RouteTarget` 等分片概念即可运行
2. **适用于任意连接** — 普通 `DatabaseConnection` 和 `ShardingConnection` 都能挂载
3. **保持现有能力** — `ShardingConnection` 场景下仍可访问分片上下文（作为可选扩展）
4. **零侵入** — 不修改 SeaORM 本身，通过 wrapper/decorator 模式实现

---

## 二、整体架构

### 2.1 新增 crate

```
crates/
├── summer-sql-rewrite/          ← 新 crate：独立 SQL 改写中间件
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── context.rs           ← SqlRewriteContext（通用版）
│       ├── plugin.rs            ← SqlRewritePlugin trait（通用版）
│       ├── registry.rs          ← PluginRegistry（通用版）
│       ├── extensions.rs        ← Extensions TypeMap（从 sharding 迁移）
│       ├── helpers.rs           ← SQL AST 操作工具函数（从 sharding 迁移）
│       ├── connection.rs        ← RewriteConnection 包装器
│       ├── error.rs             ← 错误类型
│       └── configurator.rs      ← Summer 框架集成
│
├── summer-sharding/             ← 现有 crate：依赖 summer-sql-rewrite
│   └── src/
│       ├── rewrite_plugin/      ← 瘦化：re-export + 分片扩展
│       └── ...
```

### 2.2 依赖关系

```
summer-sql-rewrite（独立，无分片概念）
    ↑ 依赖
    ├── summer-sharding（使用 + 扩展分片上下文）
    └── 其他业务 crate（直接使用）
```

```toml
# summer-sql-rewrite/Cargo.toml
[dependencies]
sea-orm = { workspace = true }
sqlparser = { workspace = true }
tracing = { workspace = true }
thiserror = { workspace = true }
# 无 summer 框架依赖（core library）

[features]
summer = ["dep:summer"]      # 可选：Summer 框架集成（Configurator）

[dependencies.summer]
workspace = true
optional = true
```

---

## 三、核心类型设计

### 3.1 SqlRewriteContext — 通用改写上下文

**设计原则**：只包含 SQL 改写所需的最小信息集，不引入任何分片概念。

```rust
// summer-sql-rewrite/src/context.rs

use sqlparser::ast::Statement as AstStatement;

/// SQL 操作类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqlOperation {
    Select,
    Insert,
    Update,
    Delete,
    Other,
}

/// SQL 改写上下文（通用版）。
///
/// 与 `summer-sharding` 的 `RewriteContext` 不同，本结构体不依赖
/// `RoutePlan`/`RouteTarget` 等分片概念，可用于任意数据库连接。
pub struct SqlRewriteContext<'a> {
    /// AST 本体（可变引用），插件可直接修改
    pub statement: &'a mut AstStatement,

    /// SQL 操作类型
    pub operation: SqlOperation,

    /// 涉及的表名列表（schema.table 格式）
    pub tables: Vec<String>,

    /// 原始 SQL 文本（改写前）
    pub original_sql: &'a str,

    /// 类型安全的扩展容器。
    /// 用于跨插件传递数据，或由上层（Web 拦截器、Sharding 管道）注入业务上下文。
    pub extensions: &'a Extensions,

    /// 审计注释列表
    pub comments: Vec<String>,
}
```

**便捷方法**：

```rust
impl<'a> SqlRewriteContext<'a> {
    /// 是否为 SELECT 查询
    pub fn is_select(&self) -> bool { ... }

    /// 是否为写操作（INSERT/UPDATE/DELETE）
    pub fn is_write(&self) -> bool { ... }

    /// 从 extensions 获取指定类型数据
    pub fn extension<T: Send + Sync + 'static>(&self) -> Option<&T> { ... }

    /// 追加审计注释
    pub fn append_comment(&mut self, comment: &str) { ... }

    /// 主表名（第一个表）
    pub fn primary_table(&self) -> Option<&str> { ... }
}
```

### 3.2 SqlRewritePlugin — 通用插件 trait

```rust
// summer-sql-rewrite/src/plugin.rs

/// SQL 改写插件（通用版）。
///
/// 与 sharding 版的区别：`matches()` 和 `rewrite()` 接收的是
/// `SqlRewriteContext`（无分片字段），而非 `RewriteContext`。
pub trait SqlRewritePlugin: Send + Sync + 'static {
    /// 插件名称
    fn name(&self) -> &str;

    /// 执行优先级（越小越先执行，默认 100）
    fn order(&self) -> i32 { 100 }

    /// 判断是否需要改写
    fn matches(&self, ctx: &SqlRewriteContext) -> bool;

    /// 执行改写
    fn rewrite(&self, ctx: &mut SqlRewriteContext) -> Result<()>;
}
```

### 3.3 PluginRegistry — 通用注册表

与现有实现几乎一致，只是操作 `SqlRewriteContext` 而非 `RewriteContext`：

```rust
// summer-sql-rewrite/src/registry.rs

pub struct PluginRegistry {
    plugins: Vec<Arc<dyn SqlRewritePlugin>>,
}

impl PluginRegistry {
    pub fn new() -> Self { ... }
    pub fn register(&mut self, plugin: impl SqlRewritePlugin) -> &mut Self { ... }
    pub fn rewrite_all(&self, ctx: &mut SqlRewriteContext) -> Result<()> { ... }
    pub fn len(&self) -> usize { ... }
    pub fn is_empty(&self) -> bool { ... }
    pub fn summary(&self) -> String { ... }
}
```

### 3.4 Extensions — 类型安全扩展容器

直接从 `summer-sharding/src/extensions.rs` 迁移，无需修改：

```rust
// summer-sql-rewrite/src/extensions.rs
// （完整迁移现有实现：IdHasher, AnyClone, Extensions）
```

### 3.5 helpers — SQL AST 工具函数

从 `summer-sharding/src/rewrite_plugin/helpers.rs` 迁移。大部分函数是通用的：

| 函数 | 是否通用 | 说明 |
|------|----------|------|
| `append_where` | ✅ 通用 | |
| `build_eq_expr` | ✅ 通用 | |
| `build_eq_int_expr` | ✅ 通用 | |
| `build_in_expr` | ✅ 通用 | |
| `build_in_int_expr` | ✅ 通用 | |
| `build_not_in_expr` | ✅ 通用 | |
| `build_is_null_expr` | ✅ 通用 | |
| `build_between_expr` | ✅ 通用 | |
| `build_like_expr` | ✅ 通用 | |
| `build_exists_expr` | ✅ 通用 | |
| `and` / `or` | ✅ 通用 | |
| `wrap_subquery` | ✅ 通用 | |
| `format_with_comments` | ✅ 通用 | |
| `replace_table` | ⚠️ 需调整 | 核心 API 保持 `QualifiedTableName`，额外提供 `&str` 便利重载 |

---

## 四、RewriteConnection — 连接包装器

### 4.1 设计思路

实现 SeaORM 的 `ConnectionTrait`，作为装饰器包裹任意 `DatabaseConnection`：

```
用户代码
  → RewriteConnection (SQL 改写)
    → DatabaseConnection (实际执行)
```

### 4.2 结构定义

```rust
// summer-sql-rewrite/src/connection.rs

use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, DbErr, ExecResult, QueryResult, Statement};

/// SQL 改写连接包装器。
///
/// 包裹一个标准 `DatabaseConnection`，在执行前自动运行 SQL 改写插件链。
///
/// # 示例
///
/// ```rust,ignore
/// let db: DatabaseConnection = Database::connect("postgres://...").await?;
/// let rewrite_conn = RewriteConnection::new(db, registry, extensions);
///
/// // 通过 rewrite_conn 执行的所有 SQL 都会经过插件改写
/// let users = User::find().all(&rewrite_conn).await?;
/// ```
pub struct RewriteConnection {
    /// 底层实际连接
    inner: DatabaseConnection,
    /// 插件注册表
    registry: Arc<PluginRegistry>,
    /// 请求级扩展上下文（用户信息、权限等）
    extensions: Arc<Extensions>,
}
```

### 4.3 ConnectionTrait 实现

```rust
#[async_trait::async_trait]
impl ConnectionTrait for RewriteConnection {
    fn get_database_backend(&self) -> DbBackend {
        self.inner.get_database_backend()
    }

    async fn execute_raw(&self, stmt: Statement) -> Result<ExecResult, DbErr> {
        let rewritten = self.rewrite_statement(stmt)?;
        self.inner.execute_raw(rewritten).await
    }

    async fn execute_unprepared(&self, sql: &str) -> Result<ExecResult, DbErr> {
        let rewritten_sql = self.rewrite_sql(sql)?;
        self.inner.execute_unprepared(&rewritten_sql).await
    }

    async fn query_one_raw(&self, stmt: Statement) -> Result<Option<QueryResult>, DbErr> {
        let rewritten = self.rewrite_statement(stmt)?;
        self.inner.query_one_raw(rewritten).await
    }

    async fn query_all_raw(&self, stmt: Statement) -> Result<Vec<QueryResult>, DbErr> {
        let rewritten = self.rewrite_statement(stmt)?;
        self.inner.query_all_raw(rewritten).await
    }
}
```

### 4.4 核心改写逻辑

```rust
impl RewriteConnection {
    /// 创建改写连接
    pub fn new(
        inner: DatabaseConnection,
        registry: Arc<PluginRegistry>,
        extensions: Arc<Extensions>,
    ) -> Self {
        Self { inner, registry, extensions }
    }

    /// 仅替换 extensions（用于请求级上下文切换）
    pub fn with_extensions(&self, extensions: Arc<Extensions>) -> Self {
        Self {
            inner: self.inner.clone(),
            registry: self.registry.clone(),
            extensions,
        }
    }

    /// 改写 Statement
    fn rewrite_statement(&self, stmt: Statement) -> Result<Statement, DbErr> {
        if self.registry.is_empty() {
            return Ok(stmt);
        }

        let sql = &stmt.sql;

        // 1. 解析 SQL 为 AST
        let dialect = PostgreSqlDialect {};
        let mut ast = Parser::parse_sql(&dialect, sql)
            .map_err(|e| DbErr::Custom(format!("SQL parse error: {e}")))?;

        if ast.is_empty() {
            return Ok(stmt);
        }

        let mut parsed = ast.remove(0);

        // 2. 提取元信息
        let operation = detect_operation(&parsed);
        let tables = extract_tables(&parsed);

        // 3. 构建改写上下文并执行插件链
        let mut ctx = SqlRewriteContext {
            statement: &mut parsed,
            operation,
            tables,
            original_sql: sql,
            extensions: &self.extensions,
            comments: Vec::new(),
        };

        self.registry.rewrite_all(&mut ctx)
            .map_err(|e| DbErr::Custom(e.to_string()))?;

        let comments = ctx.comments;

        // 4. 渲染改写后的 SQL
        let new_sql = helpers::format_with_comments(&parsed.to_string(), &comments);

        Ok(Statement {
            sql: new_sql,
            values: stmt.values,
            db_backend: stmt.db_backend,
        })
    }
}
```

### 4.5 SQL 轻量解析

为 `RewriteConnection` 提供独立的 SQL 解析函数（不依赖 sharding 的 `analyze_statement`）：

```rust
/// 检测 SQL 操作类型（轻量版，不做深度分析）
fn detect_operation(stmt: &AstStatement) -> SqlOperation {
    match stmt {
        AstStatement::Query(_) => SqlOperation::Select,
        AstStatement::Insert(_) => SqlOperation::Insert,
        AstStatement::Update { .. } => SqlOperation::Update,
        AstStatement::Delete(_) => SqlOperation::Delete,
        _ => SqlOperation::Other,
    }
}

/// 提取 SQL 中涉及的表名
fn extract_tables(stmt: &AstStatement) -> Vec<String> {
    // 遍历 AST 提取 FROM/JOIN/UPDATE/INSERT INTO 中的表名
    // 简化版实现，不需要 sharding 的完整解析能力
    ...
}
```

---

## 五、与 ShardingConnection 的集成

### 5.1 sharding 内的 rewrite_plugin 模块瘦化

`summer-sharding` 的 `rewrite_plugin/` 模块改为：
- **re-export** `summer-sql-rewrite` 的核心类型
- **扩展**分片特有的上下文

```rust
// summer-sharding/src/rewrite_plugin/mod.rs

// 直接 re-export 通用类型
pub use summer_sql_rewrite::{
    SqlRewritePlugin, PluginRegistry, Extensions,
    helpers, SqlRewriteContext, SqlOperation,
};

// 分片扩展上下文
pub mod sharding_context;
```

### 5.2 ShardingRewriteContext — 分片扩展

在 `summer-sharding` 中提供一个扩展版上下文，通过 `Extensions` 注入分片信息：

```rust
// summer-sharding/src/rewrite_plugin/sharding_context.rs

/// 分片路由信息（注入到 Extensions 中供插件读取）
#[derive(Clone, Debug)]
pub struct ShardingRouteInfo {
    /// 当前目标数据源
    pub datasource: String,
    /// 逻辑表 → 物理表映射
    pub table_rewrites: Vec<(String, String)>,
    /// 是否为多分片扇出
    pub is_fanout: bool,
}

// 插件中使用方式：
// if let Some(route_info) = ctx.extension::<ShardingRouteInfo>() {
//     println!("datasource: {}", route_info.datasource);
// }
```

### 5.3 DefaultSqlRewriter 中的适配

`DefaultSqlRewriter::rewrite()` 内部构建 `SqlRewriteContext`（而非原来的 `RewriteContext`），并将分片信息注入 `Extensions`：

```rust
// summer-sharding/src/rewrite/mod.rs（修改后）

// 插件链改写
if let Some(registry) = plugin_registry {
    // 将分片信息注入 extensions
    let mut ext = extensions.clone(); // 从 access_ctx 获取
    ext.insert(ShardingRouteInfo {
        datasource: target.datasource.clone(),
        table_rewrites: target.table_rewrites.iter()
            .map(|rw| (rw.logic_table.full_name(), rw.actual_table.full_name()))
            .collect(),
        is_fanout: plan.targets.len() > 1,
    });

    let mut ctx = SqlRewriteContext {
        statement: &mut parsed,
        operation: SqlOperation::from(analysis.operation),
        tables: analysis.tables.iter().map(|t| t.full_name()).collect(),
        original_sql: &stmt.sql,
        extensions: &ext,
        comments: Vec::new(),
    };

    registry.rewrite_all(&mut ctx)?;
    comments = ctx.comments;
}
```

---

## 六、使用场景

### 6.1 纯 SQL 改写（独立库模式，不依赖 Summer 框架）

这一节展示 `summer-sql-rewrite` 作为**普通 Rust 库**单独使用时的最小示例。
如果是在 `summerrs-admin` / Summer 框架中接入，数据库连接不应在业务代码中手动
`Database::connect(...)`，而应由 `SeaOrmPlugin` 创建，再在 `SummerSqlRewritePlugin::build()`
里通过 `AppBuilder` 获取并包装。

```rust
// main.rs
use summer_sql_rewrite::{RewriteConnection, PluginRegistry, Extensions};

// 1. 创建插件注册表
let mut registry = PluginRegistry::new();
registry.register(DataPermissionPlugin);
registry.register(AuditCommentPlugin);
let registry = Arc::new(registry);

// 2. 创建普通数据库连接
let db = Database::connect("postgres://localhost/mydb").await?;

// 3. 包装为改写连接
let conn = RewriteConnection::new(db, registry, Arc::new(Extensions::new()));

// 4. 正常使用 SeaORM API
let users = User::find().all(&conn).await?;
// 实际执行的 SQL: SELECT * FROM users WHERE create_by = '42' /* user_id=42 */
```

#### 6.1.1 Summer 框架内的真实接法（贴近 summerrs-admin）

在 Summer 中推荐的生命周期应该是：

1. `SeaOrmPlugin` 在启动时创建 `DatabaseConnection`
2. `SummerSqlRewritePlugin::build()` 从 `AppBuilder` 里取出 `DatabaseConnection`
3. 构建全局 `RewriteConnection` 组件
4. 如果启用 Web，再通过 `app.add_router_layer(...)` 注册请求级 `SqlRewriteLayer`
5. `SqlRewriteLayer` 在请求进入时自动读取 `request.extensions` 里的认证上下文，并构建请求级 `RewriteConnection`

```rust
// summer-sql-rewrite/src/plugin.rs（规划）

use sea_orm::DatabaseConnection;
use summer::app::AppBuilder;
use summer::plugin::{ComponentRegistry, MutableComponentRegistry, Plugin};
use summer_web::LayerConfigurator;

use crate::{
    Extensions, PluginRegistry, RewriteConnection,
    web::{SqlRewriteLayer, SqlRewriteRequestExtender},
};

pub struct SummerSqlRewritePlugin;

impl Plugin for SummerSqlRewritePlugin {
    async fn build(&self, app: &mut AppBuilder) {
        let db: DatabaseConnection = app
            .get_component::<DatabaseConnection>()
            .expect("DatabaseConnection not found; ensure SeaOrmPlugin is registered first");

        let registry = app
            .get_component::<PluginRegistry>()
            .cloned()
            .unwrap_or_default();

        // 全局默认连接：适合非 Web / 后台任务 / 显式注入上下文的场景
        app.add_component(RewriteConnection::new(
            db.clone(),
            registry.clone(),
            std::sync::Arc::new(Extensions::new()),
        ));

        // Web 场景：请求进入时自动注入请求级上下文
        let mut layer = SqlRewriteLayer::new(db, registry);
        if let Some(extender) = app.get_component::<SqlRewriteRequestExtender>() {
            layer = layer.with_request_extender(extender.clone());
        }
        app.add_router_layer(move |router| router.layer(layer.clone()));
    }
}
```

### 6.2 请求级上下文注入（Web 场景 — 中间件 + 提取器）

参照现有 `TenantContextLayer` + `TenantShardingConnection` 的模式，
通过 **Axum 中间件**在请求进入时自动构建 `RewriteConnection`（注入当前请求上下文），
通过 **Axum 提取器**在 handler 中直接获取，handler **零感知**改写逻辑。

推荐主路径改成：

1. `SummerAuthPlugin` 先把 `UserSession` 注入 `request.extensions`
2. `SqlRewriteLayer` 直接把 `UserSession` 复制进改写用的 `Extensions`
3. 应用层如需补充额外信息，再通过可选的 `request_extender` 往同一个 `Extensions` 里追加

这样 `main` 不需要再手动 `Extensions::new()`，也不需要额外定义一层认证镜像结构体。

#### 6.2.1 中间件 — `SqlRewriteLayer`

```rust
// summer-sql-rewrite/src/web/middleware.rs（feature = "summer"）

use std::{future::Future, pin::Pin, sync::Arc};
#[cfg(feature = "summer-auth")]
use summer_auth::UserSession;
use summer_web::axum::{body::Body, extract::Request, response::Response};
use tower_layer::Layer;
use tower_service::Service;

use crate::{Extensions, PluginRegistry, RewriteConnection};

/// 请求级扩展补充器。
///
/// 中间件会先写入内建请求对象（如 `UserSession`），
/// 再调用它补充业务自定义数据。
pub type SqlRewriteRequestExtender =
    Arc<dyn Fn(&http::Extensions, &mut Extensions) + Send + Sync + 'static>;

/// SQL 改写中间件层。
///
/// 在每个请求进入时：
/// 1. 创建空的 `Extensions`
/// 2. 从 `request.extensions` 提取 `UserSession`（由 `AuthLayer` 注入）
/// 3. 直接将 `UserSession` 写入改写用的 `Extensions`
/// 4. 调用可选的 `request_extender` 追加业务扩展
/// 5. 构建带有当前请求上下文的 `RewriteConnection`
/// 6. 将 `RewriteConnection` 注入到 `request.extensions`
///
/// handler 中通过 `RewriteDbConn` 提取器直接获取。
#[derive(Clone)]
pub struct SqlRewriteLayer {
    /// 底层数据库连接
    db: DatabaseConnection,
    /// 插件注册表
    registry: Arc<PluginRegistry>,
    /// 可选：业务层追加请求级扩展
    request_extender: Option<SqlRewriteRequestExtender>,
}

impl SqlRewriteLayer {
    pub fn new(db: DatabaseConnection, registry: Arc<PluginRegistry>) -> Self {
        Self {
            db,
            registry,
            request_extender: None,
        }
    }

    pub fn with_request_extender(mut self, extender: SqlRewriteRequestExtender) -> Self {
        self.request_extender = Some(extender);
        self
    }
}

impl<S: Clone> Layer<S> for SqlRewriteLayer {
    type Service = SqlRewriteMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        SqlRewriteMiddleware {
            inner,
            db: self.db.clone(),
            registry: self.registry.clone(),
            request_extender: self.request_extender.clone(),
        }
    }
}

#[derive(Clone)]
pub struct SqlRewriteMiddleware<S> {
    inner: S,
    db: DatabaseConnection,
    registry: Arc<PluginRegistry>,
    request_extender: Option<SqlRewriteRequestExtender>,
}

impl<S> Service<Request> for SqlRewriteMiddleware<S>
where
    S: Service<Request, Response = Response<Body>> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = Response<Body>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: Request) -> Self::Future {
        // 1. 中间件内部统一创建扩展容器
        let mut ext = Extensions::new();

        // 2. 自动注入认证对象
        inject_builtin_request_extensions(req.extensions(), &mut ext);

        // 3. 可选：补充业务自定义扩展
        if let Some(extender) = &self.request_extender {
            extender(req.extensions(), &mut ext);
        }

        // 4. 构建请求级 RewriteConnection
        let conn = RewriteConnection::new(
            self.db.clone(),
            self.registry.clone(),
            Arc::new(ext),
        );

        // 5. 注入到 request.extensions
        req.extensions_mut().insert(conn);

        let mut inner = self.inner.clone();
        Box::pin(async move {
            inner.call(req).await
        })
    }
}

fn inject_builtin_request_extensions(req_ext: &http::Extensions, ext: &mut Extensions) {
    #[cfg(feature = "summer-auth")]
    if let Some(session) = req_ext.get::<UserSession>() {
        ext.insert(session.clone());
    }
}
```

#### 6.2.2 提取器 — `RewriteDbConn`

```rust
// summer-sql-rewrite/src/web/extractor.rs

use std::ops::Deref;
use summer_web::axum::extract::FromRequestParts;
use summer_web::axum::http::request::Parts;
use summer_web::axum::response::{IntoResponse, Response};

use crate::RewriteConnection;

/// SQL 改写数据库连接提取器。
///
/// 从 `request.extensions` 中提取由 `SqlRewriteLayer` 注入的 `RewriteConnection`。
///
/// # 用法
///
/// ```rust,ignore
/// async fn list_users(
///     RewriteDbConn(db): RewriteDbConn,
/// ) -> Result<Json<Vec<User>>> {
///     // db 已自动绑定当前用户上下文，所有查询自动注入数据权限
///     let users = User::find().all(&db).await?;
///     Ok(Json(users))
/// }
/// ```
#[derive(Debug, Clone)]
pub struct RewriteDbConn(pub RewriteConnection);

impl Deref for RewriteDbConn {
    type Target = RewriteConnection;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<S: Send + Sync> FromRequestParts<S> for RewriteDbConn {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let conn = parts
            .extensions
            .get::<RewriteConnection>()
            .cloned()
            .ok_or_else(missing_rewrite_connection)?;
        Ok(Self(conn))
    }
}

impl summer_web::aide::OperationInput for RewriteDbConn {}

fn missing_rewrite_connection() -> Response {
    summer_web::problem_details::ProblemDetails::new(
        "rewrite-connection-missing",
        "Internal Server Error",
        500,
    )
    .with_detail("SQL 改写连接未初始化，请确认已添加 SqlRewriteLayer 中间件")
    .into_response()
}
```

#### 6.2.3 完整使用示例（贴近当前 `main.rs` / Plugin 风格）

```rust
// ── main.rs ──
use summer::App;
use summer::auto_config;
use summer_auth::{SummerAuthConfigurator, SummerAuthPlugin};
use summer_sea_orm::SeaOrmPlugin;
use summer_sql_rewrite::{
    SqlRewriteConfigurator,
    SummerSqlRewritePlugin,
};
use summer_web::{WebConfigurator, WebPlugin};

#[auto_config(WebConfigurator)]
#[tokio::main]
async fn main() {
    App::new()
        .add_plugin(WebPlugin)
        .add_plugin(SeaOrmPlugin)
        .add_plugin(SummerAuthPlugin)
        .add_plugin(SummerSqlRewritePlugin)
        .sql_rewrite_configure(|registry| {
            registry
                .register(DataPermissionPlugin)
                .register(AuditCommentPlugin)
        })
        .run()
        .await;
}
```

这个版本是推荐主路径：

- `SummerAuthPlugin` 负责认证并把 `UserSession` 放进 `request.extensions`
- `SummerSqlRewritePlugin` 安装的 `SqlRewriteLayer` 会把 `UserSession` 复制进改写用的 `Extensions`
- `DataPermissionPlugin` / `AuditCommentPlugin` 可以直接读取 `UserSession`

如果业务还需要额外的请求级上下文，再启用一个“补充式”钩子即可：

```rust
.sql_rewrite_web_configure(|req_ext, ext| {
    if let Some(trace_id) = req_ext.get::<TraceId>() {
        ext.insert(trace_id.clone());
    }
})
```

这个钩子只负责“追加”，不再要求业务层手动 `Extensions::new()`。

```rust
// ── handler ──
use summer_sql_rewrite::web::RewriteDbConn;

/// handler 完全不感知 SQL 改写逻辑，直接提取 db 连接使用
async fn list_users(
    RewriteDbConn(db): RewriteDbConn,
) -> Result<Json<Vec<User>>> {
    let users = User::find().all(&db).await?;
    // 实际执行的 SQL（普通用户场景）:
    //   SELECT * FROM users WHERE create_by = 42 /* user_id=42 */
    Ok(Json(users))
}

/// 管理员看到的 SQL 不会被改写（DataPermissionPlugin 对 admin 放行）
async fn admin_list_all(
    RewriteDbConn(db): RewriteDbConn,
) -> Result<Json<Vec<User>>> {
    let users = User::find().all(&db).await?;
    // 实际执行: SELECT * FROM users （无额外条件）
    Ok(Json(users))
}
```

#### 6.2.4 与现有提取器的关系

```
请求进入
  │
  ├── AuthLayer         → 注入 UserSession 到 req.extensions
  │
  ├── SqlRewriteLayer   → 读取 UserSession，复制到改写用 Extensions，构建 RewriteConnection，注入 req.extensions
  │   （可选 request_extender 只做补充，不负责从零构造 Extensions）
  │
  └── Handler
      ├── RewriteDbConn(db)         ← 提取改写连接（普通表）
      ├── TenantShardingConnection  ← 提取分片连接（分片表，如果需要）
      ├── AdminUser { .. }          ← 提取用户信息（认证场景）
      └── ...
```

**关键设计**：
- `SqlRewriteLayer` 默认直接复用 `UserSession`，应用层不再手写 `Extensions::new()`
- 如需额外上下文，`request_extender` 采用 `Fn(&http::Extensions, &mut Extensions)` 这种“补充式”签名，避免业务层重复造壳
- `summer-sql-rewrite` 的 core crate 仍可保持与认证系统解耦；只有 Summer Web 集成层才知道 `UserSession`
- 中间件层级：`AuthLayer` 先执行注入 `UserSession`，`SqlRewriteLayer` 后执行读取它
- Handler 中只用 `RewriteDbConn(db)` 一行提取，**零模板代码**

### 6.3 与 ShardingConnection 共存

```rust
// main.rs
App::new()
    .add_plugin(SummerShardingPlugin)
    .add_plugin(SummerSqlRewritePlugin)   // 注册独立改写插件
    .sql_rewrite_configure(|registry| {
        registry
            .register(DataPermissionPlugin)     // 同时对分片和非分片表生效
            .register(AuditCommentPlugin)
    })
    .run()
    .await;
```

### 6.4 示例插件（条件拼接 / 审计注释 / 表名替换）

```rust,ignore
use summer_auth::UserSession;
use summer_sql_rewrite::{SqlRewritePlugin, SqlRewriteContext, helpers};

struct DataPermissionPlugin;

impl SqlRewritePlugin for DataPermissionPlugin {
    fn name(&self) -> &str { "data_permission" }
    fn order(&self) -> i32 { 50 }

    fn matches(&self, ctx: &SqlRewriteContext) -> bool {
        ctx.is_select() && ctx.extension::<UserSession>().is_some()
    }

    fn rewrite(&self, ctx: &mut SqlRewriteContext) -> Result<()> {
        let session = ctx.extension::<UserSession>().unwrap();

        if session.profile.roles().iter().any(|role| role == "admin") {
            return Ok(());
        }

        let condition = helpers::build_eq_int_expr("create_by", session.login_id.user_id);
        helpers::append_where(ctx.statement, condition);
        Ok(())
    }
}
```

#### 6.4.1 当前文档聚焦范围

这版文档只聚焦三类主能力：

- **基于请求上下文拼接条件**
  - 典型做法是读取 `UserSession`，通过 `helpers::append_where(...)` 追加过滤条件

- **追加审计注释**
  - 典型做法是读取 `UserSession` / trace id，在 SQL 末尾追加注释，便于排查和审计

- **表名替换**
  - 典型做法是按租户、归档、冷热分离规则做 `replace_table(...)`

#### 6.4.2 审计注释插件示例

```rust,ignore
use summer_auth::UserSession;
use summer_sql_rewrite::{SqlRewriteContext, SqlRewritePlugin};

struct AuditCommentPlugin;

impl SqlRewritePlugin for AuditCommentPlugin {
    fn name(&self) -> &str { "audit_comment" }
    fn order(&self) -> i32 { 100 }

    fn matches(&self, ctx: &SqlRewriteContext) -> bool {
        ctx.extension::<UserSession>().is_some()
    }

    fn rewrite(&self, ctx: &mut SqlRewriteContext) -> Result<()> {
        let session = ctx.extension::<UserSession>().unwrap();
        ctx.append_comment(format!("user_id={}", session.login_id.user_id).as_str());
        Ok(())
    }
}
```

#### 6.4.3 表名替换插件示例

```rust,ignore
use summer_sql_rewrite::{SqlRewriteContext, SqlRewritePlugin, helpers};

struct ArchiveTablePlugin;

impl SqlRewritePlugin for ArchiveTablePlugin {
    fn name(&self) -> &str { "archive_table" }

    fn matches(&self, ctx: &SqlRewriteContext) -> bool {
        ctx.primary_table() == Some("biz.order")
    }

    fn rewrite(&self, ctx: &mut SqlRewriteContext) -> Result<()> {
        helpers::replace_table(ctx.statement, "biz.order", "biz.order_archive")?;
        Ok(())
    }
}
```

---

## 七、迁移计划

### 第一阶段：创建 `summer-sql-rewrite` crate

| 步骤 | 内容 | 从哪里来 |
|------|------|----------|
| 1 | 创建 `crates/summer-sql-rewrite/Cargo.toml` | 新建 |
| 2 | 迁移 `extensions.rs` | `summer-sharding/src/extensions.rs` → 原样复制 |
| 3 | 创建 `error.rs` | 新建（简化版，只需 `Plugin` 变体） |
| 4 | 创建 `context.rs`（`SqlRewriteContext`） | 参考 `summer-sharding/src/rewrite_plugin/context.rs`，去掉分片字段 |
| 5 | 创建 `plugin.rs`（`SqlRewritePlugin` trait） | 参考 `summer-sharding/src/rewrite_plugin/mod.rs`，改用 `SqlRewriteContext` |
| 6 | 迁移 `registry.rs` | 参考 `summer-sharding/src/rewrite_plugin/registry.rs`，改用新类型 |
| 7 | 迁移 `helpers.rs` | `summer-sharding/src/rewrite_plugin/helpers.rs` → 保留强类型核心 API，补 `&str` 便利函数 |
| 8 | 创建 `connection.rs`（`RewriteConnection`） | 新建 |
| 9 | 创建 `transaction.rs`（`RewriteTransaction`） | 新建，作为初版事务包装器 |
| 10 | 创建 `configurator.rs`（Summer 框架集成，feature-gated） | 同时提供 `SqlRewriteConfigurator` 与 `SqlRewriteWebConfigurator` |
| 11 | `lib.rs` 模块声明和 re-export | 新建 |

### 第二阶段：`summer-sharding` 改为依赖 `summer-sql-rewrite`

| 步骤 | 内容 |
|------|------|
| 1 | `Cargo.toml` 添加 `summer-sql-rewrite` 依赖 |
| 2 | 删除 `src/extensions.rs`，改为 `pub use summer_sql_rewrite::Extensions` |
| 3 | 瘦化 `src/rewrite_plugin/`：re-export 通用类型，保留 `ShardingRouteInfo` 扩展 |
| 4 | 调整 `DefaultSqlRewriter::rewrite()`：构建 `SqlRewriteContext` 替代 `RewriteContext` |
| 5 | 调整 `src/rewrite/mod.rs`：将分片信息注入 `Extensions` |
| 6 | 更新 `src/lib.rs` 的 re-export |
| 7 | 更新 `src/connector/hint.rs`：`ShardingAccessContext.extensions` 改用 `summer_sql_rewrite::Extensions` |

### 第三阶段：框架集成与应用适配

| 步骤 | 内容 | 说明 |
|------|------|------|
| 1 | 创建 `SummerSqlRewritePlugin`（Summer 框架 Plugin） | 在 `build()` 中从 AppBuilder 取 `PluginRegistry` + `DatabaseConnection`，构建全局 `RewriteConnection` 组件 |
| 2 | 提供 `SqlRewriteConfigurator` trait for `AppBuilder` | `.sql_rewrite_configure(\|registry\| { ... })` |
| 3 | 提供 `SqlRewriteWebConfigurator` trait for `AppBuilder` | `.sql_rewrite_web_configure(\|req_ext, ext\| { ... })`，只做“补充式”请求扩展 |
| 4 | 实现 `SqlRewriteLayer`（Axum 中间件） | 在 `Plugin build()` 中通过 `app.add_router_layer(...)` 安装；自动读取 `req.extensions`，写入 `UserSession` 并构建请求级 `RewriteConnection` |
| 5 | 实现 `RewriteDbConn`（Axum 提取器） | `FromRequestParts`，从 `req.extensions` 提取 `RewriteConnection`，handler 零感知改写 |
| 6 | 在 `summerrs-admin` 的 `app` crate 中使用 | `main.rs` 注册 Plugin；常规场景无需手动拼 `Extensions` |
| 7 | 实现示例业务插件 | `DataPermissionPlugin`、`AuditCommentPlugin`、`ArchiveTablePlugin` |

---

## 八、现有文件改动清单

### 新增文件

| 文件 | 内容 | 状态 |
|------|------|------|
| `crates/summer-sql-rewrite/Cargo.toml` | crate 配置 | 待实现 |
| `crates/summer-sql-rewrite/src/lib.rs` | 模块入口 | 待实现 |
| `crates/summer-sql-rewrite/src/extensions.rs` | Extensions TypeMap | 待迁移 |
| `crates/summer-sql-rewrite/src/error.rs` | 错误类型 | 待实现 |
| `crates/summer-sql-rewrite/src/context.rs` | SqlRewriteContext | 待实现 |
| `crates/summer-sql-rewrite/src/plugin.rs` | SqlRewritePlugin trait | 待实现 |
| `crates/summer-sql-rewrite/src/registry.rs` | PluginRegistry | 待实现 |
| `crates/summer-sql-rewrite/src/helpers.rs` | SQL AST 工具函数（核心强类型 API + 便利函数） | 待迁移 |
| `crates/summer-sql-rewrite/src/pipeline.rs` | 改写执行共享管线 | 待实现 |
| `crates/summer-sql-rewrite/src/connection.rs` | RewriteConnection | 待实现 |
| `crates/summer-sql-rewrite/src/transaction.rs` | RewriteTransaction | 待实现 |
| `crates/summer-sql-rewrite/src/configurator.rs` | Summer 框架集成（registry + request extender 注册） | 待实现 |
| `crates/summer-sql-rewrite/src/web/mod.rs` | Web 集成模块入口 | 待实现 |
| `crates/summer-sql-rewrite/src/web/middleware.rs` | SqlRewriteLayer + SqlRewriteMiddleware | 待实现 |
| `crates/summer-sql-rewrite/src/web/extractor.rs` | RewriteDbConn 提取器 | 待实现 |

### 修改文件

| 文件 | 改动 | 状态 |
|------|------|------|
| `Cargo.toml`（workspace） | 添加 `summer-sql-rewrite` member 和 workspace dependency | 待改动 |
| `summer-sharding/Cargo.toml` | 添加 `summer-sql-rewrite` 依赖 | 待改动 |
| `summer-sharding/src/extensions.rs` | 改为 re-export `summer_sql_rewrite::Extensions` | 待改动 |
| `summer-sharding/src/rewrite_plugin/mod.rs` | re-export 通用类型 | 待改动 |
| `summer-sharding/src/rewrite_plugin/context.rs` | 删除或改为薄包装 | 待改动 |
| `summer-sharding/src/rewrite_plugin/registry.rs` | 删除，改为 re-export | 待改动 |
| `summer-sharding/src/rewrite_plugin/helpers.rs` | 删除，改为 re-export | 待改动 |
| `summer-sharding/src/rewrite/mod.rs` | 适配 `SqlRewriteContext`，注入 `ShardingRouteInfo` | 待改动 |
| `summer-sharding/src/connector/hint.rs` | `extensions` 类型改用 `summer_sql_rewrite::Extensions` | 待改动 |
| `summer-sharding/src/lib.rs` | 更新 re-export | 待改动 |

---

## 九、设计决策记录

### Q1: 为什么不直接在 SeaORM 中加 middleware 层？

SeaORM 已有 `ProxyDatabaseTrait`，但它工作在**驱动层**（接收/返回 raw rows），不是 SQL 改写的正确抽象层。SQL 改写需要在 `Statement` 层面拦截，`ConnectionTrait` 的装饰器模式更合适。

### Q2: `SqlRewriteContext` 为什么不持有 `&mut Extensions` 而是 `&Extensions`？

改写插件**读取**上层注入的上下文（用户信息、权限），不应该修改它。
插件之间的数据传递通过 `comments`（追加注释）或未来可扩展的 `plugin_data` 字段实现。

### Q3: `replace_table` 的 `QualifiedTableName` 如何处理？

不建议把核心 API 直接退化成纯 `&str`。

推荐做法是分层：

1. **核心 API 保持强类型**
  - `replace_table_qualified(statement, &QualifiedTableName, &QualifiedTableName)`
  - 内部 AST 改写、路由计划对接、测试断言都基于显式的 `(schema, table)` 结构

2. **对外补便利 API**
  - `replace_table(statement, "sys.orders", "sys.orders_202603")`
  - 便利函数内部再 parse 成 `QualifiedTableName`

3. **如果拆出 `summer-sql-rewrite` 独立 crate**
  - 在该 crate 中定义中立版本的 `QualifiedTableName`
  - `summer-sharding` 通过 `From` / `Into` trait 做转换
  - 避免独立 crate 反向依赖 `summer-sharding`

这样既保留类型安全，也不牺牲插件作者的易用性。

### Q4: 插件能否在两个连接中共享？

可以。`PluginRegistry` 实现了 `Clone`（内部是 `Vec<Arc<dyn SqlRewritePlugin>>`），
`SqlRewritePlugin` 要求 `Send + Sync + 'static`。同一套插件可以同时挂载在
`RewriteConnection` 和 `ShardingConnection` 上。

### Q5: 性能影响？

SQL 解析（sqlparser）是主要开销。对于 `ShardingConnection`，解析已在 `analyze_statement` 中完成，
可以复用 AST 避免二次解析。对于 `RewriteConnection`，每条 SQL 需要一次解析，
但 sqlparser 性能通常在微秒级，不构成瓶颈。

如果真需要优化，可以加上规则判断：对不含改写目标表的 SQL 直接跳过解析。

### Q6: 为什么用中间件 + 提取器，而不是在 handler 中手动构建 `RewriteConnection`？

参考现有 `TenantContextLayer` + `TenantShardingConnection` 的成功模式：
- **中间件**负责在请求进入时构建连接（`req.extensions_mut().insert(conn)`）
- **提取器**负责从请求中取出（`parts.extensions.get::<RewriteConnection>()`）
- **handler** 完全不感知改写逻辑，只声明参数 `RewriteDbConn(db): RewriteDbConn`

好处：
1. **零模板代码** — handler 不需要重复 `Extensions::new()` / `insert()` / `with_extensions()`
2. **不可遗漏** — 只要配了中间件，所有走该路由的查询都自动改写
3. **主路径更自然** — `AuthLayer` 已经把 `UserSession` 放进 `request.extensions`，SQL 改写层直接复用即可，不需要在 `main` 再写一段搬运代码
4. **可扩展** — 如需自定义请求级信息，再通过 `request_extender` 做补充，而不是整段上下文都交给业务层手搓
5. **一致性** — 与团队已有的 `TenantShardingConnection` 提取器用法完全一致，学习成本为零

### Q7: 事务支持？

`RewriteConnection` 初版就应当支持事务，不建议继续挂到后续版本。

推荐的初版设计：

1. `RewriteConnection` 实现 `TransactionTrait`
2. `begin()` / `begin_with_config()` / `begin_with_options()` 返回 `RewriteTransaction`
3. `RewriteTransaction` 包装底层 `DatabaseTransaction`
4. `RewriteTransaction` 实现：
  - `ConnectionTrait`
  - `TransactionTrait`
  - `TransactionSession`
5. 事务内每条 SQL 继续走同一套插件改写链

初版边界明确如下：

- **支持**
  - 单库事务
  - 嵌套事务（底层支持时继续包装）
  - `transaction(callback)` 语法
  - `commit()` / `rollback()`

- **暂不支持**
  - 跨数据源分布式事务
  - 与 `ShardingTransaction` 的统一协调
  - 事务中途动态切换 `Extensions`
  - 两阶段提交 / Saga 集成

实现建议：

- 抽一个共享执行管线模块（如 `pipeline.rs`）
- `RewriteConnection` 与 `RewriteTransaction` 共用“解析 SQL → 执行插件链 → 生成改写 SQL → 委托执行”的逻辑
- 事务开始时冻结一份 `Extensions` 快照，保证同一事务内上下文稳定

---

## 十、里程碑

| 阶段 | 内容 | 预计改动量 |
|------|------|-----------|
| **Phase 1** | 创建 `summer-sql-rewrite` crate，迁移通用代码，实现 `RewriteConnection` + `RewriteTransaction` | ~12 个文件，~1000 行 |
| **Phase 2** | `summer-sharding` 改为依赖 `summer-sql-rewrite`，瘦化 `rewrite_plugin` | ~7 个文件改动 |
| **Phase 3** | Summer 框架集成（Plugin + Configurator + Web 中间件） | ~3 个文件 |
| **Phase 4** | 应用层适配（`main.rs`、业务插件迁移） | 按需 |

---

## 附录 A：Cargo.toml 参考

```toml
# crates/summer-sql-rewrite/Cargo.toml

[package]
name = "summer-sql-rewrite"
version = "0.1.0"
edition = "2024"

[dependencies]
sea-orm = { workspace = true }
sqlparser = { workspace = true }
tracing = { workspace = true }
thiserror = { workspace = true }

[features]
default = []
web = ["dep:summer-web", "dep:tower-layer", "dep:tower-service", "dep:http"]
summer = ["dep:summer"]
summer-auth = ["web", "summer", "dep:summer-auth"]

[dependencies.summer]
workspace = true
optional = true

[dependencies.summer-web]
workspace = true
optional = true

[dependencies.summer-auth]
workspace = true
optional = true

[dependencies.tower-layer]
workspace = true
optional = true

[dependencies.tower-service]
workspace = true
optional = true

[dependencies.http]
workspace = true
optional = true
```

## 附录 B：模块依赖图

```
summer-sql-rewrite
├── extensions.rs          (独立，无外部依赖)
├── error.rs               (thiserror)
├── context.rs             → extensions
├── plugin.rs              → context, error
├── registry.rs            → plugin, context, error
├── helpers.rs             (sqlparser)
├── pipeline.rs            → registry, context, helpers, extensions
├── connection.rs          → registry, context, helpers, extensions, pipeline (sea-orm)
├── transaction.rs         → registry, context, helpers, extensions, pipeline (sea-orm)
├── configurator.rs        → registry + request extender 注册 (summer, feature-gated)
└── web/                   (feature = "web")
    ├── mod.rs             → middleware, extractor
    ├── middleware.rs       → SqlRewriteLayer / SqlRewriteMiddleware (tower)
    └── extractor.rs       → RewriteDbConn (FromRequestParts)

summer-sharding (改动后)
├── rewrite_plugin/
│   ├── mod.rs             → re-export summer-sql-rewrite
│   ├── sharding_context.rs → ShardingRouteInfo (注入到 Extensions)
│   └── configurator.rs    → summer-sharding 专用配置器
├── rewrite/mod.rs         → 使用 SqlRewriteContext + ShardingRouteInfo
└── extensions.rs          → re-export summer_sql_rewrite::Extensions
```
