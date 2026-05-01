use std::{collections::BTreeMap, sync::Arc};

use sqlparser::ast::{BinaryOperator, Expr, Query, SetExpr, Statement as AstStatement};

use crate::{
    algorithm::ShardingValue,
    config::{ShadowConditionKind, ShardingConfig},
    connector::{ShardingHint, statement::StatementContext},
    router::{QualifiedTableName, RoutePlan},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShadowCondition {
    Header {
        key: String,
        value: Option<String>,
    },
    Column {
        column: String,
        value: Option<String>,
    },
    Hint,
}

#[derive(Debug, Clone)]
pub struct ShadowRouter {
    config: ShardingConfig,
}

impl ShadowRouter {
    pub fn new(config: Arc<ShardingConfig>) -> Self {
        Self {
            config: config.as_ref().clone(),
        }
    }

    pub fn should_route(&self, analysis: &StatementContext) -> bool {
        self.should_route_with_headers(analysis, &analysis.shadow_headers)
    }

    pub fn should_route_with_headers(
        &self,
        analysis: &StatementContext,
        headers: &BTreeMap<String, String>,
    ) -> bool {
        if !self.config.shadow.enabled {
            return false;
        }
        if matches!(analysis.hint, Some(ShardingHint::Shadow)) {
            return true;
        }

        self.config
            .shadow
            .conditions
            .iter()
            .any(|condition| match condition.kind {
                ShadowConditionKind::Hint => matches!(analysis.hint, Some(ShardingHint::Shadow)),
                ShadowConditionKind::Header => condition
                    .key
                    .as_ref()
                    .and_then(|key| header_value(headers, key))
                    .is_some_and(|actual| {
                        condition
                            .value
                            .as_ref()
                            .is_none_or(|expected| expected.eq_ignore_ascii_case(actual))
                    }),
                ShadowConditionKind::Column => condition.column.as_ref().is_some_and(|column| {
                    column_condition_matches(analysis, column, condition.value.as_deref())
                }),
            })
    }

    pub fn apply(&self, plan: &mut RoutePlan, analysis: &StatementContext) {
        if !self.should_route_with_headers(analysis, &analysis.shadow_headers) {
            return;
        }

        for target in &mut plan.targets {
            if self.config.shadow.database_mode.enabled {
                if let Some(datasource) = self.config.shadow.database_mode.datasource.as_deref() {
                    target.datasource = datasource.to_string();
                }
            } else if !self.config.shadow.table_mode.enabled {
                target.datasource =
                    format!("{}{}", target.datasource, self.config.shadow.shadow_suffix);
            }

            if self.config.shadow.table_mode.enabled {
                for rewrite in &mut target.table_rewrites {
                    if self
                        .config
                        .shadow_routes_table(rewrite.logic_table.full_name().as_str())
                    {
                        rewrite.actual_table = self.shadow_table(&rewrite.actual_table);
                    }
                }
            }
        }
    }

    pub fn shadow_table(&self, table: &QualifiedTableName) -> QualifiedTableName {
        QualifiedTableName {
            schema: table.schema.clone(),
            table: format!("{}{}", table.table, self.config.shadow.shadow_suffix),
        }
    }
}

fn header_value<'a>(headers: &'a BTreeMap<String, String>, key: &str) -> Option<&'a String> {
    headers
        .get(key)
        .or_else(|| headers.get(&key.to_ascii_lowercase()))
}

fn column_condition_matches(
    analysis: &StatementContext,
    column: &str,
    expected: Option<&str>,
) -> bool {
    analysis
        .exact_condition_value(column)
        .and_then(sharding_value_string)
        .is_some_and(|actual| matches_expected(actual.as_str(), expected))
        || statement_has_matching_in_list(&analysis.ast, column, expected)
}

