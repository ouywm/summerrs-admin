use std::ops::ControlFlow;

use sea_orm::{DbBackend, Statement, Values};
use sqlparser::{
    ast::{
        Expr, ObjectName, Statement as AstStatement, Value as AstValue, visit_expressions_mut,
        visit_relations,
    },
    dialect::{GenericDialect, MySqlDialect, PostgreSqlDialect, SQLiteDialect},
    parser::Parser,
};

use crate::{
    context::{SqlOperation, SqlRewriteContext},
    error::{Result, SqlRewriteError},
    extensions::Extensions,
    helpers,
    registry::PluginRegistry,
};

pub fn rewrite_statement(
    stmt: Statement,
    registry: &PluginRegistry,
    extensions: &Extensions,
) -> Result<Statement> {
    if registry.is_empty() {
        return Ok(stmt);
    }

    let sql = stmt.sql.clone();
    let mut ast = parse_sql(stmt.db_backend, sql.as_str())?;
    if ast.is_empty() {
        return Ok(stmt);
    }
    if ast.len() > 1 {
        return Err(SqlRewriteError::Rewrite(
            "prepared statement rewrite does not support multiple SQL statements".to_string(),
        ));
    }

    let mut parsed = ast.remove(0);
    if let Some(values) = stmt.values.as_ref() {
        tag_prepared_placeholders(&mut parsed, stmt.db_backend, values)?;
    }
    let comments = rewrite_ast_statement(&mut parsed, sql.as_str(), registry, extensions)?;
    let rewritten_values = stmt
        .values
        .map(|values| remap_prepared_values(&mut parsed, stmt.db_backend, values))
        .transpose()?
        .filter(|values| !values.0.is_empty());
    let rewritten_sql = helpers::format_with_comments(&parsed.to_string(), &comments);
    let rewritten = Statement {
        sql: rewritten_sql,
        values: rewritten_values,
        db_backend: stmt.db_backend,
    };
    validate_placeholder_alignment(&rewritten)?;
    Ok(rewritten)
}

pub fn rewrite_unprepared_sql(
    sql: &str,
    db_backend: DbBackend,
    registry: &PluginRegistry,
    extensions: &Extensions,
) -> Result<String> {
    if registry.is_empty() {
        return Ok(sql.to_string());
    }

    let ast = parse_sql(db_backend, sql)?;
    if ast.is_empty() {
        return Ok(sql.to_string());
    }

    let mut rewritten = Vec::with_capacity(ast.len());
    for mut statement in ast {
        let comments = rewrite_ast_statement(&mut statement, sql, registry, extensions)?;
        rewritten.push(helpers::format_with_comments(
            &statement.to_string(),
            &comments,
        ));
    }

    Ok(rewritten.join("; "))
}

fn parse_sql(db_backend: DbBackend, sql: &str) -> Result<Vec<AstStatement>> {
    match db_backend {
        DbBackend::MySql => Parser::parse_sql(&MySqlDialect {}, sql).map_err(Into::into),
        DbBackend::Postgres => Parser::parse_sql(&PostgreSqlDialect {}, sql).map_err(Into::into),
        DbBackend::Sqlite => Parser::parse_sql(&SQLiteDialect {}, sql).map_err(Into::into),
        _ => Parser::parse_sql(&GenericDialect {}, sql).map_err(Into::into),
    }
}

pub fn detect_operation(stmt: &AstStatement) -> SqlOperation {
    match stmt {
        AstStatement::Query(_) => SqlOperation::Select,
        AstStatement::Insert(_) => SqlOperation::Insert,
        AstStatement::Update { .. } => SqlOperation::Update,
        AstStatement::Delete(_) => SqlOperation::Delete,
        _ => SqlOperation::Other,
    }
}

pub fn extract_tables(stmt: &AstStatement) -> Vec<String> {
    let mut tables = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let _ = visit_relations(stmt, |relation: &ObjectName| {
        let table = relation.to_string();
        let key = table.to_ascii_lowercase();
        if seen.insert(key) {
            tables.push(table);
        }
        ControlFlow::<()>::Continue(())
    });
    tables
}

