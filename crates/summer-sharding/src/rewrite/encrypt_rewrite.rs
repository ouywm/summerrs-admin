use sea_orm::{Statement, Value};
use sqlparser::ast::{
    Assignment, AssignmentTarget, BinaryOperator, Expr, FunctionArg, FunctionArgExpr,
    FunctionArguments, Ident, ObjectName, Query, SelectItem, SetExpr, Statement as AstStatement,
    Value as SqlValue,
};

use crate::{
    config::{EncryptRuleConfig, ShardingConfig},
    connector::statement::StatementContext,
    encrypt::{AesGcmEncryptor, DigestAlgorithm, EncryptAlgorithm},
    error::Result,
};

pub fn apply_encrypt_rewrite(
    statement: &mut Statement,
    ast: &mut AstStatement,
    analysis: &StatementContext,
    config: &ShardingConfig,
) -> Result<()> {
    if !config.encrypt.enabled {
        return Ok(());
    }

    let Some(primary_table) = analysis.primary_table() else {
        return Ok(());
    };
    let table_name = primary_table.full_name();
    let rules = config
        .encrypt
        .rules
        .iter()
        .filter(|rule| {
            rule.table.eq_ignore_ascii_case(table_name.as_str())
                || rule
                    .table
                    .eq_ignore_ascii_case(primary_table.table.as_str())
        })
        .collect::<Vec<_>>();
    if rules.is_empty() {
        return Ok(());
    }

    match ast {
        AstStatement::Insert(insert) => {
            rewrite_insert(statement, insert, table_name.as_str(), &rules)?
        }
        AstStatement::Query(query) => rewrite_query(statement, query, &rules)?,
        AstStatement::Update {
            assignments,
            selection,
            ..
        } => rewrite_update(statement, assignments, selection.as_mut(), &rules)?,
        AstStatement::Delete(delete) => {
            if let Some(selection) = &mut delete.selection {
                rewrite_filter_expr(selection, &mut statement.values, &rules)?;
            }
        }
        _ => {}
    }

    Ok(())
}

fn rewrite_insert(
    statement: &mut Statement,
    insert: &mut sqlparser::ast::Insert,
    table_name: &str,
    rules: &[&EncryptRuleConfig],
) -> Result<()> {
    let Some(source) = &mut insert.source else {
        return Ok(());
    };
    let SetExpr::Values(values) = source.body.as_mut() else {
        return Ok(());
    };

    let mut extra_columns = Vec::new();
    let mut extra_values = Vec::new();
    for (column_index, column) in insert.columns.iter_mut().enumerate() {
        let Some(rule) = lookup_rule_in_rules(rules, table_name, column.value.as_str()) else {
            continue;
        };
        let encryptor = AesGcmEncryptor::from_env(rule.key_env.as_str())?;
        let plaintexts = values
            .rows
            .iter()
            .map(|row| plaintext_for_expr(row.get(column_index), statement.values.as_ref()))
            .collect::<Vec<_>>();
        column.value = rule.cipher_column.clone();
        if let Some(sql_values) = &mut statement.values {
            for row in &values.rows {
                if let Some(Expr::Value(SqlValue::Placeholder(placeholder))) = row.get(column_index)
                {
                    rewrite_placeholder_value(placeholder, sql_values, |plain| {
                        encryptor.encrypt(plain.as_str())
                    })?;
                }
            }
        }
        for row in &mut values.rows {
            if let Some(expr) = row.get_mut(column_index) {
                rewrite_expr_value(expr, &encryptor)?;
            }
        }
        if let Some(assisted_query_column) = &rule.assisted_query_column {
            extra_columns.push(Ident::new(assisted_query_column));
            extra_values.push(
                plaintexts
                    .into_iter()
                    .map(|plain| {
                        Expr::Value(SqlValue::SingleQuotedString(
                            plain
                                .map(|value| DigestAlgorithm::Sha256.digest(value.as_str()))
                                .unwrap_or_default(),
                        ))
                    })
                    .collect::<Vec<_>>(),
            );
        }
    }

    insert.columns.extend(extra_columns);
    for (row, extras) in values.rows.iter_mut().zip(extra_values.into_iter()) {
        row.extend(extras);
    }
    Ok(())
}

