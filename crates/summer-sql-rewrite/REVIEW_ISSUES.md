# summer-sql-rewrite 代码审查报告

> **审查日期**: 2026年3月31日
> **最新同步**: 2026年3月31日
> **编译状态**: ✅ 通过 | **单元测试**: ✅ 28/28 | **Doc-tests**: ✅ 2/2

> 状态说明：
> - `✅ 已修`：代码或文档已同步到当前实现
> - `🟡 部分已修`：风险已降级或已补保护，但未做到最理想形态
> - `⏳ 未修`：仍保持原问题描述

---

## P0 — 正确性 / 数据丢失风险

### 1. `rewrite_statement()` 丢弃预编译参数绑定位置信息 🟡 部分已修

- **位置**: `pipeline.rs:25-49`
- **问题**: `rewrite_statement()` 将 SQL 文本 parse 成 AST → 插件改写 AST → `parsed.to_string()` 重新序列化回 SQL 文本。但原始 `Statement` 的 `values` (参数绑定 `Values`) 是按**位置**（`$1`, `$2`, ...）与 SQL 中的占位符对应的。

  如果插件在改写中**重新排列了 WHERE 子句的结构**（例如 `append_where` 在已有条件前/后插入新条件），sqlparser 重新序列化后占位符的文本顺序可能改变，但 `stmt.values` 的顺序**不会随之调整**。

  ```rust
  // 原始 SQL: SELECT * FROM users WHERE name = $1 AND age > $2
  // 插件插入新条件: AND tenant_id = 'abc'
  // 改写后: SELECT * FROM users WHERE name = $1 AND age > $2 AND tenant_id = 'abc'
  // ↑ 这种情况 OK，追加在末尾没问题
  //
  // 但如果插件做了：
  // append_where → inject_condition → and(existing, new_condition)
  // 并且 existing 被重构了，占位符顺序可能改变
  ```

  **实际风险评估**:
  - `append_where()` 的 `inject_condition()` 使用 `selection.take()` + `and(existing, condition)` — 即 `(existing) AND (new_condition)`。这**保持了** existing 中 `$1`, `$2` 的相对顺序，追加的新条件通常使用字面值而非占位符，所以**当前 helper 函数是安全的**。
  - 但 `SqlRewritePlugin::rewrite()` 给了用户 `&mut AstStatement` 的完全控制权。如果用户在插件中直接操作 AST 重排了占位符，`values` 就会错位。
  - sqlparser 的 `to_string()` 不保证与原始 SQL 的占位符编号一致（虽然实践中通常一致）。

- **当前状态**:
  1. 已实现 prepared placeholder remap：
     - PostgreSQL 会按最终 AST 的占位符使用情况做重排 / 压缩 / 去除未使用绑定
     - MySQL / SQLite 在“复用原 AST 占位符节点”的改写场景下，会按最终 AST 顺序重排 `values`
  2. 已保留数量校验，出现无法安全重排的情况时直接报错，而不是静默带错执行
  3. 对于 MySQL / SQLite，如果插件完全重建了新的 `?` 占位符而不是复用原节点，当前仍无法可靠恢复原始绑定语义，因此该项保留为部分已修

### 2. `extract_tables()` 未处理子查询中的表名 ✅ 已修

- **位置**: `pipeline.rs:81-85`, `collect_statement_tables()` / `collect_query_tables()`
- **问题**: `collect_query_tables()` 在处理 `SetExpr::Select` 时只遍历 `select.from`，**没有递归检查 WHERE 子句中的子查询**。

  ```sql
  SELECT * FROM users WHERE id IN (SELECT user_id FROM orders WHERE shop_id = 1)
  ```

  `tables` 结果只有 `["users"]`，缺少 `"orders"`。插件的 `matches()` 方法依赖 `ctx.tables` 判断是否需要改写，如果表名不完整，可能**漏掉应该改写的 SQL**。

- **影响范围**:
  - WHERE 子查询 (`IN (SELECT ...)`, `EXISTS (SELECT ...)`)
  - HAVING 子查询
  - SELECT 列中的标量子查询 (`(SELECT count(*) FROM ...)`)
  - JOIN 条件中的子查询

- **当前状态**: 已改为遍历 relation 访问器，子查询中的表名可被提取，且结果已做去重。

### 3. `parse_sql()` 只取第一个 AST 节点，静默丢弃多语句 ✅ 已修

- **位置**: `pipeline.rs:26-31`
  ```rust
  let mut ast = parse_sql(stmt.db_backend, sql.as_str())?;
  if ast.is_empty() { return Ok(stmt); }
  let mut parsed = ast.remove(0);
  ```
