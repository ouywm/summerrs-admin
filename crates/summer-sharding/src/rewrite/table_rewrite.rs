use sqlparser::ast::{
    Delete, FromTable, Ident, Insert, Query, SetExpr, Statement, TableAlias, TableFactor,
    TableObject, TableWithJoins, UpdateTableFromKind,
};

use crate::router::QualifiedTableName;

pub fn rewrite_table_names(
    statement: &mut Statement,
    logic_table: &QualifiedTableName,
    actual_table: &QualifiedTableName,
) {
    match statement {
        Statement::Query(query) => rewrite_query(query, logic_table, actual_table),
        Statement::Insert(insert) => rewrite_insert(insert, logic_table, actual_table),
        Statement::CreateTable(create_table) => {
            if logic_table.matches_object_name(&create_table.name) {
                create_table.name = actual_table.to_object_name();
            }
            if let Some(query) = &mut create_table.query {
                rewrite_query(query, logic_table, actual_table);
            }
        }
        Statement::Update { table, from, .. } => {
            rewrite_table_with_joins(table, logic_table, actual_table);
            if let Some(from) = from {
                match from {
                    UpdateTableFromKind::BeforeSet(table)
                    | UpdateTableFromKind::AfterSet(table) => {
                        rewrite_table_with_joins(table, logic_table, actual_table);
                    }
                }
            }
        }
        Statement::Delete(delete) => rewrite_delete(delete, logic_table, actual_table),
        Statement::AlterTable { name, .. } if logic_table.matches_object_name(name) => {
            *name = actual_table.to_object_name();
        }
        Statement::Truncate { table_names, .. } => {
            for table in table_names {
                if logic_table.matches_object_name(&table.name) {
                    table.name = actual_table.to_object_name();
                }
            }
        }
        _ => {}
    }
}

fn rewrite_insert(
    insert: &mut Insert,
    logic_table: &QualifiedTableName,
    actual_table: &QualifiedTableName,
) {
    if let TableObject::TableName(name) = &mut insert.table
        && logic_table.matches_object_name(name)
    {
        *name = actual_table.to_object_name();
    }
    if let Some(source) = &mut insert.source {
        rewrite_query(source, logic_table, actual_table);
    }
}