fn rewrite_query(
    statement: &mut Statement,
    query: &mut Query,
    rules: &[&EncryptRuleConfig],
) -> Result<()> {
    if let Some(with) = &mut query.with {
        for cte in &mut with.cte_tables {
            rewrite_query(statement, &mut cte.query, rules)?;
        }
    }

    match query.body.as_mut() {
        SetExpr::Select(select) => {
            for item in &mut select.projection {
                rewrite_select_item(item, rules);
            }
            if let Some(selection) = &mut select.selection {
                rewrite_filter_expr(selection, &mut statement.values, rules)?;
            }
            if let sqlparser::ast::GroupByExpr::Expressions(expressions, _) = &mut select.group_by {
                for expression in expressions {
                    rewrite_plain_expr(expression, rules);
                }
            }
            if let Some(having) = &mut select.having {
                rewrite_filter_expr(having, &mut statement.values, rules)?;
            }
        }
        SetExpr::Query(query) => rewrite_query(statement, query, rules)?,
        SetExpr::SetOperation { left, right, .. } => {
            rewrite_set_expr(statement, left, rules)?;
            rewrite_set_expr(statement, right, rules)?;
        }
        _ => {}
    }
    Ok(())
}

fn rewrite_set_expr(
    statement: &mut Statement,
    set_expr: &mut SetExpr,
    rules: &[&EncryptRuleConfig],
) -> Result<()> {
    match set_expr {
        SetExpr::Select(select) => {
            for item in &mut select.projection {
                rewrite_select_item(item, rules);
            }
            if let Some(selection) = &mut select.selection {
                rewrite_filter_expr(selection, &mut statement.values, rules)?;
            }
        }
        SetExpr::Query(query) => rewrite_query(statement, query, rules)?,
        SetExpr::SetOperation { left, right, .. } => {
            rewrite_set_expr(statement, left, rules)?;
            rewrite_set_expr(statement, right, rules)?;
        }
        _ => {}
    }
    Ok(())
}

fn rewrite_update(
    statement: &mut Statement,
    assignments: &mut Vec<Assignment>,
    selection: Option<&mut Expr>,
    rules: &[&EncryptRuleConfig],
) -> Result<()> {
    let mut assisted_assignments = Vec::new();
    for assignment in assignments.iter_mut() {
        let Some(rule) = assignment_rule(assignment, rules) else {
            continue;
        };
        let encryptor = AesGcmEncryptor::from_env(rule.key_env.as_str())?;
        let plaintext = plaintext_for_expr(Some(&assignment.value), statement.values.as_ref());
        if let AssignmentTarget::ColumnName(target) = &mut assignment.target {
            *target = rewrite_column_object_name(target.clone(), rule.cipher_column.as_str());
        }
        if let Some(sql_values) = &mut statement.values
            && let Expr::Value(SqlValue::Placeholder(placeholder)) = &assignment.value
        {
            rewrite_placeholder_value(placeholder, sql_values, |plain| {
                encryptor.encrypt(plain.as_str())
            })?;
        }
        rewrite_expr_value(&mut assignment.value, &encryptor)?;

        if let (Some(assisted_column), Some(plaintext)) =
            (rule.assisted_query_column.as_ref(), plaintext)
        {
            assisted_assignments.push(Assignment {
                target: AssignmentTarget::ColumnName(ObjectName(vec![Ident::new(assisted_column)])),
                value: Expr::Value(SqlValue::SingleQuotedString(
                    DigestAlgorithm::Sha256.digest(plaintext.as_str()),
                )),
            });
        }
    }
    assignments.extend(assisted_assignments);
    if let Some(selection) = selection {
        rewrite_filter_expr(selection, &mut statement.values, rules)?;
    }
    Ok(())
}

fn rewrite_select_item(item: &mut SelectItem, rules: &[&EncryptRuleConfig]) {
    match item {
        SelectItem::UnnamedExpr(expr) => {
            if let Some((alias, expr)) = rewrite_plain_projection(expr, rules) {
                *item = SelectItem::ExprWithAlias {
                    expr,
                    alias: Ident::new(alias),
                };
            } else {
                rewrite_plain_expr(expr, rules);
            }
        }
        SelectItem::ExprWithAlias { expr, .. } => rewrite_plain_expr(expr, rules),
        SelectItem::Wildcard(_) | SelectItem::QualifiedWildcard(_, _) => {}
    }
}