- **问题**: 如果 SQL 包含多个语句（用 `;` 分隔），`parse_sql()` 返回 `Vec<AstStatement>`，但只取 `ast.remove(0)`，后续语句**被静默丢弃**。
- **实际风险**: SeaORM 正常使用中几乎不会产生多语句 SQL。但如果用户通过 `execute_unprepared()` 传入多语句 SQL（比如数据迁移脚本），只有第一条会被改写，其余会丢失。
- **当前状态**:
  - `rewrite_unprepared_sql()` 已支持逐条改写多语句 SQL
  - prepared `Statement` 场景下若出现多语句，会直接报错，不再静默丢弃

---

## P1 — 功能缺失 / 设计问题

### 4. `RewriteConnection` / `RewriteTransaction` 未实现 `StreamTrait` ✅ 已修

- **位置**: `connection.rs`, `transaction.rs`
- **问题**: SeaORM 的 `DatabaseConnection` 和 `DatabaseTransaction` 都实现了 `StreamTrait`（提供 `stream()` / `stream_raw()` 方法用于流式读取大结果集）。`RewriteConnection` 和 `RewriteTransaction` 只实现了 `ConnectionTrait` 和 `TransactionTrait`。
- **影响**: 用户如果通过 `RewriteConnection` 使用 `stream()` 查询，编译会失败（因为 `StreamTrait` 未实现），这比"静默跳过"要好——至少用户知道不支持。但这限制了 `RewriteConnection` 作为 `DatabaseConnection` 的**透明替代**。
- **当前状态**: `RewriteConnection` 和 `RewriteTransaction` 已实现 `StreamTrait`，并补了对应测试。

### 5. `SqlRewriteMiddleware` 未使用 `poll_ready` 的正确 Tower 模式 ✅ 已修

- **位置**: `web/middleware.rs:65-70`
  ```rust
  fn poll_ready(&mut self, cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
      self.inner.poll_ready(cx)
  }

  fn call(&mut self, mut req: Request) -> Self::Future {
      ...
      let mut inner = self.inner.clone();
      ...
  }
  ```
- **问题**: `poll_ready()` 在 `&mut self.inner` 上调用，但 `call()` 中使用的是 `self.inner.clone()`。这违反了 Tower 的 `Service` 契约：`poll_ready` 应该检查**即将被用于 `call()` 的那个 service 实例**的就绪状态。当前代码对 `self.inner` poll_ready，但实际 call 用的是 clone 出来的新实例，那个 clone 可能还没 ready。
- **正确模式** (参考 `tower::util::CloneableService`):
  ```rust
  fn poll_ready(&mut self, _cx: &mut std::task::Context<'_>) -> Poll<Result<(), Self::Error>> {
      Poll::Ready(Ok(()))
  }

  fn call(&mut self, req: Request) -> Self::Future {
      let mut inner = self.inner.clone();
      std::mem::swap(&mut self.inner, &mut inner);
      // 现在 inner 持有 poll_ready 过的实例
      Box::pin(async move { inner.call(req).await })
  }
  ```
  或者更简单地，如果内部 Service 总是 Ready（如 Axum 的 Router），可以直接返回 `Poll::Ready(Ok(()))` 而不调用 inner.poll_ready()。
- **当前状态**: 已改为 `poll_ready` 后在 `call()` 中交换同一实例，补了 ready-sensitive 测试。

### 6. `PluginRegistry::rewrite_all()` 的错误处理——double wrapping ✅ 已修

- **位置**: `registry.rs:40-45`
  ```rust
  plugin
      .rewrite(ctx)
      .map_err(|error| SqlRewriteError::Plugin {
          plugin: plugin.name().to_string(),
          message: error.to_string(),
      })?;
  ```
- **问题**: `SqlRewritePlugin::rewrite()` 返回 `Result<()>`（即 `Result<(), SqlRewriteError>`）。如果插件内部已经返回了一个 `SqlRewriteError::Plugin { plugin: "xxx", message: "yyy" }`，外面又包了一层 `SqlRewriteError::Plugin { plugin: "xxx", message: "sql rewrite plugin `xxx` failed: yyy" }`。导致错误信息嵌套冗余。
- **当前状态**: 已避免对 `SqlRewriteError::Plugin` 再包一层。

### 7. `PluginRegistry::register()` 每次插入后排序，时间复杂度高 ✅ 已修

- **位置**: `registry.rs:20-24`
  ```rust
  pub fn register(&mut self, plugin: impl SqlRewritePlugin) -> &mut Self {
      self.plugins.push(Arc::new(plugin));
      self.plugins.sort_by_key(|plugin| plugin.order());
      self
  }
  ```
