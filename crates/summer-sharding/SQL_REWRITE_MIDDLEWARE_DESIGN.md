# SQL 改写中间件层实现说明

> **版本**: v2.0  
> **更新日期**: 2026年3月31日  
> **状态**: 已实现  
> **相关 crate**: [`summer-sql-rewrite`](../summer-sql-rewrite/README.md)

---

## 一、当前状态

这份文档原本是“把 SQL 改写能力从 `summer-sharding` 抽离出来”的设计稿。现在该方案已经落地，实现结果就是独立 crate `summer-sql-rewrite`。

当前结论：

- **Phase 1**：核心抽离，已完成
- **Phase 2**：`RewriteConnection` / `RewriteTransaction`，已完成
- **Phase 3**：Web 集成层，已完成
- **Phase 4**：与 `summer-sharding` 的集成，已完成

因此这份文档不再描述“计划做什么”，而是说明“现在已经是什么”。

---

## 二、已落地的模块

`summer-sql-rewrite` 当前已包含：

```text
src/
├── context.rs        # SqlRewriteContext / SqlOperation
├── plugin.rs         # SqlRewritePlugin
├── registry.rs       # PluginRegistry
├── extensions.rs     # 类型安全 Extensions
├── table.rs          # QualifiedTableName
├── helpers.rs        # 常用 AST helper
├── pipeline.rs       # parse -> plugin chain -> render
├── connection.rs     # RewriteConnection
├── transaction.rs    # RewriteTransaction
├── configurator.rs   # Summer AppBuilder 集成
└── web/
    ├── middleware.rs # SqlRewriteLayer
    └── extractor.rs  # RewriteDbConn
```

---

## 三、运行模型

### 3.1 非 Web 场景

用于后台任务、脚本、定时任务：

```rust
let db: DatabaseConnection = ...;
let registry: PluginRegistry = ...;
let conn = RewriteConnection::new(db, registry, Extensions::new());
```

这时 `Extensions` 是静态提供的，通常来自调用方手动构造。

### 3.2 Web 场景

用于请求级上下文注入：

```rust
SummerSqlRewritePlugin
  -> 注册 RewriteConnection component（非 Web 默认连接）
  -> 注册 SqlRewriteLayer router layer（Web 请求级连接）
```

进入 Web 请求后：

1. `SqlRewriteLayer` 从 `request.extensions` 收集上下文
2. 构建请求级 `Extensions`
3. 创建带请求上下文的 `RewriteConnection`
4. 注入到 `request.extensions`
5. 业务 handler 通过 `RewriteDbConn` 提取

---

## 四、请求上下文注入规则

### 4.1 内建注入

当前内建注入只有一项：

- `UserSession`（当启用 `summer-auth` feature 且请求中存在会话时）

### 4.2 自定义注入

其他请求级上下文应通过 `SqlRewriteRequestExtender` 注入：

```rust
app.sql_rewrite_web_configure(|req_ext, ext| {
    if let Some(session) = req_ext.get::<UserSession>() {
        ext.insert(session.clone());
    }
});
```

这里的设计意图是：

- 内建层只负责“最基础、最稳定”的上下文
- 业务语义的数据由应用自行决定是否注入

---

## 五、当前能力边界

### 5.1 已支持

- `ConnectionTrait`
- `TransactionTrait`
- `StreamTrait`
- 多语句 `execute_unprepared()` 改写
- 子查询中的表提取
- 占位符数量校验
- request-scoped `RewriteDbConn`

### 5.2 当前约束

- `SqlRewriteContext.extensions` 仍是只读引用
- prepared `Statement` 改写只做“占位符数量一致性校验”，不做复杂的占位符重映射
- 非 Web 下通过 `app.get_component::<RewriteConnection>()` 拿到的是“无请求上下文”的基础连接
- Web 下应优先使用 `RewriteDbConn`

---

## 六、与 `summer-sharding` 的关系

当前不是“二选一”的关系，而是“通用层 + 分片层”的组合关系：

- `summer-sql-rewrite` 提供通用改写能力
- `summer-sharding` 在其基础上额外注入 `ShardingRouteInfo`
- 分片执行链中的插件依旧使用统一的 `SqlRewritePlugin`

也就是说：

- 普通业务表可单独使用 `summer-sql-rewrite`
- 分片表可在 `summer-sharding` 中继续复用同一套插件接口

---

## 七、文档口径调整

旧设计稿中曾提到一些现在已经不准确的点，这里统一修正：

- `RewriteConnection` 当前版本**已经支持事务包装**
- `RewriteTransaction` 当前版本**已经支持 `StreamTrait`**
- `replace_table` 的核心类型仍保留 `QualifiedTableName`，没有退回为只接受 `&str` 的弱类型 API
- `summer-sharding` 不再维护一套独立的 `QualifiedTableName` / `SqlOperation`

---

## 八、后续只剩文档与体验项

当前未完成的主要不是核心功能，而是外围体验：

- 完整 README
- 更系统的 crate-level 文档
- 更多 doc-test 示例
- 与业务鉴权/上下文的最佳实践文档

如果看到旧讨论里还写着“设计中”或“后续再做”，以这份文档和当前代码为准。