fn rewrite_plain_projection(expr: &Expr, rules: &[&EncryptRuleConfig]) -> Option<(String, Expr)> {
    let rule = lookup_rule_for_expr(expr, rules)?;
    let alias = projection_alias(expr)?;
    Some((
        alias,
        rewrite_column_expr(expr.clone(), rule.cipher_column.as_str()),
    ))
}

fn rewrite_filter_expr(
    expr: &mut Expr,
    values: &mut Option<sea_orm::Values>,
    rules: &[&EncryptRuleConfig],
) -> Result<()> {
    match expr {
        Expr::BinaryOp { left, op, right } => {
            if rewrite_comparison(left, op, right, values, rules)? {
                return Ok(());
            }
            rewrite_filter_expr(left, values, rules)?;
            rewrite_filter_expr(right, values, rules)?;
        }
        Expr::Nested(inner) => rewrite_filter_expr(inner, values, rules)?,
        Expr::Between {
            expr, low, high, ..
        } => {
            rewrite_plain_expr(expr, rules);
            rewrite_filter_expr(low, values, rules)?;
            rewrite_filter_expr(high, values, rules)?;
        }
        Expr::InList { expr, list, .. } => {
            rewrite_plain_expr(expr, rules);
            for item in list {
                rewrite_filter_expr(item, values, rules)?;
            }
        }
        Expr::UnaryOp { expr, .. } => rewrite_filter_expr(expr, values, rules)?,
        Expr::Cast { expr, .. } => rewrite_filter_expr(expr, values, rules)?,
        Expr::Function(function) => rewrite_function(function, rules),
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            if let Some(operand) = operand {
                rewrite_filter_expr(operand, values, rules)?;
            }
            for condition in conditions {
                rewrite_filter_expr(condition, values, rules)?;
            }
            if let Some(else_result) = else_result {
                rewrite_filter_expr(else_result, values, rules)?;
            }
        }
        _ => rewrite_plain_expr(expr, rules),
    }
    Ok(())
}

fn rewrite_comparison(
    left: &mut Expr,
    op: &BinaryOperator,
    right: &mut Expr,
    values: &mut Option<sea_orm::Values>,
    rules: &[&EncryptRuleConfig],
) -> Result<bool> {
    if let Some(rule) = lookup_rule_for_expr(left, rules) {
        rewrite_comparison_side(left, right, op, values, rule)?;
        return Ok(true);
    }
    if let Some(rule) = lookup_rule_for_expr(right, rules) {
        rewrite_comparison_side(right, left, op, values, rule)?;
        return Ok(true);
    }
    Ok(false)
}

fn rewrite_comparison_side(
    column_expr: &mut Expr,
    value_expr: &mut Expr,
    op: &BinaryOperator,
    values: &mut Option<sea_orm::Values>,
    rule: &EncryptRuleConfig,
) -> Result<()> {
    if *op == BinaryOperator::Eq
        && let Some(assisted_query_column) = &rule.assisted_query_column
    {
        rewrite_expr_for_digest(value_expr, values, rule)?;
        *column_expr = rewrite_column_expr(column_expr.clone(), assisted_query_column.as_str());
        return Ok(());
    }

    // AES-GCM uses a random nonce, so encrypting the same plaintext twice
    // produces different ciphertexts.  Equality comparisons (`=`) against a
    // non-deterministic cipher will NEVER match.  Require an
    // `assisted_query_column` (deterministic digest) for equality queries.
    if *op == BinaryOperator::Eq {
        return Err(crate::error::ShardingError::Rewrite(format!(
            "encrypt rule for column `{}` uses non-deterministic AES-GCM encryption; \
             equality queries require `assisted_query_column` to be configured",
            rule.column
        )));
    }

    let encryptor = AesGcmEncryptor::from_env(rule.key_env.as_str())?;
    if let Some(sql_values) = values.as_mut()
        && let Expr::Value(SqlValue::Placeholder(placeholder)) = value_expr
    {
        rewrite_placeholder_value(placeholder, sql_values, |plain| {
            encryptor.encrypt(plain.as_str())
        })?;
    }
    rewrite_expr_value(value_expr, &encryptor)?;
    *column_expr = rewrite_column_expr(column_expr.clone(), rule.cipher_column.as_str());
    Ok(())
}

