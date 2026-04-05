//! SQL 操作便捷函数。

use sqlparser::ast::helpers::attached_token::AttachedToken;
use sqlparser::ast::*;

use crate::table::QualifiedTableName;

pub fn append_where(statement: &mut Statement, condition: Expr) {
    match statement {
        Statement::Query(query) => append_where_to_query(query, condition),
        Statement::Update { selection, .. } => inject_condition(selection, condition),
        Statement::Delete(delete) => inject_condition(&mut delete.selection, condition),
        _ => {}
    }
}

fn append_where_to_query(query: &mut Query, condition: Expr) {
    append_where_to_set_expr(query.body.as_mut(), condition);
}

fn append_where_to_set_expr(body: &mut SetExpr, condition: Expr) {
    match body {
        SetExpr::Select(select) => inject_condition(&mut select.selection, condition),
        SetExpr::Query(inner) => append_where_to_query(inner, condition),
        SetExpr::SetOperation { left, right, .. } => {
            append_where_to_set_expr(left.as_mut(), condition.clone());
            append_where_to_set_expr(right.as_mut(), condition);
        }
        _ => {}
    }
}

fn inject_condition(selection: &mut Option<Expr>, condition: Expr) {
    match selection.take() {
        Some(existing) => *selection = Some(and(existing, condition)),
        None => *selection = Some(condition),
    }
}

pub fn build_eq_expr(column: &str, value: &str) -> Expr {
    Expr::BinaryOp {
        left: Box::new(column_expr(column)),
        op: BinaryOperator::Eq,
        right: Box::new(Expr::Value(Value::SingleQuotedString(value.to_string()))),
    }
}

pub fn build_eq_int_expr(column: &str, value: i64) -> Expr {
    Expr::BinaryOp {
        left: Box::new(column_expr(column)),
        op: BinaryOperator::Eq,
        right: Box::new(Expr::Value(Value::Number(value.to_string(), false))),
    }
}

pub fn build_in_expr(column: &str, values: &[&str]) -> Expr {
    Expr::InList {
        expr: Box::new(column_expr(column)),
        list: values
            .iter()
            .map(|value| Expr::Value(Value::SingleQuotedString((*value).to_string())))
            .collect(),
        negated: false,
    }
}

pub fn build_in_int_expr(column: &str, values: &[i64]) -> Expr {
    Expr::InList {
        expr: Box::new(column_expr(column)),
        list: values
            .iter()
            .map(|value| Expr::Value(Value::Number(value.to_string(), false)))
            .collect(),
        negated: false,
    }
}

pub fn build_not_in_expr(column: &str, values: &[&str]) -> Expr {
    Expr::InList {
        expr: Box::new(column_expr(column)),
        list: values
            .iter()
            .map(|value| Expr::Value(Value::SingleQuotedString((*value).to_string())))
            .collect(),
        negated: true,
    }
}

pub fn build_is_null_expr(column: &str) -> Expr {
    Expr::IsNull(Box::new(column_expr(column)))
}

pub fn build_is_not_null_expr(column: &str) -> Expr {
    Expr::IsNotNull(Box::new(column_expr(column)))
}

pub fn build_between_expr(column: &str, low: &str, high: &str) -> Expr {
    Expr::Between {
        expr: Box::new(column_expr(column)),
        negated: false,
        low: Box::new(Expr::Value(Value::SingleQuotedString(low.to_string()))),
        high: Box::new(Expr::Value(Value::SingleQuotedString(high.to_string()))),
    }
}

pub fn build_like_expr(column: &str, pattern: &str) -> Expr {
    Expr::Like {
        negated: false,
        any: false,
        expr: Box::new(column_expr(column)),
        pattern: Box::new(Expr::Value(Value::SingleQuotedString(pattern.to_string()))),
        escape_char: None,
    }
}

pub fn build_bound_placeholder(index: usize) -> Expr {
    Expr::Value(Value::Placeholder(crate::pipeline::internal_placeholder(
        index,
    )))
}

pub fn build_exists_expr(subquery: Query) -> Expr {
    Expr::Exists {
        subquery: Box::new(subquery),
        negated: false,
    }
}

pub fn and(left: Expr, right: Expr) -> Expr {
    Expr::BinaryOp {
        left: Box::new(left),
        op: BinaryOperator::And,
        right: Box::new(right),
    }
}

