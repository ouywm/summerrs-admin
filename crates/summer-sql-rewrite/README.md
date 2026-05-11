# summer-sql-rewrite

通用 SQL 改写插件框架。

提供 `SqlRewritePlugin` trait、`PluginRegistry`、AST 操作辅助函数等通用能力，
被 `summer-sharding` 作为底层能力复用。本 crate **不**提供 DB 连接封装——
所有连接（`ShardingConnection` 等）由上层 crate 提供。

## 能力

- AST 级 SQL 插件链：`SqlRewritePlugin` trait + `PluginRegistry`
- 类型安全的请求级容器 `Extensions`
- 框架层表名过滤：trait 的 `tables()` / `skip_tables()` 由 registry 统一处理
- AST 操作辅助 `helpers`：`append_where` / `build_eq_int_expr` / `replace_table_qualified` 等
- `QualifiedTableName`：分离的 schema/table 名表示，含大小写不敏感匹配
- 探针插件 `ProbePlugin`：不改 SQL，只统计命中次数，用于集成测试
- 与 Summer `AppBuilder` 的集成：`SqlRewriteConfigurator` trait 提供 `.sql_rewrite_configure(|reg| ...)`

## 快速开始

### 实现一个插件

```rust,no_run
use summer_sql_rewrite::{
    QualifiedTableName, Result, SqlRewriteContext, SqlRewritePlugin,
};

struct AuditCommentPlugin {
    tables: Vec<QualifiedTableName>,
}

impl SqlRewritePlugin for AuditCommentPlugin {
    fn name(&self) -> &str {
        "audit_comment"
    }

    fn tables(&self) -> &[QualifiedTableName] {
        &self.tables
    }

    fn matches(&self, ctx: &SqlRewriteContext) -> bool {
        ctx.is_select()
    }

    fn rewrite(&self, ctx: &mut SqlRewriteContext) -> Result<()> {
        ctx.append_comment("trace=demo");
        Ok(())
    }
}
```

### 使用 helper 操作 AST

```rust
use sqlparser::{dialect::PostgreSqlDialect, parser::Parser};
use summer_sql_rewrite::{QualifiedTableName, helpers};

let mut stmt = Parser::parse_sql(&PostgreSqlDialect {}, "SELECT * FROM biz.orders")
    .unwrap()
    .remove(0);

helpers::replace_table_qualified(
    &mut stmt,
    &QualifiedTableName::parse("biz.orders"),
    &QualifiedTableName::parse("biz.orders_202603"),
);

assert!(stmt.to_string().contains("biz.orders_202603"));
```

### 在 Summer 应用里注册插件

```rust,ignore
use summer::App;
use summer_sharding::SummerShardingPlugin;
use summer_sql_rewrite::SqlRewriteConfigurator;
use summer_sql_rewrite::builtin::ProbePlugin;

App::new()
    .add_plugin(SummerShardingPlugin)
    .sql_rewrite_configure(|registry| {
        registry.register(ProbePlugin::new())
        // .register(YourPlugin::new(...))
    })
    .run()
    .await;
```

`SummerShardingPlugin` 会拉取注册的 `PluginRegistry`，灌入 `ShardingConnection`。
所有进出 `ShardingConnection` 的 SQL 都会按 `order` 顺序应用 matched 插件。

## 与 summer-sharding 的关系

`summer-sharding` 直接复用本 crate：

- `SqlRewritePlugin` / `PluginRegistry` / `SqlRewriteContext` / `SqlOperation`
- `QualifiedTableName` / `Extensions`
- `helpers` AST 操作

分片侧额外注入 `ShardingRouteInfo` 扩展，让业务插件能拿到当前数据源、物理表改写结果和 fanout 信息。
