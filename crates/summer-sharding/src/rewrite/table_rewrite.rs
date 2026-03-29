use sqlparser::ast::{
    Delete, FromTable, Insert, Query, SetExpr, Statement, TableFactor, TableObject, TableWithJoins,
    UpdateTableFromKind,
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
        Statement::AlterTable { name, .. } => {
            if logic_table.matches_object_name(name) {
                *name = actual_table.to_object_name();
            }
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
    if let TableObject::TableName(name) = &mut insert.table {
        if logic_table.matches_object_name(name) {
            *name = actual_table.to_object_name();
        }
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
        }
        SetExpr::Query(query) => rewrite_query(query, logic_table, actual_table),
        SetExpr::SetOperation { left, right, .. } => {
            rewrite_set_expr(left, logic_table, actual_table);
            rewrite_set_expr(right, logic_table, actual_table);
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
        TableFactor::Table { name, .. } => {
            if logic_table.matches_object_name(name) {
                *name = actual_table.to_object_name();
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
