mod aggregate_rewrite;
mod limit_rewrite;
mod schema_rewrite;
mod table_rewrite;

use std::sync::Arc;

use crate::sql_rewrite::SqlRewriteContext;
use sea_orm::Statement;

use crate::{
    config::ShardingConfig,
    connector::statement::StatementContext,
    error::{Result, ShardingError},
    rewrite_plugin::{PluginRegistry, ShardingRouteInfo, TableRewritePair},
    router::RoutePlan,
    tenant::apply_tenant_rewrite,
};

use aggregate_rewrite::apply_aggregate_rewrite;
pub use limit_rewrite::inflate_limit_for_fanout;
pub use schema_rewrite::apply_schema_rewrite;
pub use table_rewrite::rewrite_table_names;

pub trait SqlRewriter: Send + Sync + 'static {
    fn rewrite(
        &self,
        stmt: &Statement,
        analysis: &StatementContext,
        plan: &RoutePlan,
        plugin_registry: Option<&PluginRegistry>,
    ) -> Result<Vec<Statement>>;
}

#[derive(Debug, Clone)]
pub struct DefaultSqlRewriter {
    config: ShardingConfig,
}

impl DefaultSqlRewriter {
    pub fn new(config: Arc<ShardingConfig>) -> Self {
        Self {
            config: config.as_ref().clone(),
        }
    }
}