fn rewrite_ast_statement(
    parsed: &mut AstStatement,
    original_sql: &str,
    registry: &PluginRegistry,
    extensions: &Extensions,
) -> Result<Vec<String>> {
    let mut local_extensions = extensions.clone();
    let mut ctx = SqlRewriteContext {
        operation: detect_operation(parsed),
        tables: extract_tables(parsed),
        original_sql,
        statement: parsed,
        extensions: &mut local_extensions,
        comments: Vec::new(),
    };
    registry.rewrite_all(&mut ctx)?;
    Ok(ctx.comments)
}

fn validate_placeholder_alignment(stmt: &Statement) -> Result<()> {
    let Some(values) = &stmt.values else {
        return Ok(());
    };

    let expected = values.0.len();
    let actual = count_placeholders(stmt.db_backend, stmt.sql.as_str());
    if actual == expected {
        return Ok(());
    }

    Err(SqlRewriteError::Rewrite(format!(
        "sql rewrite placeholder mismatch: sql expects {actual} bound values but statement carries {expected}"
    )))
}

fn count_placeholders(db_backend: DbBackend, sql: &str) -> usize {
    match db_backend {
        DbBackend::Postgres => count_postgres_placeholders(sql),
        DbBackend::MySql | DbBackend::Sqlite => sql.matches('?').count(),
        _ => 0,
    }
}

fn count_postgres_placeholders(sql: &str) -> usize {
    let mut max_index = 0_usize;
    let bytes = sql.as_bytes();
    let mut idx = 0;
    while idx < bytes.len() {
        if bytes[idx] != b'$' {
            idx += 1;
            continue;
        }
        let start = idx + 1;
        let mut end = start;
        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1;
        }
        if end > start
            && let Ok(value) = sql[start..end].parse::<usize>()
        {
            max_index = max_index.max(value);
        }
        idx = end.max(start);
    }
    max_index
}

const INTERNAL_PLACEHOLDER_PREFIX: &str = "__summer_sql_rewrite_bind_";

fn tag_prepared_placeholders(
    statement: &mut AstStatement,
    db_backend: DbBackend,
    values: &Values,
) -> Result<()> {
    let mut positional_index = 0_usize;
    let mut postgres_max = 0_usize;
    let visit = visit_expressions_mut(statement, |expr| {
        let Expr::Value(AstValue::Placeholder(placeholder)) = expr else {
            return ControlFlow::<SqlRewriteError>::Continue(());
        };

        match db_backend {
            DbBackend::MySql | DbBackend::Sqlite => {
                positional_index += 1;
                *placeholder = internal_placeholder(positional_index);
            }
            DbBackend::Postgres => {
                let Some(index) = parse_postgres_placeholder(placeholder.as_str()) else {
                    return ControlFlow::Break(SqlRewriteError::Rewrite(format!(
                        "unsupported prepared postgres placeholder `{placeholder}` during sql rewrite"
                    )));
                };
                postgres_max = postgres_max.max(index);
                *placeholder = internal_placeholder(index);
            }
            _ => {}
        }
        ControlFlow::Continue(())
    });

    if let ControlFlow::Break(error) = visit {
        return Err(error);
    }

    match db_backend {
        DbBackend::MySql | DbBackend::Sqlite if positional_index != values.0.len() => {
            Err(SqlRewriteError::Rewrite(format!(
                "sql rewrite placeholder mismatch before rewrite: sql expects {positional_index} bound values but statement carries {}",
                values.0.len()
            )))
        }
        DbBackend::Postgres if postgres_max > values.0.len() => {
            Err(SqlRewriteError::Rewrite(format!(
                "sql rewrite placeholder mismatch before rewrite: sql expects {postgres_max} bound values but statement carries {}",
                values.0.len()
            )))
        }
        _ => Ok(()),
    }
}

