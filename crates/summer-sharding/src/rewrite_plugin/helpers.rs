//! SQL 操作便捷函数。
//!
//! 为不需要直接操作 `sqlparser` AST 的使用者提供高层 API。
//! 所有函数直接操作 `sqlparser::ast` 类型。

use sqlparser::ast::*;
use sqlparser::ast::helpers::attached_token::AttachedToken;

use crate::router::QualifiedTableName;

// ---------------------------------------------------------------------------
// WHERE 条件追加
// ---------------------------------------------------------------------------

/// 在 SELECT / UPDATE / DELETE 语句的 WHERE 子句后追加 AND 条件。
/// 如果原 SQL 没有 WHERE，则创建 WHERE 子句。
///
/// # 示例
///
/// ```rust,ignore
/// let condition = helpers::build_eq_expr("create_by", "123");
/// helpers::append_where(&mut statement, condition);
/// // SELECT * FROM t → SELECT * FROM t WHERE create_by = '123'
/// // SELECT * FROM t WHERE x = 1 → SELECT * FROM t WHERE (x = 1) AND create_by = '123'
/// ```
pub fn append_where(statement: &mut Statement, condition: Expr) {
    match statement {
        Statement::Query(query) => append_where_to_query(query, condition),
        Statement::Update { selection, .. } => {
            inject_condition(selection, condition);
        }
        Statement::Delete(delete) => {
            inject_condition(&mut delete.selection, condition);
        }
        _ => {}
    }
}

fn append_where_to_query(query: &mut Query, condition: Expr) {
    match query.body.as_mut() {
        SetExpr::Select(select) => {
            inject_condition(&mut select.selection, condition);
        }
        SetExpr::Query(inner) => append_where_to_query(inner, condition),
        SetExpr::SetOperation { left, right, .. } => {
            // 对 UNION 的两侧都注入条件
            if let SetExpr::Select(select) = left.as_mut() {
                inject_condition(&mut select.selection, condition.clone());
            }
            if let SetExpr::Select(select) = right.as_mut() {
                inject_condition(&mut select.selection, condition);
            }
        }
        _ => {}
    }
}

fn inject_condition(selection: &mut Option<Expr>, condition: Expr) {
    match selection.take() {
        Some(existing) => {
            *selection = Some(and(existing, condition));
        }
        None => {
            *selection = Some(condition);
        }
    }
}

// ---------------------------------------------------------------------------
// 表达式构建
// ---------------------------------------------------------------------------

/// 构建 `column = 'value'` 表达式（字符串值）
pub fn build_eq_expr(column: &str, value: &str) -> Expr {
    Expr::BinaryOp {
        left: Box::new(column_expr(column)),
        op: BinaryOperator::Eq,
        right: Box::new(Expr::Value(Value::SingleQuotedString(value.to_string()))),
    }
}

/// 构建 `column = number` 表达式（整数值）
pub fn build_eq_int_expr(column: &str, value: i64) -> Expr {
    Expr::BinaryOp {
        left: Box::new(column_expr(column)),
        op: BinaryOperator::Eq,
        right: Box::new(Expr::Value(Value::Number(value.to_string(), false))),
    }
}

/// 构建 `column IN ('v1', 'v2', ...)` 表达式（字符串列表）
pub fn build_in_expr(column: &str, values: &[&str]) -> Expr {
    Expr::InList {
        expr: Box::new(column_expr(column)),
        list: values
            .iter()
            .map(|v| Expr::Value(Value::SingleQuotedString(v.to_string())))
            .collect(),
        negated: false,
    }
}

/// 构建 `column IN (1, 2, ...)` 表达式（整数列表）
pub fn build_in_int_expr(column: &str, values: &[i64]) -> Expr {
    Expr::InList {
        expr: Box::new(column_expr(column)),
        list: values
            .iter()
            .map(|v| Expr::Value(Value::Number(v.to_string(), false)))
            .collect(),
        negated: false,
    }
}

/// 构建 `column NOT IN ('v1', 'v2', ...)` 表达式（字符串列表）
pub fn build_not_in_expr(column: &str, values: &[&str]) -> Expr {
    Expr::InList {
        expr: Box::new(column_expr(column)),
        list: values
            .iter()
            .map(|v| Expr::Value(Value::SingleQuotedString(v.to_string())))
            .collect(),
        negated: true,
    }
}

/// 构建 `column IS NULL` 表达式
pub fn build_is_null_expr(column: &str) -> Expr {
    Expr::IsNull(Box::new(column_expr(column)))
}

/// 构建 `column IS NOT NULL` 表达式
pub fn build_is_not_null_expr(column: &str) -> Expr {
    Expr::IsNotNull(Box::new(column_expr(column)))
}

