//! 分片标记插件——把当前 SQL 的逻辑→物理表名记录到 SQL 注释里。
//!
//! 实际的表名替换由 [`crate::rewrite::DefaultSqlRewriter`] 在调用插件之前完成，
//! 这个插件只是把分片路由结果以注释形式标在 SQL 末尾，便于业务侧 / DBA 观察
//! 真实路由命中的表：
//!
//! ```sql
//! -- 原 SQL
//! SELECT * FROM ai.log WHERE create_time > '2026-01-01'
//! -- 改写并打标后（注释由 PluginRegistry 收集统一输出）
//! SELECT * FROM ai.log_202601 WHERE create_time > '2026-01-01'
//!   /* sharding:ds=ds_ai;ai.log=>ai.log_202601 */
//! ```
//!
//! 该插件读 [`crate::rewrite_plugin::ShardingRouteInfo`] 扩展，
//! 这个扩展由 `DefaultSqlRewriter` 在执行 plugin chain 之前注入。

use crate::sql_rewrite::{Result, SqlRewriteContext, SqlRewritePlugin};

use crate::rewrite_plugin::ShardingRouteInfo;

#[derive(Debug, Default)]
pub struct TableShardingPlugin;

impl TableShardingPlugin {
    pub fn new() -> Self {
        Self
    }
}

impl SqlRewritePlugin for TableShardingPlugin {
    fn name(&self) -> &str {
        "table_sharding"
    }

    fn order(&self) -> i32 {
        30
    }

    fn matches(&self, ctx: &SqlRewriteContext) -> bool {
        ctx.extension::<ShardingRouteInfo>().is_some()
    }

    fn rewrite(&self, ctx: &mut SqlRewriteContext) -> Result<()> {
        let route = match ctx.extension::<ShardingRouteInfo>() {
            Some(r) => r.clone(),
            None => return Ok(()),
        };
        let mut parts = vec![format!("ds={}", route.datasource)];
        for pair in &route.table_rewrites {
            parts.push(format!("{}=>{}", pair.logic, pair.actual));
        }
        if route.is_fanout {
            parts.push("fanout=true".to_string());
        }
        ctx.append_comment(format!("sharding:{}", parts.join(";")).as_str());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::sql_rewrite::{Extensions, SqlOperation, SqlRewriteContext};
    use sqlparser::dialect::PostgreSqlDialect;
    use sqlparser::parser::Parser;

    use super::*;
    use crate::rewrite_plugin::{ShardingRouteInfo, TableRewritePair};

    #[test]
    fn writes_route_comment_when_extension_present() {
        let plugin = TableShardingPlugin::new();
        let mut stmt = Parser::parse_sql(&PostgreSqlDialect {}, "SELECT * FROM ai.log")
            .expect("parse")
            .remove(0);
        let mut ext = Extensions::new();
        ext.insert(ShardingRouteInfo {
            datasource: "ds_ai".to_string(),
            table_rewrites: vec![TableRewritePair {
                logic: "ai.log".to_string(),
                actual: "ai.log_202601".to_string(),
            }],
            is_fanout: false,
        });
        let mut ctx = SqlRewriteContext {
            statement: &mut stmt,
            operation: SqlOperation::Select,
            tables: vec!["ai.log".to_string()],
            original_sql: "",
            extensions: &mut ext,
            comments: Vec::new(),
        };

        plugin.rewrite(&mut ctx).expect("rewrite");

        assert_eq!(ctx.comments.len(), 1);
        let comment = &ctx.comments[0];
        assert!(comment.contains("ds=ds_ai"), "comment: {comment}");
        assert!(
            comment.contains("ai.log=>ai.log_202601"),
            "comment: {comment}"
        );
    }

    #[test]
    fn no_op_without_extension() {
        let plugin = TableShardingPlugin::new();
        let mut stmt = Parser::parse_sql(&PostgreSqlDialect {}, "SELECT 1")
            .expect("parse")
            .remove(0);
        let mut ext = Extensions::new();
        let ctx = SqlRewriteContext {
            statement: &mut stmt,
            operation: SqlOperation::Select,
            tables: vec![],
            original_sql: "",
            extensions: &mut ext,
            comments: Vec::new(),
        };
        assert!(!plugin.matches(&ctx));
    }
}
