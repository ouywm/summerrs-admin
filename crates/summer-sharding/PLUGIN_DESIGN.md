# SQL 改写插件体系说明

> **版本**: v2.0  
> **更新日期**: 2026年3月31日  
> **状态**: 已实现  
> **说明**: 这份文档用于描述当前生效的实现，不再是早期“内嵌在 `summer-sharding` 内部”的设计稿。

---

## 一、当前结论

`summer-sharding` 的 SQL 改写插件体系已经不再是一个“仅供分片内部使用”的独立实现，而是建立在独立 crate [`summer-sql-rewrite`](../summer-sql-rewrite/README.md) 之上：

- 通用 SQL 改写能力在 `summer-sql-rewrite` 中实现
- `summer-sharding` 负责在分片执行链路里注入分片上下文
- 分片侧只保留轻量 re-export 与 route-aware 扩展

也就是说：

1. **通用插件接口** 属于 `summer-sql-rewrite`
2. **分片运行时上下文** 属于 `summer-sharding`
3. **两者在运行期组合**，而不是互相复制一套类型系统

---

## 二、当前架构

### 2.1 通用层：`summer-sql-rewrite`

`summer-sql-rewrite` 负责提供以下能力：

- `SqlRewritePlugin`
- `SqlRewriteContext`
- `PluginRegistry`
- `Extensions`
- `QualifiedTableName`
- `SqlOperation`
- `RewriteConnection`
- `RewriteTransaction`
- `SqlRewriteLayer` / `RewriteDbConn`

这些能力不依赖分片概念，可以单独挂在普通 `DatabaseConnection` 上使用。

### 2.2 分片层：`summer-sharding`

`summer-sharding` 在现有分片执行链路中复用上述通用能力，并注入分片路由信息：

```
用户 SQL
  -> Analyze (StatementContext)
  -> Route (RoutePlan)
  -> Built-in Rewrite
     - table/schema rewrite
     - tenant rewrite
     - limit/aggregate rewrite
     - encrypt rewrite
  -> Optional Plugin Rewrite
     - PluginRegistry::rewrite_all(SqlRewriteContext)
     - Extensions 中额外注入 ShardingRouteInfo
  -> SQL Render
  -> Execute / Merge
```

### 2.3 分片专有扩展

分片侧会额外向插件链注入 `ShardingRouteInfo`：

```rust
pub struct ShardingRouteInfo {
    pub datasource: String,
    pub table_rewrites: Vec<TableRewritePair>,
    pub is_fanout: bool,
}

pub struct TableRewritePair {
    pub logic: String,
    pub actual: String,
}
```

插件如果只做通用 SQL 改写，可以完全不关心这个结构；如果需要感知“当前语句被路由到了哪个物理表/数据源”，再从 `Extensions` 中读取它。

---

## 三、类型统一说明

早期设计中，`summer-sharding` 和 `summer-sql-rewrite` 曾各自定义过一套：

- `QualifiedTableName`
- `SqlOperation`

当前实现已经统一：

- `summer-sharding::QualifiedTableName` 直接 re-export `summer_sql_rewrite::QualifiedTableName`
- `summer-sharding::SqlOperation` 直接 re-export `summer_sql_rewrite::SqlOperation`

这样做的目的有两点：

1. 避免相同语义却不同 `TypeId` 的重复类型
2. 让插件 helper、分片路由、改写上下文使用同一套基础类型

---

## 四、对外使用方式

### 4.1 只需要 SQL 改写，不需要分片

直接使用 `summer-sql-rewrite`：

- 后台任务 / CLI：使用 `RewriteConnection`
- Web 请求：使用 `SqlRewriteLayer` + `RewriteDbConn`

### 4.2 需要分片 + 自定义改写

使用 `summer-sharding`：

- 内置分片改写仍由 `DefaultSqlRewriter` 执行
- 应用层通过 `ShardingRewriteConfigurator` 注册插件
- 插件收到的是通用 `SqlRewriteContext`
- 如需分片信息，再从 `Extensions` 中读取 `ShardingRouteInfo`

---

## 五、当前边界

以下是当前实现的明确边界：

- `summer-sql-rewrite` 已支持 `ConnectionTrait`、`TransactionTrait`、`StreamTrait`
- Web builtin 注入目前只自动带入 `UserSession`
- 其他请求级上下文应通过 `SqlRewriteRequestExtender` 注入
- `SqlRewriteContext.extensions` 目前是只读的
- `summer-sharding` 只在“完成路由后”的物理 SQL 上运行插件链

---

## 六、历史说明

这份文档替代的是“插件系统完全内嵌在 `summer-sharding` 内部”的旧设计。旧设计的核心思想没有丢失：

- 仍然支持插件排序
- 仍然支持 AST 级改写
- 仍然支持类型安全上下文传递

变化点在于：

- **通用能力被抽离成独立 crate**
- **分片只保留 route-aware 扩展**
- **基础类型已经统一，不再重复定义**

如果你在查阅旧提交或旧讨论时看到“`RewriteContext` / `QualifiedTableName` 是 sharding 内部独有类型”的说法，以当前文档为准。