/// 构建 `column BETWEEN 'low' AND 'high'` 表达式
pub fn build_between_expr(column: &str, low: &str, high: &str) -> Expr {
    Expr::Between {
        expr: Box::new(column_expr(column)),
        negated: false,
        low: Box::new(Expr::Value(Value::SingleQuotedString(low.to_string()))),
        high: Box::new(Expr::Value(Value::SingleQuotedString(high.to_string()))),
    }
}

/// 构建 `column LIKE 'pattern'` 表达式
pub fn build_like_expr(column: &str, pattern: &str) -> Expr {
    Expr::Like {
        negated: false,
        any: false,
        expr: Box::new(column_expr(column)),
        pattern: Box::new(Expr::Value(Value::SingleQuotedString(
            pattern.to_string(),
        ))),
        escape_char: None,
    }
}

/// 构建 `EXISTS (subquery)` 表达式
pub fn build_exists_expr(subquery: Query) -> Expr {
    Expr::Exists {
        subquery: Box::new(subquery),
        negated: false,
    }
}

// ---------------------------------------------------------------------------
// 逻辑运算
// ---------------------------------------------------------------------------

/// 两个 `Expr` 用 `AND` 连接
pub fn and(left: Expr, right: Expr) -> Expr {
    Expr::BinaryOp {
        left: Box::new(left),
        op: BinaryOperator::And,
        right: Box::new(right),
    }
}

/// 两个 `Expr` 用 `OR` 连接（自动加括号）
pub fn or(left: Expr, right: Expr) -> Expr {
    Expr::Nested(Box::new(Expr::BinaryOp {
        left: Box::new(left),
        op: BinaryOperator::Or,
        right: Box::new(right),
    }))
}

// ---------------------------------------------------------------------------
// 表名替换
// ---------------------------------------------------------------------------

/// 替换 FROM 子句中的表名。
/// 将所有出现的 `from_table` 替换为 `to_table`。
///
/// # 示例
///
/// ```rust,ignore
/// helpers::replace_table(&mut statement, "orders", "orders_archive");
/// ```
pub fn replace_table(statement: &mut Statement, from_table: &str, to_table: &str) {
    let logic = QualifiedTableName::parse(from_table);
    let actual = QualifiedTableName::parse(to_table);
    crate::rewrite::rewrite_table_names(statement, &logic, &actual);
}

// ---------------------------------------------------------------------------
// SELECT 列操作
// ---------------------------------------------------------------------------

/// 从 SELECT 投影列表中移除指定列。
/// 常用于字段级权限控制。
///
/// # 示例
///
/// ```rust,ignore
/// helpers::remove_columns(&mut statement, &["password", "secret_key"]);
/// // SELECT id, name, password FROM users → SELECT id, name FROM users
/// ```
pub fn remove_columns(statement: &mut Statement, columns: &[&str]) {
    if let Statement::Query(query) = statement {
        remove_columns_from_query(query, columns);
    }
}