impl SqlRewriter for DefaultSqlRewriter {
    fn rewrite(
        &self,
        stmt: &Statement,
        analysis: &StatementContext,
        plan: &RoutePlan,
        plugin_registry: Option<&PluginRegistry>,
    ) -> Result<Vec<Statement>> {
        let mut rewritten = Vec::with_capacity(plan.targets.len().max(1));

        for target in &plan.targets {
            let mut parsed = analysis.ast.clone();
            for rewrite in &target.table_rewrites {
                let logic_table = &rewrite.logic_table;
                let actual_table = &rewrite.actual_table;
                rewrite_table_names(&mut parsed, logic_table, actual_table);
                apply_schema_rewrite(&mut parsed, logic_table, actual_table);
            }

            if plan.targets.len() > 1 {
                inflate_limit_for_fanout(&mut parsed, plan.limit, plan.offset);
                apply_aggregate_rewrite(&mut parsed, analysis);
            }

            let mut statement = stmt.clone();
            if let Some(tenant) = analysis.tenant.as_ref() {
                apply_tenant_rewrite(&mut parsed, tenant, &self.config, &plan.logic_tables);
            }

            let mut comments = Vec::new();
            if let Some(registry) = plugin_registry {
                let mut extensions = analysis
                    .access_context
                    .as_ref()
                    .map(|ctx| ctx.extensions.clone())
                    .unwrap_or_default();
                extensions.insert(ShardingRouteInfo {
                    datasource: target.datasource.clone(),
                    table_rewrites: target
                        .table_rewrites
                        .iter()
                        .map(|rewrite| TableRewritePair {
                            logic: rewrite.logic_table.full_name(),
                            actual: rewrite.actual_table.full_name(),
                        })
                        .collect(),
                    is_fanout: plan.targets.len() > 1,
                });

                let mut ctx = SqlRewriteContext {
                    statement: &mut parsed,
                    operation: analysis.operation,
                    tables: analysis
                        .tables
                        .iter()
                        .map(|table| table.full_name())
                        .collect(),
                    original_sql: &stmt.sql,
                    extensions: &mut extensions,
                    comments: Vec::new(),
                };
                registry.rewrite_all(&mut ctx)?;
                comments = ctx.comments;
            }

            statement.sql =
                crate::sql_rewrite::helpers::format_with_comments(&parsed.to_string(), &comments);
            rewritten.push(statement);
        }

        if rewritten.is_empty() {
            return Err(ShardingError::Rewrite(
                "route plan does not have any rewritten statements".to_string(),
            ));
        }

        Ok(rewritten)
    }
}
#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sea_orm::{DbBackend, Statement};

    use crate::{
        config::ShardingConfig,
        connector::analyze_statement,
        rewrite::{DefaultSqlRewriter, SqlRewriter},
        router::{
            OrderByItem, QualifiedTableName, RoutePlan, RouteTarget, SqlOperation, TableRewrite,
        },
    };

    #[test]
    fn rewriter_replaces_logic_table_and_inflates_limit() {
        let stmt = Statement::from_string(
            DbBackend::Postgres,
            "SELECT * FROM ai.log WHERE create_time >= $1 AND create_time < $2 ORDER BY create_time DESC LIMIT 10 OFFSET 20",
        );
        let analysis = analyze_statement(&stmt).expect("analysis");
        let plan = RoutePlan {
            operation: SqlOperation::Select,
            logic_tables: vec![QualifiedTableName::parse("ai.log")],
            targets: vec![
                RouteTarget {
                    datasource: "ds_ai".to_string(),
                    table_rewrites: vec![TableRewrite {
                        logic_table: QualifiedTableName::parse("ai.log"),
                        actual_table: QualifiedTableName::parse("ai.log_202602"),
                    }],
                },
                RouteTarget {
                    datasource: "ds_ai".to_string(),
                    table_rewrites: vec![TableRewrite {
                        logic_table: QualifiedTableName::parse("ai.log"),
                        actual_table: QualifiedTableName::parse("ai.log_202603"),
                    }],
                },
            ],
            order_by: vec![OrderByItem {
                column: "create_time".to_string(),
                asc: false,
            }],
            limit: Some(10),
            offset: Some(20),
            broadcast: true,
        };

        let rewritten = DefaultSqlRewriter::new(Arc::new(ShardingConfig::default()))
            .rewrite(&stmt, &analysis, &plan, None)
            .expect("rewrite");

        assert_eq!(rewritten.len(), 2);
        assert!(rewritten[0].sql.contains("ai.log_202602"));
        assert!(rewritten[1].sql.contains("ai.log_202603"));
        assert!(rewritten[0].sql.contains("LIMIT 30"));
        assert!(!rewritten[0].sql.contains("OFFSET 20"));
    }

    #[test]
    fn rewriter_rewrites_alter_table_targets() {
        let stmt = Statement::from_string(
            DbBackend::Postgres,
            "ALTER TABLE ai.log ADD COLUMN archived_at timestamptz",
        );
        let analysis = analyze_statement(&stmt).expect("analysis");
        let plan = RoutePlan {
            operation: SqlOperation::Other,
            logic_tables: vec![QualifiedTableName::parse("ai.log")],
            targets: vec![RouteTarget {
                datasource: "ds_ai".to_string(),
                table_rewrites: vec![TableRewrite {
                    logic_table: QualifiedTableName::parse("ai.log"),
                    actual_table: QualifiedTableName::parse("ai.log_202603"),
                }],
            }],
            order_by: Vec::new(),
            limit: None,
            offset: None,
            broadcast: false,
        };

        let rewritten = DefaultSqlRewriter::new(Arc::new(ShardingConfig::default()))
            .rewrite(&stmt, &analysis, &plan, None)
            .expect("rewrite");

        assert_eq!(rewritten.len(), 1);
        assert!(rewritten[0].sql.contains("ALTER TABLE ai.log_202603"));
    }

    #[test]
    fn rewriter_keeps_logic_table_alias_for_entity_qualified_columns() {
        let stmt = Statement::from_string(
            DbBackend::Postgres,
            r#"SELECT "tenant_case_isolated"."id", "tenant_case_isolated"."title" FROM "test"."tenant_case_isolated" WHERE "tenant_case_isolated"."id" = $1"#,
        );
        let analysis = analyze_statement(&stmt).expect("analysis");
        let plan = RoutePlan {
            operation: SqlOperation::Select,
            logic_tables: vec![QualifiedTableName::parse("test.tenant_case_isolated")],
            targets: vec![RouteTarget {
                datasource: "ds_test".to_string(),
                table_rewrites: vec![TableRewrite {
                    logic_table: QualifiedTableName::parse("test.tenant_case_isolated"),
                    actual_table: QualifiedTableName::parse("test.tenant_case_isolated_tseedtable"),
                }],
            }],
            order_by: Vec::new(),
            limit: None,
            offset: None,
            broadcast: false,
        };

        let rewritten = DefaultSqlRewriter::new(Arc::new(ShardingConfig::default()))
            .rewrite(&stmt, &analysis, &plan, None)
            .expect("rewrite");

        assert_eq!(rewritten.len(), 1);
        assert!(
            rewritten[0]
                .sql
                .contains("FROM test.tenant_case_isolated_tseedtable AS tenant_case_isolated"),
            "rewritten sql: {}",
            rewritten[0].sql
        );
        assert!(
            rewritten[0]
                .sql
                .contains(r#"WHERE "tenant_case_isolated"."id" = $1"#)
        );
    }
}