fn rewrite_expr_for_digest(
    expr: &mut Expr,
    values: &mut Option<sea_orm::Values>,
    rule: &EncryptRuleConfig,
) -> Result<()> {
    if let Some(sql_values) = values.as_mut()
        && let Expr::Value(SqlValue::Placeholder(placeholder)) = expr
    {
        rewrite_placeholder_value(placeholder, sql_values, |plain| {
            Ok(DigestAlgorithm::Sha256.digest(plain.as_str()))
        })?;
    }
    if let Some(plain) = plaintext_for_expr(Some(expr), None) {
        *expr = Expr::Value(SqlValue::SingleQuotedString(
            DigestAlgorithm::Sha256.digest(plain.as_str()),
        ));
    } else if let Some(value) = extract_expr_plaintext(Some(expr)) {
        *expr = Expr::Value(SqlValue::SingleQuotedString(
            DigestAlgorithm::Sha256.digest(value.as_str()),
        ));
    }
    let _ = rule;
    Ok(())
}

fn rewrite_plain_expr(expr: &mut Expr, rules: &[&EncryptRuleConfig]) {
    if let Some(rule) = lookup_rule_for_expr(expr, rules) {
        *expr = rewrite_column_expr(expr.clone(), rule.cipher_column.as_str());
        return;
    }

    match expr {
        Expr::BinaryOp { left, right, .. } => {
            rewrite_plain_expr(left, rules);
            rewrite_plain_expr(right, rules);
        }
        Expr::UnaryOp { expr, .. }
        | Expr::Nested(expr)
        | Expr::Cast { expr, .. }
        | Expr::IsNull(expr)
        | Expr::IsNotNull(expr) => rewrite_plain_expr(expr, rules),
        Expr::Between {
            expr, low, high, ..
        } => {
            rewrite_plain_expr(expr, rules);
            rewrite_plain_expr(low, rules);
            rewrite_plain_expr(high, rules);
        }
        Expr::InList { expr, list, .. } => {
            rewrite_plain_expr(expr, rules);
            for item in list {
                rewrite_plain_expr(item, rules);
            }
        }
        Expr::Function(function) => rewrite_function(function, rules),
        Expr::Case {
            operand,
            conditions,
            results,
            else_result,
            ..
        } => {
            if let Some(operand) = operand {
                rewrite_plain_expr(operand, rules);
            }
            for condition in conditions {
                rewrite_plain_expr(condition, rules);
            }
            for result in results {
                rewrite_plain_expr(result, rules);
            }
            if let Some(else_result) = else_result {
                rewrite_plain_expr(else_result, rules);
            }
        }
        _ => {}
    }
}

fn rewrite_function(function: &mut sqlparser::ast::Function, rules: &[&EncryptRuleConfig]) {
    if let FunctionArguments::List(arguments) = &mut function.args {
        for arg in &mut arguments.args {
            match arg {
                FunctionArg::Unnamed(FunctionArgExpr::Expr(expr))
                | FunctionArg::Named {
                    arg: FunctionArgExpr::Expr(expr),
                    ..
                }
                | FunctionArg::ExprNamed {
                    arg: FunctionArgExpr::Expr(expr),
                    ..
                } => rewrite_plain_expr(expr, rules),
                _ => {}
            }
        }
    }
}

fn rewrite_expr_value(expr: &mut Expr, encryptor: &dyn EncryptAlgorithm) -> Result<()> {
    match expr {
        Expr::Value(SqlValue::SingleQuotedString(value))
        | Expr::Value(SqlValue::DoubleQuotedString(value))
        | Expr::Value(SqlValue::EscapedStringLiteral(value))
        | Expr::Value(SqlValue::UnicodeStringLiteral(value))
        | Expr::Value(SqlValue::NationalStringLiteral(value)) => {
            *value = encryptor.encrypt(value.as_str())?;
        }
        Expr::Cast { expr, .. } | Expr::Nested(expr) | Expr::UnaryOp { expr, .. } => {
            rewrite_expr_value(expr, encryptor)?;
        }
        _ => {}
    }
    Ok(())
}

fn extract_expr_plaintext(expr: Option<&Expr>) -> Option<String> {
    match expr {
        Some(Expr::Value(SqlValue::SingleQuotedString(value)))
        | Some(Expr::Value(SqlValue::DoubleQuotedString(value)))
        | Some(Expr::Value(SqlValue::EscapedStringLiteral(value)))
        | Some(Expr::Value(SqlValue::UnicodeStringLiteral(value)))
        | Some(Expr::Value(SqlValue::NationalStringLiteral(value))) => Some(value.clone()),
        _ => None,
    }
}