- **问题**: 每次 `register()` 调用后对整个 Vec 做 `sort_by_key()`，如果注册 N 个插件，总时间复杂度 O(N² log N)。
- **实际影响**: 插件数量通常很少（< 20），性能不是问题。但设计上可以改进。
- **当前状态**: 已改成按 `order()` 二分插入。

### 8. `RewriteConnection::with_extensions()` clone 整个 registry ✅ 已修

- **位置**: `connection.rs:42-48`
  ```rust
  pub fn with_extensions(&self, extensions: Extensions) -> Self {
      Self {
          inner: self.inner.clone(),
          registry: self.registry.clone(), // Vec<Arc<...>> 浅拷贝
          extensions,
      }
  }
  ```
- **问题**: `PluginRegistry` 内部是 `Vec<Arc<dyn SqlRewritePlugin>>`，clone 是浅拷贝（只复制 Arc 指针），但在高 QPS 场景下（每个 HTTP 请求都 clone 一次），堆分配开销仍然存在。
- **当前状态**: `RewriteConnection` / `RewriteTransaction` / `SqlRewriteLayer` 已统一使用共享 `Arc<PluginRegistry>`。

---

## P2 — 代码卫生 / 测试覆盖

### 9. `column_expr()` 空字符串处理不当 ✅ 已修

- **位置**: `helpers.rs:351-362`
  ```rust
  fn column_expr(column: &str) -> Expr {
      let mut idents = column
          .split('.')
          .filter(|value| !value.is_empty())
          .map(Ident::new)
          .collect::<Vec<_>>();
      match idents.len() {
          0 => Expr::Identifier(Ident::new(column)), // column == "" 时创建空 Ident
          1 => Expr::Identifier(idents.remove(0)),
          _ => Expr::CompoundIdentifier(idents),
      }
  }
  ```
- **问题**: 如果传入 `""`，filter 会过滤掉所有空段，`idents.len() == 0`，然后 `Ident::new("")` 创建了一个空标识符，生成无效 SQL。如果传入 `".."`, filter 后同样为空。
- **当前状态**: 已改为对空列名 / 非法空段直接 panic，并补了测试。

### 10. `QualifiedTableName::parse()` 不处理多级 schema ✅ 已修

- **位置**: `table.rs:10-21`
  ```rust
  pub fn parse(value: &str) -> Self {
      match value.split_once('.') {
          Some((schema, table)) => Self {
              schema: Some(schema.to_string()),
              table: table.to_string(),
          },
          None => Self { schema: None, table: value.to_string() },
      }
  }
  ```
- **问题**: `split_once('.')` 只分割第一个 `.`。对于 `catalog.schema.table` 这种三级命名，会解析为 `schema = "catalog"`, `table = "schema.table"`，逻辑错误。
- **实际影响**: PostgreSQL 支持 `catalog.schema.table` 格式（虽然 catalog 通常省略）。但一般使用中几乎总是 `schema.table`，所以风险低。
- **当前状态**: 已按最后一段识别表名，并同步修正 `to_object_name()` / `matches_object_name()`。

### 11. `SqlRewriteContext.tables` 是 `Vec<String>` 而非去重集合 ✅ 已修

- **位置**: `context.rs:17`
  ```rust
  pub tables: Vec<String>,
  ```
- **问题**: 对于 `SELECT * FROM users u JOIN users u2 ON ...`，`tables` 会包含 `["users", "users"]`。这不影响正确性，但可能导致插件在 `matches()` 中用 `tables.contains()` 时产生重复匹配。
- **当前状态**: 仍保持 `Vec<String>`，但在提取阶段已去重，保持顺序的同时避免重复。

### 12. `inject_builtin_request_extensions()` 只注入 `UserSession` 🟡 部分已修

- **位置**: `web/middleware.rs:89-94`
  ```rust
  fn inject_builtin_request_extensions(req_ext: &http::Extensions, ext: &mut Extensions) {
      #[cfg(feature = "summer-auth")]
      if let Some(session) = req_ext.get::<UserSession>() {
          ext.insert(session.clone());
      }
  }
  ```
- **问题**: 只处理了 `UserSession`。如果 `summer-auth` feature 未开启，这个函数**什么都不做**——builtin 注入为空。其他常见的请求上下文信息（如 request ID、trace ID、IP 地址等）没有自动注入。
- **当前状态**: 代码保持不变，但 README 与 crate 文档已明确说明 builtin 只注入 `UserSession`，其他上下文需通过 extender 注入。

### 13. `SummerSqlRewritePlugin::build()` 中 `db.clone()` 被用了两次 🟡 部分已修

