# summer-sharding / summer-sql-rewrite 当前问题清单

> **审查日期**: 2026年3月31日
> **审查范围**: `summer-sharding`（分片中间件）+ `summer-sql-rewrite`（独立 SQL 改写层）
> **最新同步**: 2026年3月31日
> **编译状态**: ✅ 通过
> **测试状态**:
> - `summer-sql-rewrite`: ✅ 28/28 + doc-tests 2/2
> - `summer-sharding`: ✅ 156 passed / 17 ignored + 1 doc-test ignored
> - `summer-sharding ignored / 基础设施级 E2E`: ✅ 17/17

> 状态说明：
> - `✅ 已修`
> - `🟡 部分已修`
> - `⏳ 未修`

---

## P0 — 类型重复，需尽快统一

### 1. `QualifiedTableName` 双重定义 ✅ 已修

- **位置**:
  - `summer-sql-rewrite::table::QualifiedTableName`
  - `summer-sharding::router::QualifiedTableName`
- **问题**: 两个 crate 各自定义了完全相同的 `QualifiedTableName`，但它们是**不同的 `TypeId`**。
  - `summer-sql-rewrite::helpers` 中的 `replace_table()` / `replace_table_qualified()` 使用 `summer-sql-rewrite::QualifiedTableName`
  - `summer-sharding::rewrite/` 中的 `table_rewrite.rs`、`schema_rewrite.rs` 使用 `summer-sharding::router::QualifiedTableName`
- **当前影响**: 因为两个 crate 的代码路径各自封闭（sharding 的内置改写直接用自己的类型，`summer-sql-rewrite` 的 helpers 用自己的），所以**暂时不会导致编译错误或运行 bug**。
- **潜在风险**: 如果用户在 `SqlRewritePlugin::rewrite()` 中调用 `helpers::replace_table_qualified()` 并传入从 sharding 的 `ShardingRouteInfo` 获取的表名，会遇到类型不兼容问题。虽然 `ShardingRouteInfo.table_rewrites` 目前是 `Vec<(String, String)>`（规避了类型问题），但设计上不干净。
- **当前状态**: `summer-sharding` 已直接 re-export `summer_sql_rewrite::QualifiedTableName`，重复定义已移除。

### 2. `SqlOperation` 双重定义 ✅ 已修

- **位置**:
  - `summer-sql-rewrite::context::SqlOperation`
  - `summer-sharding::router::SqlOperation`
- **问题**: 同样是两个完全相同的枚举，存在于两个 crate 中。
- **当前缓解**: `summer-sharding::rewrite/mod.rs` 中有一个 `rewrite_operation()` 函数做显式映射转换（5 个 match arm），所以功能上是正确的。
- **影响**: 增加维护成本。如果未来一方增加新变体而另一方忘记同步，会导致 `match` 不完整。
- **当前状态**: `summer-sharding` 已统一复用 `summer_sql_rewrite::SqlOperation`，中间转换函数已删除。

---

## P1 — 功能缺失 / 架构改进

### 3. `RewriteConnection` / `RewriteTransaction` 未实现 `StreamTrait` ✅ 已修

- **位置**: `summer-sql-rewrite::connection::RewriteConnection`、`summer-sql-rewrite::transaction::RewriteTransaction`
- **问题**: SeaORM 的 `DatabaseConnection` 和 `DatabaseTransaction` 都实现了 `StreamTrait`（提供 `stream()` / `stream_raw()` 方法）。`RewriteConnection` 和 `RewriteTransaction` 当前**只实现了 `ConnectionTrait` 和 `TransactionTrait`**，未实现 `StreamTrait`。
- **影响**: 如果用户在 `RewriteConnection` 或事务内使用 `stream()` 查询，SQL 将**不经过改写**直接发送到数据库——这是一个静默的行为不一致。
  ```rust
  // 这条 SQL 会被改写 ✅
  Entity::find().all(&rewrite_conn).await?;
  // 这条 SQL 不会被改写 ❌ （如果 stream 直接委托到 inner）
  Entity::find().stream(&rewrite_conn).await?;
  ```
- **当前状态**: 已实现并补了对应测试。

### 4. `PluginRegistry` 缺少 `Send + Sync` 编译时断言 ✅ 已修

- **位置**: `summer-sql-rewrite::registry::PluginRegistry`
- **问题**: `PluginRegistry` 内部是 `Vec<Arc<dyn SqlRewritePlugin>>`，由于 `SqlRewritePlugin: Send + Sync + 'static`，`PluginRegistry` 自动满足 `Send + Sync`。但这依赖于自动推导，如果后续增加了 `!Send` 字段（如 `Rc<T>`），会**静默失去** `Send + Sync`，导致使用方（`ShardingConnectionInner` 跨线程共享）编译失败。
- **当前状态**: 已添加编译时断言。

### 5. `RewriteConnection` 的 `DatabaseConnection` clone 语义 ✅ 已修

- **位置**: `summer-sql-rewrite::connection::RewriteConnection::with_extensions()`
- **问题**: `with_extensions()` 调用了 `self.inner.clone()`。SeaORM 的 `DatabaseConnection` clone 是引用计数（内部 `Arc<Pool>`），所以没有性能问题。但整个 `RewriteConnection` 也 `#[derive(Clone)]`，这意味着每次 Web 请求通过 extractor 拿到的 `RewriteDbConn` 都是 clone 后的。
- **影响**: 功能上正确，但每次请求都 clone 一份 `PluginRegistry`（内部 `Vec<Arc<...>>`），虽然只是浅拷贝，但在高 QPS 场景下会产生不必要的堆分配。
- **当前状态**: `RewriteConnection` / `RewriteTransaction` / `SqlRewriteLayer` 已统一使用共享 `Arc<PluginRegistry>`。