fn plaintext_for_expr(expr: Option<&Expr>, values: Option<&sea_orm::Values>) -> Option<String> {
    extract_expr_plaintext(expr).or_else(|| match expr {
        Some(Expr::Value(SqlValue::Placeholder(placeholder))) => {
            placeholder_value(placeholder, values)
        }
        _ => None,
    })
}

fn placeholder_value(name: &str, values: Option<&sea_orm::Values>) -> Option<String> {
    let values = values?;
    let index = name
        .strip_prefix('$')
        .and_then(|value| value.parse::<usize>().ok())
        .and_then(|value| value.checked_sub(1))?;
    values.0.get(index).and_then(value_to_plaintext)
}

fn value_to_plaintext(value: &Value) -> Option<String> {
    match value {
        Value::String(Some(value)) => Some(value.clone()),
        Value::Char(Some(value)) => Some(value.to_string()),
        Value::BigInt(Some(value)) => Some(value.to_string()),
        Value::Int(Some(value)) => Some(value.to_string()),
        Value::SmallInt(Some(value)) => Some(value.to_string()),
        Value::TinyInt(Some(value)) => Some(value.to_string()),
        Value::BigUnsigned(Some(value)) => Some(value.to_string()),
        Value::Unsigned(Some(value)) => Some(value.to_string()),
        Value::SmallUnsigned(Some(value)) => Some(value.to_string()),
        Value::TinyUnsigned(Some(value)) => Some(value.to_string()),
        Value::Double(Some(value)) => Some(value.to_string()),
        Value::Float(Some(value)) => Some(value.to_string()),
        _ => None,
    }
}

fn rewrite_placeholder_value(
    placeholder: &str,
    values: &mut sea_orm::Values,
    mapper: impl FnOnce(String) -> Result<String>,
) -> Result<()> {
    let Some(index) = placeholder
        .strip_prefix('$')
        .and_then(|value| value.parse::<usize>().ok())
        .and_then(|value| value.checked_sub(1))
    else {
        return Ok(());
    };
    let Some(value) = values.0.get_mut(index) else {
        return Ok(());
    };
    let plaintext = value_to_plaintext(value).unwrap_or_default();
    *value = Value::String(Some(mapper(plaintext)?));
    Ok(())
}

fn lookup_rule_in_rules<'a>(
    rules: &'a [&EncryptRuleConfig],
    _table_name: &str,
    column: &str,
) -> Option<&'a EncryptRuleConfig> {
    rules
        .iter()
        .copied()
        .find(|rule| rule.column.eq_ignore_ascii_case(column))
}

fn lookup_rule_for_expr<'a>(
    expr: &Expr,
    rules: &'a [&EncryptRuleConfig],
) -> Option<&'a EncryptRuleConfig> {
    let column = projection_alias(expr)?;
    lookup_rule_in_rules(rules, "", column.as_str())
}

fn assignment_rule<'a>(
    assignment: &Assignment,
    rules: &'a [&EncryptRuleConfig],
) -> Option<&'a EncryptRuleConfig> {
    match &assignment.target {
        AssignmentTarget::ColumnName(column) => {
            let name = column.0.last()?.value.clone();
            lookup_rule_in_rules(rules, "", name.as_str())
        }
        AssignmentTarget::Tuple(_) => None,
    }
}

fn rewrite_column_expr(expr: Expr, target_column: &str) -> Expr {
    match expr {
        Expr::Identifier(_) => Expr::Identifier(Ident::new(target_column)),
        Expr::CompoundIdentifier(mut parts) => {
            if let Some(last) = parts.last_mut() {
                *last = Ident::new(target_column);
            }
            Expr::CompoundIdentifier(parts)
        }
        other => other,
    }
}

fn rewrite_column_object_name(mut name: ObjectName, target_column: &str) -> ObjectName {
    if let Some(last) = name.0.last_mut() {
        *last = Ident::new(target_column);
    }
    name
}

fn projection_alias(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Identifier(identifier) => Some(identifier.value.clone()),
        Expr::CompoundIdentifier(parts) => parts.last().map(|ident| ident.value.clone()),
        _ => None,
    }
}