fn statement_has_matching_in_list(
    statement: &AstStatement,
    column: &str,
    expected: Option<&str>,
) -> bool {
    match statement {
        AstStatement::Query(query) => query_has_matching_in_list(query, column, expected),
        AstStatement::Update {
            selection: Some(selection),
            ..
        } => expr_has_matching_in_list(selection, column, expected),
        AstStatement::Delete(delete) => delete
            .selection
            .as_ref()
            .is_some_and(|selection| expr_has_matching_in_list(selection, column, expected)),
        _ => false,
    }
}

fn query_has_matching_in_list(query: &Query, column: &str, expected: Option<&str>) -> bool {
    query.with.as_ref().is_some_and(|with| {
        with.cte_tables
            .iter()
            .any(|cte| query_has_matching_in_list(&cte.query, column, expected))
    }) || set_expr_has_matching_in_list(query.body.as_ref(), column, expected)
}

fn set_expr_has_matching_in_list(body: &SetExpr, column: &str, expected: Option<&str>) -> bool {
    match body {
        SetExpr::Select(select) => select
            .selection
            .as_ref()
            .is_some_and(|selection| expr_has_matching_in_list(selection, column, expected)),
        SetExpr::Query(query) => query_has_matching_in_list(query, column, expected),
        SetExpr::SetOperation { left, right, .. } => {
            set_expr_has_matching_in_list(left, column, expected)
                || set_expr_has_matching_in_list(right, column, expected)
        }
        _ => false,
    }
}

fn expr_has_matching_in_list(expr: &Expr, column: &str, expected: Option<&str>) -> bool {
    match expr {
        Expr::InList {
            expr,
            list,
            negated,
        } if !negated && expr_matches_column(expr, column) => list
            .iter()
            .filter_map(expr_literal_string)
            .any(|actual| matches_expected(actual.as_str(), expected)),
        Expr::BinaryOp { left, op, right }
            if *op == BinaryOperator::And || *op == BinaryOperator::Or =>
        {
            expr_has_matching_in_list(left, column, expected)
                || expr_has_matching_in_list(right, column, expected)
        }
        Expr::Nested(expr)
        | Expr::UnaryOp { expr, .. }
        | Expr::IsNull(expr)
        | Expr::IsNotNull(expr)
        | Expr::Cast { expr, .. } => expr_has_matching_in_list(expr, column, expected),
        Expr::Between {
            expr, low, high, ..
        } => {
            expr_has_matching_in_list(expr, column, expected)
                || expr_has_matching_in_list(low, column, expected)
                || expr_has_matching_in_list(high, column, expected)
        }
        Expr::Case {
            operand,
            conditions,
            results,
            else_result,
            ..
        } => {
            operand
                .as_ref()
                .is_some_and(|operand| expr_has_matching_in_list(operand, column, expected))
                || conditions
                    .iter()
                    .any(|condition| expr_has_matching_in_list(condition, column, expected))
                || results
                    .iter()
                    .any(|result| expr_has_matching_in_list(result, column, expected))
                || else_result
                    .as_ref()
                    .is_some_and(|result| expr_has_matching_in_list(result, column, expected))
        }
        _ => false,
    }
}

fn expr_matches_column(expr: &Expr, column: &str) -> bool {
    match expr {
        Expr::Identifier(identifier) => identifier.value.eq_ignore_ascii_case(column),
        Expr::CompoundIdentifier(parts) => parts
            .last()
            .is_some_and(|part| part.value.eq_ignore_ascii_case(column)),
        Expr::Nested(expr) => expr_matches_column(expr, column),
        _ => false,
    }
}

fn expr_literal_string(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Value(sqlparser::ast::Value::Number(number, _)) => Some(number.clone()),
        Expr::Value(sqlparser::ast::Value::SingleQuotedString(value))
        | Expr::Value(sqlparser::ast::Value::DoubleQuotedString(value))
        | Expr::Value(sqlparser::ast::Value::EscapedStringLiteral(value))
        | Expr::Value(sqlparser::ast::Value::UnicodeStringLiteral(value))
        | Expr::Value(sqlparser::ast::Value::NationalStringLiteral(value)) => Some(value.clone()),
        Expr::Value(sqlparser::ast::Value::Boolean(value)) => Some(i64::from(*value).to_string()),
        Expr::Nested(expr) => expr_literal_string(expr),
        _ => None,
    }
}