fn remap_prepared_values(
    statement: &mut AstStatement,
    db_backend: DbBackend,
    original_values: Values,
) -> Result<Values> {
    let source_values = original_values.0;
    let mut remapped_values = Vec::new();
    let mut postgres_index_map = std::collections::BTreeMap::<usize, usize>::new();

    let visit = visit_expressions_mut(statement, |expr| {
        let Expr::Value(AstValue::Placeholder(placeholder)) = expr else {
            return ControlFlow::<SqlRewriteError>::Continue(());
        };

        match db_backend {
            DbBackend::MySql | DbBackend::Sqlite => {
                let Some(index) = parse_internal_placeholder(placeholder.as_str()) else {
                    return ControlFlow::Break(SqlRewriteError::Rewrite(format!(
                        "prepared positional placeholder `{placeholder}` cannot be remapped after sql rewrite; keep original placeholder nodes instead of rebuilding fresh `?` placeholders"
                    )));
                };
                let Some(value) = source_values.get(index - 1).cloned() else {
                    return ControlFlow::Break(SqlRewriteError::Rewrite(format!(
                        "sql rewrite placeholder `{placeholder}` points to missing bound value {index}"
                    )));
                };
                remapped_values.push(value);
                *placeholder = "?".to_string();
            }
            DbBackend::Postgres => {
                let Some(original_index) = parse_internal_placeholder(placeholder.as_str())
                    .or_else(|| parse_postgres_placeholder(placeholder.as_str()))
                else {
                    return ControlFlow::Break(SqlRewriteError::Rewrite(format!(
                        "unsupported prepared postgres placeholder `{placeholder}` after sql rewrite"
                    )));
                };
                let Some(value) = source_values.get(original_index - 1).cloned() else {
                    return ControlFlow::Break(SqlRewriteError::Rewrite(format!(
                        "sql rewrite placeholder `{placeholder}` points to missing bound value {original_index}"
                    )));
                };
                let new_index = *postgres_index_map.entry(original_index).or_insert_with(|| {
                    remapped_values.push(value);
                    remapped_values.len()
                });
                *placeholder = format!("${new_index}");
            }
            _ => {}
        }

        ControlFlow::Continue(())
    });

    if let ControlFlow::Break(error) = visit {
        return Err(error);
    }

    Ok(Values(remapped_values))
}

fn internal_placeholder(index: usize) -> String {
    format!("{INTERNAL_PLACEHOLDER_PREFIX}{index}")
}

fn parse_internal_placeholder(value: &str) -> Option<usize> {
    value
        .strip_prefix(INTERNAL_PLACEHOLDER_PREFIX)?
        .parse()
        .ok()
}

fn parse_postgres_placeholder(value: &str) -> Option<usize> {
    value.strip_prefix('$')?.parse().ok()
}

#[cfg(test)]
mod tests {
    use sea_orm::DbBackend;
    use sqlparser::{
        ast::{BinaryOperator, Expr, SetExpr, Statement as AstStatement},
        dialect::PostgreSqlDialect,
        parser::Parser,
    };

    use super::{extract_tables, rewrite_statement, rewrite_unprepared_sql};
    use crate::{extensions::Extensions, plugin::SqlRewritePlugin, registry::PluginRegistry};

    struct CommentPlugin;

