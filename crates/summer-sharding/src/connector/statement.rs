use std::collections::BTreeMap;

use chrono::TimeZone;
use sea_orm::{Statement, Value, Values};
use sqlparser::{
    ast::{
        BinaryOperator, Delete, Expr, Function, FunctionArg, FunctionArgExpr, FunctionArguments,
        Insert, Query, SelectItem, SetExpr, Statement as AstStatement, TableFactor, TableObject,
        TableWithJoins, Value as SqlValue,
    },
    dialect::PostgreSqlDialect,
    parser::Parser,
};

use crate::{
    algorithm::{RangeBound, ShardingCondition, ShardingValue, parse_datetime_string},
    connector::ShardingHint,
    error::{Result, ShardingError},
    router::{OrderByItem, QualifiedTableName, SqlOperation},
    tenant::TenantContext,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AggregateFunction {
    Count,
    Sum,
    Avg,
    Min,
    Max,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectionKind {
    Wildcard,
    Column {
        source_column: String,
    },
    Aggregate {
        function: AggregateFunction,
        source_column: Option<String>,
        avg_sum_column: Option<String>,
        avg_count_column: Option<String>,
    },
    Expression {
        sql: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectProjection {
    pub output_column: String,
    pub kind: ProjectionKind,
}

impl SelectProjection {
    pub fn is_aggregate(&self) -> bool {
        matches!(self.kind, ProjectionKind::Aggregate { .. })
    }
}

#[derive(Debug, Clone)]
pub struct StatementContext {
    pub ast: AstStatement,
    pub operation: SqlOperation,
    pub tables: Vec<QualifiedTableName>,
    pub sharding_conditions: BTreeMap<String, ShardingCondition>,
    pub insert_values: BTreeMap<String, Vec<ShardingValue>>,
    pub projections: Vec<SelectProjection>,
    pub group_by: Vec<String>,
    pub order_by: Vec<OrderByItem>,
    pub limit: Option<u64>,
    pub offset: Option<u64>,
    pub hint: Option<ShardingHint>,
    pub tenant: Option<TenantContext>,
}

impl StatementContext {
    pub fn primary_table(&self) -> Option<&QualifiedTableName> {
        self.tables.first()
    }

    pub fn sharding_condition(&self, column: &str) -> Option<&ShardingCondition> {
        self.sharding_conditions.get(&normalize_column(column))
    }

    pub fn insert_values(&self, column: &str) -> &[ShardingValue] {
        self.insert_values
            .get(&normalize_column(column))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn exact_condition_value(&self, column: &str) -> Option<&ShardingValue> {
        match self.sharding_condition(column) {
            Some(ShardingCondition::Exact(value)) => Some(value),
            _ => None,
        }
    }

    pub fn has_sharding_key(&self) -> bool {
        !self.sharding_conditions.is_empty() || !self.insert_values.is_empty()
    }

    pub fn has_aggregate_projection(&self) -> bool {
        self.projections.iter().any(SelectProjection::is_aggregate)
    }

    pub fn is_grouped_query(&self) -> bool {
        !self.group_by.is_empty()
    }
}

pub fn analyze_statement(stmt: &Statement) -> Result<StatementContext> {
    let dialect = PostgreSqlDialect {};
    let mut statements = Parser::parse_sql(&dialect, stmt.sql.as_str())?;
    let ast = statements
        .drain(..)
        .next()
        .ok_or_else(|| ShardingError::Parse("statement is empty".to_string()))?;

    let operation = match &ast {
        AstStatement::Query(_) => SqlOperation::Select,
        AstStatement::Insert(_) => SqlOperation::Insert,
        AstStatement::Update { .. } => SqlOperation::Update,
        AstStatement::Delete(_) => SqlOperation::Delete,
        _ => SqlOperation::Other,
    };

    let mut tables = Vec::new();
    let mut sharding_conditions = BTreeMap::new();
    let mut insert_values = BTreeMap::new();
    let mut projections = Vec::new();
    let mut group_by = Vec::new();
    let mut order_by = Vec::new();
    let mut limit = None;
    let mut offset = None;

    collect_statement_tables(&ast, &mut tables);

    match &ast {
        AstStatement::Query(query) => {
            collect_query_shape(query, &mut projections, &mut group_by);
            collect_query_meta(
                query,
                stmt.values.as_ref(),
                &mut sharding_conditions,
                &mut order_by,
                &mut limit,
                &mut offset,
            );
        }
        AstStatement::Insert(insert) => {
            collect_insert_values(insert, stmt.values.as_ref(), &mut insert_values);
            if let Some(source) = &insert.source {
                collect_query_meta(
                    source,
                    stmt.values.as_ref(),
                    &mut sharding_conditions,
                    &mut order_by,
                    &mut limit,
                    &mut offset,
                );
            }
        }
        AstStatement::Update { selection, .. } => {
            if let Some(selection) = selection {
                collect_expr_conditions(selection, stmt.values.as_ref(), &mut sharding_conditions);
            }
        }
        AstStatement::Delete(delete) => {
            if let Some(selection) = &delete.selection {
                collect_expr_conditions(selection, stmt.values.as_ref(), &mut sharding_conditions);
            }
            if let Some(limit_expr) = &delete.limit {
                limit = expr_to_u64(limit_expr, stmt.values.as_ref());
            }
            order_by.extend(delete.order_by.iter().map(|item| OrderByItem {
                column: column_name(&item.expr),
                asc: item.asc.unwrap_or(true),
            }));
        }
        _ => {}
    }

    Ok(StatementContext {
        ast,
        operation,
        tables,
        sharding_conditions,
        insert_values,
        projections,
        group_by,
        order_by,
        limit,
        offset,
        hint: None,
        tenant: None,
    })
}

fn collect_statement_tables(statement: &AstStatement, tables: &mut Vec<QualifiedTableName>) {
    match statement {
        AstStatement::Query(query) => collect_query_tables(query, tables),
        AstStatement::Insert(insert) => {
            if let TableObject::TableName(name) = &insert.table {
                tables.push(object_name_to_table(name));
            }
            if let Some(source) = &insert.source {
                collect_query_tables(source, tables);
            }
        }
        AstStatement::Update { table, from, .. } => {
            collect_table_with_joins(table, tables);
            if let Some(from) = from {
                match from {
                    sqlparser::ast::UpdateTableFromKind::BeforeSet(table)
                    | sqlparser::ast::UpdateTableFromKind::AfterSet(table) => {
                        collect_table_with_joins(table, tables)
                    }
                }
            }
        }
        AstStatement::Delete(delete) => {
            collect_delete_tables(delete, tables);
        }
        AstStatement::CreateTable(create_table) => {
            tables.push(object_name_to_table(&create_table.name));
            if let Some(query) = &create_table.query {
                collect_query_tables(query, tables);
            }
        }
        AstStatement::AlterTable { name, .. } => {
            tables.push(object_name_to_table(name));
        }
        AstStatement::Truncate { table_names, .. } => {
            for table in table_names {
                tables.push(object_name_to_table(&table.name));
            }
        }
        _ => {}
    }
}

fn collect_delete_tables(delete: &Delete, tables: &mut Vec<QualifiedTableName>) {
    match &delete.from {
        sqlparser::ast::FromTable::WithFromKeyword(items)
        | sqlparser::ast::FromTable::WithoutKeyword(items) => {
            for item in items {
                collect_table_with_joins(item, tables);
            }
        }
    }
    if let Some(using) = &delete.using {
        for item in using {
            collect_table_with_joins(item, tables);
        }
    }
}

fn collect_query_tables(query: &Query, tables: &mut Vec<QualifiedTableName>) {
    if let Some(with) = &query.with {
        for cte in &with.cte_tables {
            collect_query_tables(&cte.query, tables);
        }
    }
    collect_set_expr_tables(query.body.as_ref(), tables);
}

fn collect_set_expr_tables(set_expr: &SetExpr, tables: &mut Vec<QualifiedTableName>) {
    match set_expr {
        SetExpr::Select(select) => {
            for table in &select.from {
                collect_table_with_joins(table, tables);
            }
        }
        SetExpr::Query(query) => collect_query_tables(query, tables),
        SetExpr::SetOperation { left, right, .. } => {
            collect_set_expr_tables(left, tables);
            collect_set_expr_tables(right, tables);
        }
        _ => {}
    }
}

fn collect_table_with_joins(table: &TableWithJoins, tables: &mut Vec<QualifiedTableName>) {
    collect_table_factor(&table.relation, tables);
    for join in &table.joins {
        collect_table_factor(&join.relation, tables);
    }
}

fn collect_table_factor(table: &TableFactor, tables: &mut Vec<QualifiedTableName>) {
    match table {
        TableFactor::Table { name, .. } => tables.push(object_name_to_table(name)),
        TableFactor::Derived { subquery, .. } => collect_query_tables(subquery, tables),
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => collect_table_with_joins(table_with_joins, tables),
        TableFactor::Pivot { table, .. } | TableFactor::Unpivot { table, .. } => {
            collect_table_factor(table, tables)
        }
        _ => {}
    }
}

fn collect_query_shape(
    query: &Query,
    projections: &mut Vec<SelectProjection>,
    group_by: &mut Vec<String>,
) {
    match query.body.as_ref() {
        SetExpr::Select(select) => {
            projections.extend(
                select
                    .projection
                    .iter()
                    .filter_map(select_item_to_projection),
            );
            if let sqlparser::ast::GroupByExpr::Expressions(expressions, _) = &select.group_by {
                group_by.extend(expressions.iter().map(column_name));
            }
        }
        SetExpr::Query(inner) => collect_query_shape(inner, projections, group_by),
        _ => {}
    }
}

fn select_item_to_projection(item: &SelectItem) -> Option<SelectProjection> {
    match item {
        SelectItem::UnnamedExpr(expr) => aggregate_projection(expr, None)
            .or_else(|| {
                extract_column(expr).map(|column| SelectProjection {
                    output_column: column.clone(),
                    kind: ProjectionKind::Column {
                        source_column: column,
                    },
                })
            })
            .or_else(|| {
                Some(SelectProjection {
                    output_column: normalize_column(expr.to_string().as_str()),
                    kind: ProjectionKind::Expression {
                        sql: expr.to_string(),
                    },
                })
            }),
        SelectItem::ExprWithAlias { expr, alias } => {
            let alias = normalize_column(alias.value.as_str());
            aggregate_projection(expr, Some(alias.clone()))
                .or_else(|| {
                    extract_column(expr).map(|column| SelectProjection {
                        output_column: alias.clone(),
                        kind: ProjectionKind::Column {
                            source_column: column,
                        },
                    })
                })
                .or_else(|| {
                    Some(SelectProjection {
                        output_column: alias,
                        kind: ProjectionKind::Expression {
                            sql: expr.to_string(),
                        },
                    })
                })
        }
        SelectItem::Wildcard(_) | SelectItem::QualifiedWildcard(_, _) => Some(SelectProjection {
            output_column: "*".to_string(),
            kind: ProjectionKind::Wildcard,
        }),
    }
}

fn aggregate_projection(expr: &Expr, alias: Option<String>) -> Option<SelectProjection> {
    let Expr::Function(function) = expr else {
        return None;
    };
    let function_name = function
        .name
        .0
        .last()
        .map(|ident| normalize_column(ident.value.as_str()))?;
    let source_column = first_function_arg_column(function);
    let output_column = alias.unwrap_or_else(|| default_projection_name(function_name.as_str()));
    let (function, avg_sum_column, avg_count_column) = match function_name.as_str() {
        "count" => (AggregateFunction::Count, None, None),
        "sum" => (AggregateFunction::Sum, None, None),
        "avg" => (
            AggregateFunction::Avg,
            Some(format!("__summer_avg_sum_{output_column}")),
            Some(format!("__summer_avg_count_{output_column}")),
        ),
        "min" => (AggregateFunction::Min, None, None),
        "max" => (AggregateFunction::Max, None, None),
        _ => return None,
    };
    Some(SelectProjection {
        output_column,
        kind: ProjectionKind::Aggregate {
            function,
            source_column,
            avg_sum_column,
            avg_count_column,
        },
    })
}

fn first_function_arg_column(function: &Function) -> Option<String> {
    let FunctionArguments::List(arguments) = &function.args else {
        return None;
    };
    arguments.args.first().and_then(|arg| match arg {
        FunctionArg::Unnamed(FunctionArgExpr::Expr(expr)) => extract_column(expr),
        FunctionArg::Unnamed(FunctionArgExpr::Wildcard) => None,
        FunctionArg::Unnamed(FunctionArgExpr::QualifiedWildcard(_)) => None,
        FunctionArg::Named { arg, .. } | FunctionArg::ExprNamed { arg, .. } => match arg {
            FunctionArgExpr::Expr(expr) => extract_column(expr),
            FunctionArgExpr::Wildcard | FunctionArgExpr::QualifiedWildcard(_) => None,
        },
    })
}

fn default_projection_name(function_name: &str) -> String {
    normalize_column(function_name)
}

fn collect_query_meta(
    query: &Query,
    values: Option<&Values>,
    conditions: &mut BTreeMap<String, ShardingCondition>,
    order_by: &mut Vec<OrderByItem>,
    limit: &mut Option<u64>,
    offset: &mut Option<u64>,
) {
    if let Some(with) = &query.with {
        for cte in &with.cte_tables {
            collect_query_meta(&cte.query, values, conditions, order_by, limit, offset);
        }
    }

    if let Some(order) = &query.order_by {
        order_by.extend(order.exprs.iter().map(|item| OrderByItem {
            column: column_name(&item.expr),
            asc: item.asc.unwrap_or(true),
        }));
    }
    if let Some(limit_expr) = &query.limit {
        *limit = expr_to_u64(limit_expr, values);
    }
    if let Some(offset_expr) = &query.offset {
        *offset = expr_to_u64(&offset_expr.value, values);
    }

    collect_set_expr_meta(query.body.as_ref(), values, conditions);
}

fn collect_set_expr_meta(
    set_expr: &SetExpr,
    values: Option<&Values>,
    conditions: &mut BTreeMap<String, ShardingCondition>,
) {
    match set_expr {
        SetExpr::Select(select) => {
            if let Some(selection) = &select.selection {
                collect_expr_conditions(selection, values, conditions);
            }
            if let sqlparser::ast::GroupByExpr::Expressions(expressions, _) = &select.group_by {
                for expression in expressions {
                    collect_expr_conditions(expression, values, conditions);
                }
            }
            if let Some(having) = &select.having {
                collect_expr_conditions(having, values, conditions);
            }
        }
        SetExpr::Query(query) => collect_query_meta(
            query,
            values,
            conditions,
            &mut Vec::new(),
            &mut None,
            &mut None,
        ),
        SetExpr::SetOperation { left, right, .. } => {
            collect_set_expr_meta(left, values, conditions);
            collect_set_expr_meta(right, values, conditions);
        }
        _ => {}
    }
}

fn collect_insert_values(
    insert: &Insert,
    values: Option<&Values>,
    insert_values: &mut BTreeMap<String, Vec<ShardingValue>>,
) {
    let Some(source) = &insert.source else {
        return;
    };
    let SetExpr::Values(values_expr) = source.body.as_ref() else {
        return;
    };

    for row in &values_expr.rows {
        for (column, expr) in insert.columns.iter().zip(row.iter()) {
            let Some(value) = expr_to_sharding_value(expr, values) else {
                continue;
            };
            insert_values
                .entry(normalize_column(column.value.as_str()))
                .or_default()
                .push(value);
        }
    }
}

fn collect_expr_conditions(
    expr: &Expr,
    values: Option<&Values>,
    conditions: &mut BTreeMap<String, ShardingCondition>,
) {
    match expr {
        Expr::BinaryOp { left, op, right } if *op == BinaryOperator::And => {
            collect_expr_conditions(left, values, conditions);
            collect_expr_conditions(right, values, conditions);
        }
        Expr::BinaryOp { left, op, right } => {
            let left_column = extract_column(left);
            let right_column = extract_column(right);

            if let Some(column) = left_column {
                apply_binary_condition(column, op, right, values, false, conditions);
            } else if let Some(column) = right_column {
                apply_binary_condition(column, op, left, values, true, conditions);
            }
        }
        Expr::Between {
            expr, low, high, ..
        } => {
            if let Some(column) = extract_column(expr) {
                let lower = expr_to_sharding_value(low, values).map(|value| RangeBound {
                    value,
                    inclusive: true,
                });
                let upper = expr_to_sharding_value(high, values).map(|value| RangeBound {
                    value,
                    inclusive: true,
                });
                merge_condition(
                    conditions,
                    column,
                    ShardingCondition::Range { lower, upper },
                );
            }
        }
        _ => {}
    }
}

fn apply_binary_condition(
    column: String,
    op: &BinaryOperator,
    value_expr: &Expr,
    values: Option<&Values>,
    reversed: bool,
    conditions: &mut BTreeMap<String, ShardingCondition>,
) {
    let Some(value) = expr_to_sharding_value(value_expr, values) else {
        return;
    };
    let condition = match op {
        BinaryOperator::Eq => ShardingCondition::Exact(value),
        BinaryOperator::Gt => ShardingCondition::Range {
            lower: Some(RangeBound {
                value,
                inclusive: reversed,
            }),
            upper: None,
        },
        BinaryOperator::GtEq => ShardingCondition::Range {
            lower: Some(RangeBound {
                value,
                inclusive: !reversed,
            }),
            upper: None,
        },
        BinaryOperator::Lt => ShardingCondition::Range {
            lower: None,
            upper: Some(RangeBound {
                value,
                inclusive: reversed,
            }),
        },
        BinaryOperator::LtEq => ShardingCondition::Range {
            lower: None,
            upper: Some(RangeBound {
                value,
                inclusive: !reversed,
            }),
        },
        _ => return,
    };
    merge_condition(conditions, column, condition);
}

fn merge_condition(
    conditions: &mut BTreeMap<String, ShardingCondition>,
    column: String,
    condition: ShardingCondition,
) {
    match conditions.get_mut(&column) {
        Some(existing @ ShardingCondition::Exact(_)) => {
            *existing = condition;
        }
        Some(ShardingCondition::Range { lower, upper }) => {
            if let ShardingCondition::Range {
                lower: next_lower,
                upper: next_upper,
            } = condition
            {
                if next_lower.is_some() {
                    *lower = next_lower;
                }
                if next_upper.is_some() {
                    *upper = next_upper;
                }
            }
        }
        None => {
            conditions.insert(column, condition);
        }
    }
}

fn expr_to_u64(expr: &Expr, values: Option<&Values>) -> Option<u64> {
    expr_to_sharding_value(expr, values)?
        .as_i64()
        .map(|value| value as u64)
}

fn expr_to_sharding_value(expr: &Expr, values: Option<&Values>) -> Option<ShardingValue> {
    match expr {
        Expr::Value(value) => sql_value_to_sharding_value(value, values),
        Expr::Cast { expr, .. } => expr_to_sharding_value(expr, values),
        Expr::Nested(expr) => expr_to_sharding_value(expr, values),
        Expr::UnaryOp { op, expr } if *op == sqlparser::ast::UnaryOperator::Minus => {
            expr_to_sharding_value(expr, values)?
                .as_i64()
                .map(|value| ShardingValue::Int(-value))
        }
        _ => None,
    }
}

fn sql_value_to_sharding_value(value: &SqlValue, values: Option<&Values>) -> Option<ShardingValue> {
    match value {
        SqlValue::Number(number, _) => number.parse::<i64>().ok().map(ShardingValue::Int),
        SqlValue::SingleQuotedString(text)
        | SqlValue::DoubleQuotedString(text)
        | SqlValue::EscapedStringLiteral(text)
        | SqlValue::UnicodeStringLiteral(text)
        | SqlValue::NationalStringLiteral(text) => parse_datetime_string(text)
            .map(ShardingValue::DateTime)
            .or_else(|| Some(ShardingValue::Str(text.clone()))),
        SqlValue::Boolean(value) => Some(ShardingValue::Int(i64::from(*value))),
        SqlValue::Null => Some(ShardingValue::Null),
        SqlValue::Placeholder(name) => placeholder_value(name.as_str(), values),
        _ => None,
    }
}

fn placeholder_value(name: &str, values: Option<&Values>) -> Option<ShardingValue> {
    let values = values?;
    if let Some(index) = name
        .strip_prefix('$')
        .and_then(|value| value.parse::<usize>().ok())
    {
        return values
            .0
            .get(index.saturating_sub(1))
            .and_then(sea_value_to_sharding_value);
    }
    if name == "?" {
        return values.0.first().and_then(sea_value_to_sharding_value);
    }
    None
}

fn sea_value_to_sharding_value(value: &Value) -> Option<ShardingValue> {
    match value {
        Value::BigInt(value) => value.map(ShardingValue::Int),
        Value::Int(value) => value.map(|value| ShardingValue::Int(value as i64)),
        Value::SmallInt(value) => value.map(|value| ShardingValue::Int(value as i64)),
        Value::TinyInt(value) => value.map(|value| ShardingValue::Int(value as i64)),
        Value::BigUnsigned(value) => value.map(|value| ShardingValue::Int(value as i64)),
        Value::Unsigned(value) => value.map(|value| ShardingValue::Int(value as i64)),
        Value::SmallUnsigned(value) => value.map(|value| ShardingValue::Int(value as i64)),
        Value::TinyUnsigned(value) => value.map(|value| ShardingValue::Int(value as i64)),
        Value::String(value) => value.as_ref().and_then(|value| {
            parse_datetime_string(value)
                .map(ShardingValue::DateTime)
                .or_else(|| Some(ShardingValue::Str(value.clone())))
        }),
        Value::ChronoDateTimeWithTimeZone(value) => value.map(ShardingValue::DateTime),
        Value::ChronoDateTimeUtc(value) => {
            value.map(|value| ShardingValue::DateTime(value.fixed_offset()))
        }
        Value::ChronoDateTimeLocal(value) => {
            value.map(|value| ShardingValue::DateTime(value.fixed_offset()))
        }
        Value::ChronoDateTime(value) => value.as_ref().and_then(|value| {
            chrono::FixedOffset::east_opt(0).and_then(|offset| {
                offset
                    .from_local_datetime(value)
                    .single()
                    .map(ShardingValue::DateTime)
            })
        }),
        Value::ChronoDate(value) => value.and_then(|value| {
            value.and_hms_opt(0, 0, 0).and_then(|datetime| {
                chrono::FixedOffset::east_opt(0).and_then(|offset| {
                    offset
                        .from_local_datetime(&datetime)
                        .single()
                        .map(ShardingValue::DateTime)
                })
            })
        }),
        _ => None,
    }
}

fn object_name_to_table(name: &sqlparser::ast::ObjectName) -> QualifiedTableName {
    match name.0.as_slice() {
        [table] => QualifiedTableName {
            schema: None,
            table: table.value.clone(),
        },
        [schema, table] => QualifiedTableName {
            schema: Some(schema.value.clone()),
            table: table.value.clone(),
        },
        items => QualifiedTableName {
            schema: None,
            table: items
                .last()
                .map(|value| value.value.clone())
                .unwrap_or_default(),
        },
    }
}

fn extract_column(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Identifier(identifier) => Some(normalize_column(identifier.value.as_str())),
        Expr::CompoundIdentifier(parts) => parts
            .last()
            .map(|identifier| normalize_column(identifier.value.as_str())),
        Expr::Nested(expr) => extract_column(expr),
        _ => None,
    }
}

fn column_name(expr: &Expr) -> String {
    extract_column(expr).unwrap_or_else(|| expr.to_string())
}

fn normalize_column(value: &str) -> String {
    value.to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use sea_orm::{DbBackend, Statement};

    use super::analyze_statement;

    #[test]
    fn analyzer_extracts_tables_and_conditions() {
        let statement = Statement::from_sql_and_values(
            DbBackend::Postgres,
            r#"SELECT * FROM ai.log WHERE create_time >= $1 AND create_time < $2 ORDER BY create_time DESC LIMIT 10 OFFSET 20"#,
            [
                chrono::FixedOffset::east_opt(0)
                    .unwrap()
                    .with_ymd_and_hms(2026, 2, 1, 0, 0, 0)
                    .unwrap()
                    .into(),
                chrono::FixedOffset::east_opt(0)
                    .unwrap()
                    .with_ymd_and_hms(2026, 4, 1, 0, 0, 0)
                    .unwrap()
                    .into(),
            ],
        );
        let analysis = analyze_statement(&statement).expect("analysis");

        assert_eq!(analysis.tables[0].full_name(), "ai.log");
        assert!(analysis.sharding_condition("create_time").is_some());
        assert_eq!(analysis.limit, Some(10));
        assert_eq!(analysis.offset, Some(20));
        assert_eq!(analysis.order_by[0].column, "create_time");
    }

    #[test]
    fn analyzer_extracts_alter_table_target() {
        let statement = Statement::from_string(
            DbBackend::Postgres,
            "ALTER TABLE ai.log ADD COLUMN archived_at timestamptz",
        );

        let analysis = analyze_statement(&statement).expect("analysis");

        assert_eq!(analysis.tables.len(), 1);
        assert_eq!(analysis.tables[0].full_name(), "ai.log");
    }

    #[test]
    fn analyzer_extracts_truncate_targets() {
        let statement =
            Statement::from_string(DbBackend::Postgres, "TRUNCATE TABLE ai.log, ai.audit_log");

        let analysis = analyze_statement(&statement).expect("analysis");

        assert_eq!(analysis.tables.len(), 2);
        assert_eq!(analysis.tables[0].full_name(), "ai.log");
        assert_eq!(analysis.tables[1].full_name(), "ai.audit_log");
    }
}