fn sharding_value_string(value: &ShardingValue) -> Option<String> {
    match value {
        ShardingValue::Str(text) => Some(text.clone()),
        ShardingValue::Int(number) => Some(number.to_string()),
        ShardingValue::DateTime(datetime) => Some(datetime.to_rfc3339()),
        ShardingValue::Null => None,
    }
}

fn matches_expected(actual: &str, expected: Option<&str>) -> bool {
    expected.is_none_or(|expected| expected.eq_ignore_ascii_case(actual))
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, sync::Arc};

    use sea_orm::{DbBackend, Statement};

    use crate::{
        config::ShardingConfig,
        connector::{ShardingHint, analyze_statement},
        shadow::ShadowRouter,
    };

    #[test]
    fn shadow_router_routes_by_hint_and_column_condition() {
        let config = Arc::new(
            ShardingConfig::from_test_str(
                r#"
                [datasources.ds_ai]
                uri = "mock://ai"
                schema = "ai"
                role = "primary"

                [shadow]
                enabled = true
                shadow_suffix = "_shadow"

                  [shadow.table_mode]
                  enabled = true
                  tables = ["ai.log"]

                  [[shadow.conditions]]
                  type = "column"
                  column = "is_shadow"
                  value = "1"
                "#,
            )
            .expect("config"),
        );
        let router = ShadowRouter::new(config);

        let mut analysis = analyze_statement(&Statement::from_string(
            DbBackend::Postgres,
            "SELECT * FROM ai.log WHERE is_shadow = 1",
        ))
        .expect("analysis");
        assert!(router.should_route(&analysis));

        analysis.hint = Some(ShardingHint::Shadow);
        assert!(router.should_route_with_headers(&analysis, &BTreeMap::new()));
    }

    #[test]
    fn shadow_router_routes_when_column_condition_uses_in_list() {
        let config = Arc::new(
            ShardingConfig::from_test_str(
                r#"
                [datasources.ds_ai]
                uri = "mock://ai"
                schema = "ai"
                role = "primary"

                [shadow]
                enabled = true
                shadow_suffix = "_shadow"

                  [[shadow.conditions]]
                  type = "column"
                  column = "is_shadow"
                  value = "1"
                "#,
            )
            .expect("config"),
        );
        let router = ShadowRouter::new(config);

        let analysis = analyze_statement(&Statement::from_string(
            DbBackend::Postgres,
            "SELECT * FROM ai.log WHERE is_shadow IN (1, 2)",
        ))
        .expect("analysis");

        assert!(
            router.should_route(&analysis),
            "shadow column conditions should match IN-list predicates"
        );
    }

    #[tokio::test]
    async fn shadow_router_routes_by_header_context() {
        let config = Arc::new(
            ShardingConfig::from_test_str(
                r#"
                [datasources.ds_ai]
                uri = "mock://ai"
                schema = "ai"
                role = "primary"

                [shadow]
                enabled = true
                shadow_suffix = "_shadow"

                  [[shadow.conditions]]
                  type = "header"
                  key = "X-Shadow"
                  value = "true"
                "#,
            )
            .expect("config"),
        );
        let router = ShadowRouter::new(config);

        let mut analysis = analyze_statement(&Statement::from_string(
            DbBackend::Postgres,
            "SELECT * FROM ai.log",
        ))
        .expect("analysis");

        assert!(!router.should_route(&analysis));

        analysis.shadow_headers = BTreeMap::from([("X-Shadow".to_string(), "true".to_string())]);
        assert!(router.should_route(&analysis));
    }
}