    #[derive(Clone)]
    struct DerivedTenant(&'static str);

    impl SqlRewritePlugin for CommentPlugin {
        fn name(&self) -> &str {
            "comment"
        }

        fn matches(&self, _ctx: &crate::context::SqlRewriteContext) -> bool {
            true
        }

        fn rewrite(&self, ctx: &mut crate::context::SqlRewriteContext) -> crate::Result<()> {
            ctx.append_comment("test=true");
            Ok(())
        }
    }

    struct StoreDerivedTenantPlugin;

    impl SqlRewritePlugin for StoreDerivedTenantPlugin {
        fn name(&self) -> &str {
            "store_derived_tenant"
        }

        fn order(&self) -> i32 {
            10
        }

        fn matches(&self, _ctx: &crate::context::SqlRewriteContext) -> bool {
            true
        }

        fn rewrite(&self, ctx: &mut crate::context::SqlRewriteContext) -> crate::Result<()> {
            ctx.insert_extension(DerivedTenant("tenant=derived"));
            Ok(())
        }
    }

    struct ReadDerivedTenantPlugin;

    impl SqlRewritePlugin for ReadDerivedTenantPlugin {
        fn name(&self) -> &str {
            "read_derived_tenant"
        }

        fn order(&self) -> i32 {
            20
        }

        fn matches(&self, _ctx: &crate::context::SqlRewriteContext) -> bool {
            true
        }

        fn rewrite(&self, ctx: &mut crate::context::SqlRewriteContext) -> crate::Result<()> {
            let tenant = ctx
                .extension::<DerivedTenant>()
                .expect("derived tenant extension");
            ctx.append_comment(tenant.0);
            Ok(())
        }
    }

    #[test]
    fn pipeline_rewrites_and_appends_comments() {
        let mut registry = PluginRegistry::new();
        registry.register(CommentPlugin);
        let stmt = sea_orm::Statement::from_string(DbBackend::Postgres, "SELECT * FROM users");
        let rewritten = rewrite_statement(stmt, &registry, &Extensions::new()).expect("rewrite");
        assert_eq!(rewritten.sql, "SELECT * FROM users /* test=true */");
    }

    #[test]
    fn pipeline_respects_mysql_backend_dialect() {
        let mut registry = PluginRegistry::new();
        registry.register(CommentPlugin);
        let stmt = sea_orm::Statement::from_string(DbBackend::MySql, "SELECT * FROM users");
        let rewritten = rewrite_statement(stmt, &registry, &Extensions::new()).expect("rewrite");
        assert_eq!(rewritten.db_backend, DbBackend::MySql);
        assert_eq!(rewritten.sql, "SELECT * FROM users /* test=true */");
    }

    struct ReplaceWithInvalidPlaceholderPlugin;

    struct SwapMysqlPredicateOrderPlugin;

    impl SqlRewritePlugin for SwapMysqlPredicateOrderPlugin {
        fn name(&self) -> &str {
            "swap_mysql_predicates"
        }

        fn matches(&self, _ctx: &crate::context::SqlRewriteContext) -> bool {
            true
        }

        fn rewrite(&self, ctx: &mut crate::context::SqlRewriteContext) -> crate::Result<()> {
            let AstStatement::Query(query) = ctx.statement else {
                panic!("expected query");
            };
            let SetExpr::Select(select) = query.body.as_mut() else {
                panic!("expected select");
            };
            let Some(Expr::BinaryOp { left, op, right }) = select.selection.as_mut() else {
                panic!("expected binary op");
            };
            assert_eq!(*op, BinaryOperator::And);
            std::mem::swap(left, right);
            Ok(())
        }
    }

    struct DropUnusedPostgresPlaceholderPlugin;

    impl SqlRewritePlugin for DropUnusedPostgresPlaceholderPlugin {
        fn name(&self) -> &str {
            "drop_unused_postgres_placeholder"
        }

        fn matches(&self, _ctx: &crate::context::SqlRewriteContext) -> bool {
            true
        }

        fn rewrite(&self, ctx: &mut crate::context::SqlRewriteContext) -> crate::Result<()> {
            *ctx.statement =
                Parser::parse_sql(&PostgreSqlDialect {}, "SELECT * FROM users WHERE name = $1")
                    .expect("parse replacement")
                    .remove(0);
            Ok(())
        }
    }

    impl SqlRewritePlugin for ReplaceWithInvalidPlaceholderPlugin {
        fn name(&self) -> &str {
            "invalid_placeholder"
        }

        fn matches(&self, _ctx: &crate::context::SqlRewriteContext) -> bool {
            true
        }

        fn rewrite(&self, ctx: &mut crate::context::SqlRewriteContext) -> crate::Result<()> {
            *ctx.statement =
                Parser::parse_sql(&PostgreSqlDialect {}, "SELECT * FROM users WHERE id = $2")
                    .expect("parse replacement")
                    .remove(0);
            Ok(())
        }
    }

    #[test]
    fn extract_tables_includes_subquery_relations_and_dedups() {
        let stmt = Parser::parse_sql(
            &PostgreSqlDialect {},
            "SELECT * FROM users WHERE id IN (SELECT user_id FROM orders WHERE shop_id IN (SELECT id FROM shops)) AND EXISTS (SELECT 1 FROM orders WHERE orders.user_id = users.id)",
        )
        .expect("parse")
        .remove(0);

        let tables = extract_tables(&stmt);

        assert_eq!(tables, vec!["users", "orders", "shops"]);
    }

    #[test]
    fn pipeline_rejects_placeholder_mismatch_after_rewrite() {
        let mut registry = PluginRegistry::new();
        registry.register(ReplaceWithInvalidPlaceholderPlugin);
        let stmt = sea_orm::Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT * FROM users WHERE id = $1",
            [1_i64.into()],
        );

        let error =
            rewrite_statement(stmt, &registry, &Extensions::new()).expect_err("should reject");

        assert!(error.to_string().contains("placeholder"));
    }