pub fn or(left: Expr, right: Expr) -> Expr {
    Expr::Nested(Box::new(Expr::BinaryOp {
        left: Box::new(left),
        op: BinaryOperator::Or,
        right: Box::new(right),
    }))
}

pub fn replace_table(
    statement: &mut Statement,
    from_table: &str,
    to_table: &str,
) -> crate::Result<()> {
    replace_table_qualified(
        statement,
        &QualifiedTableName::parse(from_table),
        &QualifiedTableName::parse(to_table),
    );
    Ok(())
}

pub fn replace_table_qualified(
    statement: &mut Statement,
    from_table: &QualifiedTableName,
    to_table: &QualifiedTableName,
) {
    match statement {
        Statement::Query(query) => rewrite_query(query, from_table, to_table),
        Statement::Insert(insert) => rewrite_insert(insert, from_table, to_table),
        Statement::CreateTable(create_table) => {
            if from_table.matches_object_name(&create_table.name) {
                create_table.name = to_table.to_object_name();
            }
            if let Some(query) = &mut create_table.query {
                rewrite_query(query, from_table, to_table);
            }
        }
        Statement::Update { table, from, .. } => {
            rewrite_table_with_joins(table, from_table, to_table);
            if let Some(from) = from {
                match from {
                    UpdateTableFromKind::BeforeSet(table)
                    | UpdateTableFromKind::AfterSet(table) => {
                        rewrite_table_with_joins(table, from_table, to_table);
                    }
                }
            }
        }
        Statement::Delete(delete) => rewrite_delete(delete, from_table, to_table),
        Statement::AlterTable { name, .. } => {
            if from_table.matches_object_name(name) {
                *name = to_table.to_object_name();
            }
        }
        Statement::Truncate { table_names, .. } => {
            for table in table_names {
                if from_table.matches_object_name(&table.name) {
                    table.name = to_table.to_object_name();
                }
            }
        }
        _ => {}
    }
}

pub fn wrap_subquery(statement: &mut Statement, alias: &str) {
    let Statement::Query(query) = statement else {
        return;
    };

    let original = query.clone();
    *query.body = SetExpr::Select(Box::new(Select {
        select_token: AttachedToken::empty(),
        distinct: None,
        top: None,
        top_before_distinct: false,
        projection: vec![SelectItem::Wildcard(WildcardAdditionalOptions::default())],
        into: None,
        from: vec![TableWithJoins {
            relation: TableFactor::Derived {
                lateral: false,
                subquery: original,
                alias: Some(TableAlias {
                    name: Ident::new(alias),
                    columns: vec![],
                }),
            },
            joins: vec![],
        }],
        lateral_views: vec![],
        prewhere: None,
        selection: None,
        group_by: GroupByExpr::Expressions(vec![], vec![]),
        cluster_by: vec![],
        distribute_by: vec![],
        sort_by: vec![],
        having: None,
        named_window: vec![],
        qualify: None,
        window_before_qualify: false,
        value_table_mode: None,
        connect_by: None,
    }));
}

pub fn format_with_comments(sql: &str, comments: &[String]) -> String {
    if comments.is_empty() {
        return sql.to_string();
    }
    format!("{sql} /* {} */", comments.join("; "))
}

fn rewrite_insert(
    insert: &mut Insert,
    from_table: &QualifiedTableName,
    to_table: &QualifiedTableName,
) {
    if let TableObject::TableName(name) = &mut insert.table
        && from_table.matches_object_name(name)
    {
        *name = to_table.to_object_name();
    }
    if let Some(source) = &mut insert.source {
        rewrite_query(source, from_table, to_table);
    }
}

fn rewrite_delete(
    delete: &mut Delete,
    from_table: &QualifiedTableName,
    to_table: &QualifiedTableName,
) {
    rewrite_from_table(&mut delete.from, from_table, to_table);
    if let Some(using) = &mut delete.using {
        for table in using {
            rewrite_table_with_joins(table, from_table, to_table);
        }
    }
}

fn rewrite_from_table(
    from: &mut FromTable,
    logic_table: &QualifiedTableName,
    actual_table: &QualifiedTableName,
) {
    match from {
        FromTable::WithFromKeyword(tables) | FromTable::WithoutKeyword(tables) => {
            for table in tables {
                rewrite_table_with_joins(table, logic_table, actual_table);
            }
        }
    }
}