fn remove_columns_from_query(query: &mut Query, columns: &[&str]) {
    match query.body.as_mut() {
        SetExpr::Select(select) => {
            select.projection.retain(|item| match item {
                SelectItem::UnnamedExpr(Expr::Identifier(ident)) => {
                    !columns.contains(&ident.value.as_str())
                }
                SelectItem::UnnamedExpr(Expr::CompoundIdentifier(idents)) => {
                    // 取最后一个部分作为列名匹配（如 t.password → password）
                    idents
                        .last()
                        .map(|ident| !columns.contains(&ident.value.as_str()))
                        .unwrap_or(true)
                }
                SelectItem::ExprWithAlias { alias, .. } => {
                    !columns.contains(&alias.value.as_str())
                }
                _ => true,
            });
        }
        SetExpr::Query(inner) => remove_columns_from_query(inner, columns),
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// 子查询包装
// ---------------------------------------------------------------------------

/// 将原始 SELECT 语句包裹为子查询。
///
/// `SELECT * FROM t WHERE x = 1` → `SELECT * FROM (SELECT * FROM t WHERE x = 1) AS alias`
///
/// 仅对 `Statement::Query` 生效，其他类型的语句不做修改。
///
/// # 示例
///
/// ```rust,ignore
/// helpers::wrap_subquery(&mut statement, "sub");
/// // SELECT id, name FROM users WHERE age > 18
/// //   → SELECT * FROM (SELECT id, name FROM users WHERE age > 18) AS sub
/// ```
pub fn wrap_subquery(statement: &mut Statement, alias: &str) {
    let Statement::Query(query) = statement else {
        return;
    };

    // 构建一个空壳 SELECT * FROM ...
    let placeholder = Query {
        with: None,
        body: Box::new(SetExpr::Select(Box::new(Select {
            select_token: AttachedToken::empty(),
            distinct: None,
            top: None,
            top_before_distinct: false,
            projection: vec![SelectItem::Wildcard(WildcardAdditionalOptions::default())],
            into: None,
            from: vec![],
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
        }))),
        order_by: None,
        limit: None,
        limit_by: vec![],
        offset: None,
        fetch: None,
        locks: vec![],
        for_clause: None,
        settings: None,
        format_clause: None,
    };

    // 取走原始 Query，替换为空壳
    let original = std::mem::replace(query.as_mut(), placeholder);

    let subquery_table = TableFactor::Derived {
        lateral: false,
        subquery: Box::new(original),
        alias: Some(TableAlias {
            name: Ident::new(alias),
            columns: vec![],
        }),
    };

    // 把子查询塞进空壳的 FROM
    if let SetExpr::Select(select) = query.body.as_mut() {
        select.from = vec![TableWithJoins {
            relation: subquery_table,
            joins: vec![],
        }];
    }
}

// ---------------------------------------------------------------------------
// 注释追加
// ---------------------------------------------------------------------------

/// 向 SQL 字符串添加审计注释。
///
/// 由于 sqlparser AST 不支持任意注释节点，注释以 `/* comment */` 形式
/// 在字符串层面追加。插件在 `rewrite()` 中调用此函数将注释收集到
/// `RewriteContext.comments` 中，`DefaultSqlRewriter` 在 `statement.to_string()`
/// 后自动拼接所有已收集的注释。
///
/// # 示例
///
/// ```rust,ignore
/// // 在插件的 rewrite() 方法中：
/// ctx.append_comment(&format!("user_id={}", user.id));
/// ```
///
/// 最终 SQL: `SELECT * FROM users WHERE id = 1 /* user_id=42 */`
pub fn format_with_comments(sql: &str, comments: &[String]) -> String {
    if comments.is_empty() {
        return sql.to_string();
    }
    let joined = comments.join("; ");
    format!("{sql} /* {joined} */")
}

// ---------------------------------------------------------------------------
// 内部工具
// ---------------------------------------------------------------------------

/// 构建列引用表达式。
/// 如果 `column` 包含 `.`（如 `t.id`），则生成 `CompoundIdentifier`。
fn column_expr(column: &str) -> Expr {
    if let Some((qualifier, col)) = column.split_once('.') {
        Expr::CompoundIdentifier(vec![Ident::new(qualifier), Ident::new(col)])
    } else {
        Expr::Identifier(Ident::new(column))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use sqlparser::{dialect::PostgreSqlDialect, parser::Parser};

    use super::*;

    fn parse(sql: &str) -> Statement {
        let stmts = Parser::parse_sql(&PostgreSqlDialect {}, sql).expect("parse");
        stmts.into_iter().next().unwrap()
    }

    fn to_sql(stmt: &Statement) -> String {
        stmt.to_string()
    }

    #[test]
    fn append_where_to_select_without_existing_where() {
        let mut stmt = parse("SELECT * FROM users");
        let cond = build_eq_expr("create_by", "42");
        append_where(&mut stmt, cond);
        let sql = to_sql(&stmt);
        assert!(sql.contains("WHERE create_by = '42'"), "got: {sql}");
    }

    #[test]
    fn append_where_to_select_with_existing_where() {
        let mut stmt = parse("SELECT * FROM users WHERE active = true");
        let cond = build_eq_int_expr("dept_id", 7);
        append_where(&mut stmt, cond);
        let sql = to_sql(&stmt);
        assert!(sql.contains("AND dept_id = 7"), "got: {sql}");
    }

    #[test]
    fn append_where_to_update() {
        let mut stmt = parse("UPDATE users SET name = 'test'");
        let cond = build_eq_expr("id", "1");
        append_where(&mut stmt, cond);
        let sql = to_sql(&stmt);
        assert!(sql.contains("WHERE id = '1'"), "got: {sql}");
    }

    #[test]
    fn append_where_to_delete() {
        let mut stmt = parse("DELETE FROM users WHERE active = false");
        let cond = build_eq_int_expr("org_id", 10);
        append_where(&mut stmt, cond);
        let sql = to_sql(&stmt);
        assert!(sql.contains("AND org_id = 10"), "got: {sql}");
    }

    #[test]
    fn build_in_expr_strings() {
        let expr = build_in_expr("status", &["active", "pending"]);
        let sql = format!("{expr}");
        assert!(sql.contains("IN"), "got: {sql}");
        assert!(sql.contains("'active'"), "got: {sql}");
        assert!(sql.contains("'pending'"), "got: {sql}");
    }

    #[test]
    fn build_in_int_expr_values() {
        let expr = build_in_int_expr("dept_id", &[1, 2, 3]);
        let sql = format!("{expr}");
        assert!(sql.contains("IN"), "got: {sql}");
    }

    #[test]
    fn build_is_null() {
        let expr = build_is_null_expr("deleted_at");
        let sql = format!("{expr}");
        assert!(sql.contains("IS NULL"), "got: {sql}");
    }

    #[test]
    fn build_between() {
        let expr = build_between_expr("create_time", "2024-01-01", "2024-12-31");
        let sql = format!("{expr}");
        assert!(sql.contains("BETWEEN"), "got: {sql}");
    }

    #[test]
    fn build_like() {
        let expr = build_like_expr("name", "%test%");
        let sql = format!("{expr}");
        assert!(sql.contains("LIKE"), "got: {sql}");
    }

    #[test]
    fn or_wraps_in_nested() {
        let left = build_eq_expr("a", "1");
        let right = build_eq_expr("b", "2");
        let combined = or(left, right);
        let sql = format!("{combined}");
        // OR 应该被括号包裹
        assert!(sql.contains("("), "got: {sql}");
        assert!(sql.contains("OR"), "got: {sql}");
    }

    #[test]
    fn replace_table_in_select() {
        let mut stmt = parse("SELECT * FROM orders WHERE id = 1");
        replace_table(&mut stmt, "orders", "orders_archive");
        let sql = to_sql(&stmt);
        assert!(sql.contains("orders_archive"), "got: {sql}");
        assert!(!sql.contains(" orders "), "got: {sql}");
    }

    #[test]
    fn remove_columns_from_select() {
        let mut stmt = parse("SELECT id, name, password, email FROM users");
        remove_columns(&mut stmt, &["password"]);
        let sql = to_sql(&stmt);
        assert!(!sql.contains("password"), "got: {sql}");
        assert!(sql.contains("id"), "got: {sql}");
        assert!(sql.contains("name"), "got: {sql}");
        assert!(sql.contains("email"), "got: {sql}");
    }

    #[test]
    fn column_expr_with_qualifier() {
        let expr = column_expr("t.id");
        let sql = format!("{expr}");
        assert!(sql.contains("t.id") || sql.contains("t.\"id\""), "got: {sql}");
    }

    #[test]
    fn wrap_subquery_select() {
        let mut stmt = parse("SELECT id, name FROM users WHERE age > 18");
        wrap_subquery(&mut stmt, "sub");
        let sql = to_sql(&stmt);
        // 应该变为 SELECT * FROM (SELECT id, name FROM users WHERE age > 18) AS sub
        assert!(sql.contains("FROM (SELECT"), "got: {sql}");
        assert!(sql.contains("AS sub"), "got: {sql}");
        assert!(sql.contains("WHERE age > 18"), "原始条件应保留在子查询内, got: {sql}");
    }

    #[test]
    fn wrap_subquery_ignores_non_query() {
        let mut stmt = parse("UPDATE users SET name = 'test'");
        let original = to_sql(&stmt);
        wrap_subquery(&mut stmt, "sub");
        assert_eq!(to_sql(&stmt), original, "非 SELECT 语句不应被修改");
    }

    #[test]
    fn format_with_comments_empty() {
        let sql = "SELECT 1";
        let result = format_with_comments(sql, &[]);
        assert_eq!(result, "SELECT 1");
    }

    #[test]
    fn format_with_comments_single() {
        let sql = "SELECT * FROM users";
        let result = format_with_comments(sql, &["user_id=42".to_string()]);
        assert_eq!(result, "SELECT * FROM users /* user_id=42 */");
    }

    #[test]
    fn format_with_comments_multiple() {
        let sql = "SELECT * FROM users";
        let result = format_with_comments(
            sql,
            &["user_id=42".to_string(), "ds=ds_main".to_string()],
        );
        assert_eq!(
            result,
            "SELECT * FROM users /* user_id=42; ds=ds_main */"
        );
    }

    #[test]
    fn compound_condition() {
        let mut stmt = parse("SELECT * FROM orders");
        let cond = and(
            build_eq_int_expr("user_id", 42),
            or(
                build_eq_expr("status", "active"),
                build_eq_expr("status", "pending"),
            ),
        );
        append_where(&mut stmt, cond);
        let sql = to_sql(&stmt);
        assert!(sql.contains("user_id = 42"), "got: {sql}");
        assert!(sql.contains("OR"), "got: {sql}");
    }
}