fn rewrite_delete(
    delete: &mut Delete,
    logic_table: &QualifiedTableName,
    actual_table: &QualifiedTableName,
) {
    rewrite_from_table(&mut delete.from, logic_table, actual_table);
    if let Some(using) = &mut delete.using {
        for table in using {
            rewrite_table_with_joins(table, logic_table, actual_table);
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
    logic_table: &QualifiedTableName,
    actual_table: &QualifiedTableName,
) {
    if let Some(with) = &mut query.with {
        for cte in &mut with.cte_tables {
            rewrite_query(&mut cte.query, logic_table, actual_table);
        }
    }
    rewrite_set_expr(&mut query.body, logic_table, actual_table);
}

fn rewrite_set_expr(
    body: &mut SetExpr,
    logic_table: &QualifiedTableName,
    actual_table: &QualifiedTableName,
) {
    match body {
        SetExpr::Select(select) => {
            for table in &mut select.from {
                rewrite_table_with_joins(table, logic_table, actual_table);
            }
            // Recurse into WHERE, HAVING, and SELECT subqueries
            if let Some(selection) = &mut select.selection {
                rewrite_expr_subqueries(selection, logic_table, actual_table);
            }
            if let Some(having) = &mut select.having {
                rewrite_expr_subqueries(having, logic_table, actual_table);
            }
            for item in &mut select.projection {
                rewrite_select_item_subqueries(item, logic_table, actual_table);
            }
        }
        SetExpr::Query(query) => rewrite_query(query, logic_table, actual_table),
        SetExpr::SetOperation { left, right, .. } => {
            rewrite_set_expr(left, logic_table, actual_table);
            rewrite_set_expr(right, logic_table, actual_table);
        }
        _ => {}
    }
}

fn rewrite_expr_subqueries(
    expr: &mut sqlparser::ast::Expr,
    logic_table: &QualifiedTableName,
    actual_table: &QualifiedTableName,
) {
    match expr {
        sqlparser::ast::Expr::Subquery(query) => {
            rewrite_query(query, logic_table, actual_table);
        }
        sqlparser::ast::Expr::BinaryOp { left, right, .. } => {
            rewrite_expr_subqueries(left, logic_table, actual_table);
            rewrite_expr_subqueries(right, logic_table, actual_table);
        }
        sqlparser::ast::Expr::UnaryOp { expr, .. }
        | sqlparser::ast::Expr::Nested(expr)
        | sqlparser::ast::Expr::IsNull(expr)
        | sqlparser::ast::Expr::IsNotNull(expr)
        | sqlparser::ast::Expr::Cast { expr, .. } => {
            rewrite_expr_subqueries(expr, logic_table, actual_table);
        }
        sqlparser::ast::Expr::InSubquery { subquery, expr, .. } => {
            rewrite_expr_subqueries(expr, logic_table, actual_table);
            rewrite_query(subquery, logic_table, actual_table);
        }
        sqlparser::ast::Expr::Exists { subquery, .. } => {
            rewrite_query(subquery, logic_table, actual_table);
        }
        sqlparser::ast::Expr::Between {
            expr, low, high, ..
        } => {
            rewrite_expr_subqueries(expr, logic_table, actual_table);
            rewrite_expr_subqueries(low, logic_table, actual_table);
            rewrite_expr_subqueries(high, logic_table, actual_table);
        }
        sqlparser::ast::Expr::InList { expr, list, .. } => {
            rewrite_expr_subqueries(expr, logic_table, actual_table);
            for item in list {
                rewrite_expr_subqueries(item, logic_table, actual_table);
            }
        }
        sqlparser::ast::Expr::Case {
            operand,
            conditions,
            results,
            else_result,
            ..
        } => {
            if let Some(operand) = operand {
                rewrite_expr_subqueries(operand, logic_table, actual_table);
            }
            for cond in conditions {
                rewrite_expr_subqueries(cond, logic_table, actual_table);
            }
            for result in results {
                rewrite_expr_subqueries(result, logic_table, actual_table);
            }
            if let Some(else_result) = else_result {
                rewrite_expr_subqueries(else_result, logic_table, actual_table);
            }
        }
        sqlparser::ast::Expr::Function(func) => {
            if let sqlparser::ast::FunctionArguments::List(args) = &mut func.args {
                for arg in &mut args.args {
                    match arg {
                        sqlparser::ast::FunctionArg::Unnamed(
                            sqlparser::ast::FunctionArgExpr::Expr(expr),
                        )
                        | sqlparser::ast::FunctionArg::Named {
                            arg: sqlparser::ast::FunctionArgExpr::Expr(expr),
                            ..
                        }
                        | sqlparser::ast::FunctionArg::ExprNamed {
                            arg: sqlparser::ast::FunctionArgExpr::Expr(expr),
                            ..
                        } => rewrite_expr_subqueries(expr, logic_table, actual_table),
                        _ => {}
                    }
                }
            }
        }
        _ => {}
    }
}

fn rewrite_select_item_subqueries(
    item: &mut sqlparser::ast::SelectItem,
    logic_table: &QualifiedTableName,
    actual_table: &QualifiedTableName,
) {
    match item {
        sqlparser::ast::SelectItem::UnnamedExpr(expr)
        | sqlparser::ast::SelectItem::ExprWithAlias { expr, .. } => {
            rewrite_expr_subqueries(expr, logic_table, actual_table);
        }
        _ => {}
    }
}

fn rewrite_table_with_joins(
    table: &mut TableWithJoins,
    logic_table: &QualifiedTableName,
    actual_table: &QualifiedTableName,
) {
    rewrite_table_factor(&mut table.relation, logic_table, actual_table);
    for join in &mut table.joins {
        rewrite_table_factor(&mut join.relation, logic_table, actual_table);
    }
}

fn rewrite_table_factor(
    factor: &mut TableFactor,
    logic_table: &QualifiedTableName,
    actual_table: &QualifiedTableName,
) {
    match factor {
        TableFactor::Table { name, alias, .. } if logic_table.matches_object_name(name) => {
            *name = actual_table.to_object_name();
            if alias.is_none() && actual_table.table != logic_table.table {
                *alias = Some(TableAlias {
                    name: Ident::new(logic_table.table.as_str()),
                    columns: vec![],
                });
            }
        }
        TableFactor::Derived { subquery, .. } => rewrite_query(subquery, logic_table, actual_table),
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => rewrite_table_with_joins(table_with_joins, logic_table, actual_table),
        TableFactor::Pivot { table, .. } | TableFactor::Unpivot { table, .. } => {
            rewrite_table_factor(table, logic_table, actual_table)
        }
        _ => {}
    }
}