- **位置**: `lib.rs:44-64`
  ```rust
  async fn build(&self, app: &mut AppBuilder) {
      let db = app.get_component::<sea_orm::DatabaseConnection>().expect(...);
      let registry = app.get_component::<PluginRegistry>().unwrap_or_default();

      app.add_component(RewriteConnection::new(
          db.clone(),
          registry.clone(),
          Extensions::new(),
      ));

      #[cfg(feature = "web")]
      {
          let mut layer = web::SqlRewriteLayer::new(db.clone(), registry);
          ...
      }
  }
  ```
- **问题**: `RewriteConnection` 和 `SqlRewriteLayer` 各持有一份 `DatabaseConnection` clone，而 `RewriteDbConn` extractor（web 模式下）实际用的是 middleware 注入的那份。手动注入的 `RewriteConnection` component 和 middleware 产生的 `RewriteConnection` 是**不同实例**——前者不携带 per-request extensions。
- **潜在困惑**: 如果用户在非 web 上下文中通过 `app.get_component::<RewriteConnection>()` 获取连接，会得到一个空 extensions 的实例——这可能是预期行为，但不够直观。
- **当前状态**: 运行语义未改，但 README / crate 文档已明确区分“component 用于非 Web，Web 场景应使用 `RewriteDbConn`”。

### 14. Clippy 警告: `helpers.rs:202` box allocation ✅ 已修

- **位置**: `helpers.rs:202-233`（`wrap_subquery` 函数）
- **问题**: `query.body = Box::new(...)` 创建新 Box，但 `query.body` 已经是 `Box<SetExpr>`，可以直接 `*query.body = ...` 原地赋值，避免不必要的堆分配。
- **当前状态**: 已改成原地赋值，不再重复创建 Box。

### 15. 零 doc-tests ✅ 已修

- **位置**: 全库
- **问题**: `cargo test` 输出 `Doc-tests summer_sql_rewrite: 0 tests`。公开 API（`RewriteConnection`, `SqlRewritePlugin`, `PluginRegistry`, helpers 函数）没有任何文档示例。
- **当前状态**: 已新增 README、crate-level 文档，并有 `2/2` doc-tests 通过。

### 16. `error.rs` 的 `From<SqlRewriteError> for DbErr` 丢失结构化信息 🟡 部分已修

- **位置**: `error.rs`
- **问题**: 所有 `SqlRewriteError` 变体（`Parse`, `Rewrite`, `Plugin`）统一转为 `DbErr::Custom(String)`，丢失了结构化信息。调用方无法通过 pattern match 区分是解析错误还是插件错误。
- **影响**: 在 `ConnectionTrait` 的实现中，`pipeline::rewrite_statement()` 返回 `Result<Statement, SqlRewriteError>`，通过 `?` 自动转为 `DbErr`。上层代码只能看到字符串错误消息。
- **当前状态**:
  - 仍然受 SeaORM `DbErr` 类型设计限制，无法直接挂载自定义结构化错误对象
  - 现已改成可逆编码，并新增 `SqlRewriteError::from_db_err()` / `from_db_err_message()` 恢复辅助
  - 调用方虽然依旧不能直接对 `DbErr` 做原生 variant pattern match，但已经可以把我们写入的 `DbErr::Custom` 恢复回 `SqlRewriteError`

### 17. `append_where` 对 `SetOperation` 的处理值得商榷 🟡 部分已修

- **位置**: `helpers.rs`
- **问题**: 对 UNION / INTERSECT / EXCEPT 操作，`append_where` 会向**左右两侧**都注入 WHERE 条件。这种行为是否符合用户预期取决于场景：
  - 多租户过滤：应该两侧都加 ✅
  - 数据范围限制：应该两侧都加 ✅
  - 但如果只想过滤最终结果集，应该用 `wrap_subquery` 包成子查询后再加 WHERE
  - 历史实现里如果 `left` 或 `right` 不是 `Select`（而是嵌套的 `SetOperation`），不会递归处理
- **当前状态**:
  - 现在已经递归处理嵌套 `SetOperation`，并补了覆盖测试
  - 但“是对子分支加条件，还是对最终结果集加条件”这个语义选择仍然保留为当前 helper 的设计约定，没有改成另一种行为

---

## 附录

### A. 当前验证

当前已验证：

- `cargo test -p summer-sql-rewrite --features summer,web,summer-auth`
  - 单元测试 `28/28` 通过
- `cargo test -p summer-sql-rewrite --features summer,web,summer-auth --doc`
  - doc-tests `2/2` 通过

### B. 编译与工具链

```
$ cargo check -p summer-sql-rewrite --features summer,web,summer-auth  ✅
$ cargo test  -p summer-sql-rewrite --features summer,web,summer-auth  ✅ 28/28
$ cargo test  -p summer-sql-rewrite --features summer,web,summer-auth --doc ✅ 2/2
```