fn rewrite_query(
    query: &mut Query,
    from_table: &QualifiedTableName,
    to_table: &QualifiedTableName,
) {
    if let Some(with) = &mut query.with {
        for cte in &mut with.cte_tables {
            rewrite_query(&mut cte.query, from_table, to_table);
        }
    }
    rewrite_set_expr(&mut query.body, from_table, to_table);
}

fn rewrite_set_expr(
    body: &mut SetExpr,
    from_table: &QualifiedTableName,
    to_table: &QualifiedTableName,
) {
    match body {
        SetExpr::Select(select) => {
            for table in &mut select.from {
                rewrite_table_with_joins(table, from_table, to_table);
            }
        }
        SetExpr::Query(query) => rewrite_query(query, from_table, to_table),
        SetExpr::SetOperation { left, right, .. } => {
            rewrite_set_expr(left, from_table, to_table);
            rewrite_set_expr(right, from_table, to_table);
        }
        _ => {}
    }
}

fn rewrite_table_with_joins(
    table: &mut TableWithJoins,
    from_table: &QualifiedTableName,
    to_table: &QualifiedTableName,
) {
    rewrite_table_factor(&mut table.relation, from_table, to_table);
    for join in &mut table.joins {
        rewrite_table_factor(&mut join.relation, from_table, to_table);
    }
}

fn rewrite_table_factor(
    factor: &mut TableFactor,
    from_table: &QualifiedTableName,
    to_table: &QualifiedTableName,
) {
    match factor {
        TableFactor::Table { name, .. } => {
            if from_table.matches_object_name(name) {
                *name = to_table.to_object_name();
            }
        }
        TableFactor::Derived { subquery, .. } => rewrite_query(subquery, from_table, to_table),
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => rewrite_table_with_joins(table_with_joins, from_table, to_table),
        TableFactor::Pivot { table, .. } | TableFactor::Unpivot { table, .. } => {
            rewrite_table_factor(table, from_table, to_table)
        }
        _ => {}
    }
}

fn column_expr(column: &str) -> Expr {
    let parts = column.split('.').collect::<Vec<_>>();
    assert!(
        !parts.is_empty()
            && parts.iter().all(|value| !value.is_empty())
            && parts.iter().any(|value| !value.trim().is_empty()),
        "column name must not be empty"
    );
    let mut idents = parts.into_iter().map(Ident::new).collect::<Vec<_>>();
    match idents.len() {
        1 => Expr::Identifier(idents.remove(0)),
        _ => Expr::CompoundIdentifier(idents),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlparser::dialect::PostgreSqlDialect;
    use sqlparser::parser::Parser;

    fn parse(sql: &str) -> Statement {
        Parser::parse_sql(&PostgreSqlDialect {}, sql)
            .expect("sql parse")
            .into_iter()
            .next()
            .expect("statement")
    }

    #[test]
    fn append_where_to_select_with_existing_where() {
        let mut stmt = parse("SELECT * FROM users WHERE status = 1");
        append_where(&mut stmt, build_eq_int_expr("create_by", 42));
        assert!(stmt.to_string().contains("status = 1 AND create_by = 42"));
    }

    #[test]
    fn append_where_recurses_into_nested_set_operations() {
        let mut stmt = parse("SELECT * FROM a UNION SELECT * FROM b UNION SELECT * FROM c");
        append_where(&mut stmt, build_eq_int_expr("tenant_id", 1));

        let sql = stmt.to_string();
        assert_eq!(sql.matches("tenant_id = 1").count(), 3, "sql={sql}");
    }

    #[test]
    fn replace_table_in_select() {
        let mut stmt = parse("SELECT * FROM biz.order");
        replace_table(&mut stmt, "biz.order", "biz.order_archive").expect("replace table");
        assert!(stmt.to_string().contains("biz.order_archive"));
    }

    #[test]
    fn format_with_comments_multiple() {
        let sql = format_with_comments(
            "SELECT * FROM users",
            &["user_id=42".to_string(), "trace_id=abc".to_string()],
        );
        assert_eq!(sql, "SELECT * FROM users /* user_id=42; trace_id=abc */");
    }

    #[test]
    #[should_panic(expected = "column name must not be empty")]
    fn build_eq_expr_rejects_empty_column_name() {
        let _ = build_eq_expr("", "42");
    }
}