    #[test]
    fn rewrite_unprepared_sql_preserves_multiple_statements() {
        let mut registry = PluginRegistry::new();
        registry.register(CommentPlugin);

        let rewritten = rewrite_unprepared_sql(
            "SELECT * FROM users; SELECT * FROM orders",
            DbBackend::Postgres,
            &registry,
            &Extensions::new(),
        )
        .expect("rewrite");

        assert!(rewritten.contains("SELECT * FROM users /* test=true */"));
        assert!(rewritten.contains("SELECT * FROM orders /* test=true */"));
        assert!(rewritten.contains(';'));
    }

    #[test]
    fn pipeline_allows_upstream_plugin_to_publish_extensions_for_downstream_plugins() {
        let mut registry = PluginRegistry::new();
        registry.register(StoreDerivedTenantPlugin);
        registry.register(ReadDerivedTenantPlugin);

        let stmt = sea_orm::Statement::from_string(DbBackend::Postgres, "SELECT * FROM users");
        let rewritten = rewrite_statement(stmt, &registry, &Extensions::new()).expect("rewrite");

        assert_eq!(rewritten.sql, "SELECT * FROM users /* tenant=derived */");
    }

    #[test]
    fn pipeline_remaps_mysql_placeholder_values_when_predicates_reorder() {
        let mut registry = PluginRegistry::new();
        registry.register(SwapMysqlPredicateOrderPlugin);

        let stmt = sea_orm::Statement::from_sql_and_values(
            DbBackend::MySql,
            "SELECT * FROM users WHERE name = ? AND age = ?",
            ["Alice".into(), 42_i64.into()],
        );

        let rewritten = rewrite_statement(stmt, &registry, &Extensions::new()).expect("rewrite");

        assert_eq!(
            rewritten.to_string(),
            "SELECT * FROM users WHERE age = 42 AND name = 'Alice'"
        );
    }

    #[test]
    fn pipeline_compacts_postgres_placeholder_values_when_unused_bindings_are_removed() {
        let mut registry = PluginRegistry::new();
        registry.register(DropUnusedPostgresPlaceholderPlugin);

        let stmt = sea_orm::Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT * FROM users WHERE name = $1 AND age = $2",
            ["Alice".into(), 42_i64.into()],
        );

        let rewritten = rewrite_statement(stmt, &registry, &Extensions::new()).expect("rewrite");

        assert_eq!(rewritten.sql, "SELECT * FROM users WHERE name = $1");
        assert_eq!(
            rewritten.to_string(),
            "SELECT * FROM users WHERE name = 'Alice'"
        );
        assert_eq!(rewritten.values.as_ref().expect("values").0.len(), 1);
    }
}
