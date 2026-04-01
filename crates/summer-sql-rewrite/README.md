# summer-sql-rewrite

通用 SQL AST 改写层。

它把 SQL 改写能力从分片运行时中抽离出来，既可以单独包裹普通 `DatabaseConnection` 使用，也可以在 `summer-sharding` 中作为底层能力继续复用。

## 能力

- AST 级 SQL 插件链
- 类型安全 `Extensions`
- `RewriteConnection`
- `RewriteTransaction`
- `StreamTrait`
- Web 中间件 `SqlRewriteLayer`
- 请求提取器 `RewriteDbConn`
- 与 Summer `AppBuilder` 的集成插件 `SummerSqlRewritePlugin`

## 快速开始

### 直接包裹普通连接

```rust,no_run
use sea_orm::{ConnectionTrait, DbBackend, MockDatabase, Statement};
use summer_sql_rewrite::{
    Extensions, PluginRegistry, RewriteConnection, SqlRewriteContext, SqlRewritePlugin,
};

struct AuditCommentPlugin;

impl SqlRewritePlugin for AuditCommentPlugin {
    fn name(&self) -> &str {
        "audit_comment"
    }

    fn matches(&self, ctx: &SqlRewriteContext) -> bool {
        ctx.is_select()
    }

    fn rewrite(&self, ctx: &mut SqlRewriteContext) -> summer_sql_rewrite::Result<()> {
        ctx.append_comment("trace=demo");
        Ok(())
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), sea_orm::DbErr> {
    let db = MockDatabase::new(DbBackend::Postgres).into_connection();
    let mut registry = PluginRegistry::new();
    registry.register(AuditCommentPlugin);

    let conn = RewriteConnection::new(db, registry, Extensions::new());

    let _ = conn
        .query_all_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT * FROM users",
        ))
        .await?;

    Ok(())
}
```

### 使用 helper 操作 AST

```rust
use sqlparser::{dialect::PostgreSqlDialect, parser::Parser};
use summer_sql_rewrite::{helpers, QualifiedTableName};

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

## Web 集成

启用 `web` feature 后，可通过 `SqlRewriteLayer` 在请求级构建改写连接。

典型流程：

1. 认证层把 `UserSession` 放入 `request.extensions`
2. `SqlRewriteLayer` 读取请求扩展，构建请求级 `Extensions`
3. handler 通过 `RewriteDbConn` 提取带上下文的连接

说明：

- 当前 builtin 注入只包含 `UserSession`
- 其他上下文应通过 `SqlRewriteRequestExtender` 手动注入

## 非 Web 与 Web 的区别

- 非 Web：`RewriteConnection` 通常由调用方直接构造
- Web：应优先通过 `RewriteDbConn` 提取器获取请求级连接
- `SummerSqlRewritePlugin` 注册的 `RewriteConnection` component 更适合后台任务、CLI、定时任务等非请求场景

## 与 summer-sharding 的关系

`summer-sharding` 直接复用本 crate 的：

- `SqlRewritePlugin`
- `SqlRewriteContext`
- `SqlOperation`
- `QualifiedTableName`
- `Extensions`

分片侧只额外注入 `ShardingRouteInfo`，用于暴露当前数据源、物理表改写结果和 fanout 信息。