### 6. `ShardingConnection.set_plugin_registry()` 的 `Arc::get_mut` 约束 ✅ 已修

- **位置**: `summer-sharding::connector::connection.rs`
- **问题**: 历史实现依赖 `Arc::get_mut(&mut self.inner)`，一旦连接先被 clone，再注入 plugin registry，就会 panic。
- **当前状态**:
  - 已改为 `OnceLock<PluginRegistry>`，不再依赖“必须未共享”的调用时机
  - clone 后再注入也不会 panic，并新增回归测试覆盖

---

## P2 — 文档过期 / 代码卫生

### 7. `PLUGIN_DESIGN.md` 与实际实现不一致 ✅ 已修

- **位置**: `crates/summer-sharding/PLUGIN_DESIGN.md`
- **问题**: 文档描述的是 SQL 改写插件系统嵌入 `summer-sharding` 内部的设计方案。实际上代码已经迁移到了独立的 `summer-sql-rewrite` crate，`summer-sharding::rewrite_plugin/` 变成了纯 re-export 层。文档中的：
  - 模块结构（§2.2）描述的是 sharding 内嵌结构，与实际不符
  - 核心类型名（§3.2 `RewriteContext` vs 实际 `SqlRewriteContext`）不一致
  - 错误类型（§9 `ShardingError::Plugin`）仍然存在于 sharding 侧，但 `summer-sql-rewrite` 有自己的 `SqlRewriteError::Plugin`
  - 使用示例中的 import 路径仍指向 `summer_sharding::rewrite_plugin::*`
- **当前状态**: 文档已重写为“当前实现说明”。

### 8. `SQL_REWRITE_MIDDLEWARE_DESIGN.md` 部分内容已实现但未标注 ✅ 已修

- **位置**: `crates/summer-sharding/SQL_REWRITE_MIDDLEWARE_DESIGN.md`
- **问题**: 这份文档是独立中间件层的设计稿，其中：
  - **已实现** ✅: `summer-sql-rewrite` crate 创建、`RewriteConnection`、`RewriteTransaction`、`pipeline.rs`、`SqlRewriteLayer`/`RewriteDbConn`（web 模块）、`Configurator`
  - **文档状态**仍标注为 "设计稿"
  - 里程碑（§10）中的 Phase 1-3 实际上已完成
- **当前状态**: 文档已改为“已实现”，并同步了当前能力边界。

### 9. `summer-sql-rewrite` 缺少 README ✅ 已修

- **位置**: `crates/summer-sql-rewrite/`
- **问题**: 作为一个独立的、可被外部 crate 直接依赖的库，没有 README 或 crate-level 文档（`//! ...` doc comments）说明其用途、API 概览、使用示例。
- **当前状态**: 已补 `README.md` 和 crate-level 文档，并启用 doc-tests。

### 10. Clippy 警告 ✅ 已修

- **summer-sql-rewrite**: 1 个警告
  - `helpers.rs:202` — `creating a new box`（clippy::box_default 或类似）
- **summer-sharding**: 若干遗留警告
  - `too_many_arguments`（`prepare_statement` 有 7 个参数）
  - `collapsible_if` 等 style 类
- **当前状态**:
  - `summer-sql-rewrite` 已通过 `cargo clippy -D warnings`
  - `summer-sharding` 已通过 `cargo clippy -D warnings`

### 11. `ShardingRouteInfo.table_rewrites` 使用 `(String, String)` 而非类型化 ✅ 已修

- **位置**: `summer-sharding::rewrite_plugin::context::ShardingRouteInfo`
  ```rust
  pub struct ShardingRouteInfo {
      pub datasource: String,
      pub table_rewrites: Vec<(String, String)>,  // (logic, actual)
      pub is_fanout: bool,
  }
  ```
- **问题**: `table_rewrites` 使用 `(String, String)` 元组，语义不明确。用户需要记住第一个是逻辑表名、第二个是物理表名。
- **当前状态**: 已改为类型化 `TableRewritePair { logic, actual }`。

### 12. `summer-sql-rewrite::context::SqlRewriteContext` 的 `extensions` 是不可变引用 ✅ 已修

- **位置**: `summer-sql-rewrite::context::SqlRewriteContext`
- **问题**: 插件只能**读取** extensions，不能在插件链中间**写入**新的扩展数据给下游插件。这限制了插件间的数据传递能力。
- **当前状态**:
  - 已改为插件链内部使用可写 `Extensions`
  - `SqlRewriteContext` 新增受控写入 API，可让上游插件为下游插件发布扩展数据
  - pipeline 使用局部扩展副本，避免把插件链的临时写入泄漏回请求级原始上下文

---

## 附录：编译与测试验证

```
$ cargo check -p summer-sql-rewrite --features summer,web,summer-auth  ✅
$ cargo check -p summer-sharding --features web                         ✅
$ cargo clippy -p summer-sql-rewrite --features summer,web,summer-auth -- -D warnings ✅
$ cargo clippy -p summer-sharding --features web -- -D warnings         ✅
$ cargo test -p summer-sql-rewrite --features summer,web,summer-auth    ✅ 28/28
$ cargo test -p summer-sql-rewrite --features summer,web,summer-auth --doc ✅ 2/2
$ cargo test -p summer-sharding --features web                          ✅ 156 passed / 17 ignored
$ cargo test -p summer-sharding --features web -- --ignored             ✅ 17/17
```
