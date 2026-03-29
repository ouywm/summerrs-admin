use sqlparser::ast::{
    BinaryOperator, Delete, Expr, Ident, Insert, Query, SetExpr, Statement, TableFactor,
    TableWithJoins, Value,
};

use crate::{
    config::{ShardingConfig, TenantIsolationLevel, TenantRowLevelStrategy},
    router::QualifiedTableName,
    tenant::TenantContext,
};

pub fn apply_tenant_rewrite(
    statement: &mut Statement,
    tenant: &TenantContext,
    config: &ShardingConfig,
    tables: &[QualifiedTableName],
) {
    if tenant.isolation_level != TenantIsolationLevel::SharedRow
        || config.tenant.row_level.strategy != TenantRowLevelStrategy::SqlRewrite
    {
        return;
    }

    if tables
        .iter()
        .all(|table| config.is_tenant_shared_table(table.full_name().as_str()))
    {
        return;
    }

    match statement {
        Statement::Query(query) => inject_query_filter(query, tenant, config),
        Statement::Update { selection, .. } => {
            inject_selection(selection, tenant, config, None);
        }
        Statement::Delete(delete) => inject_delete_filter(delete, tenant, config),
        Statement::Insert(insert) => inject_insert_tenant(insert, tenant, config),
        _ => {}
    }
}

fn inject_query_filter(query: &mut Query, tenant: &TenantContext, config: &ShardingConfig) {
    match query.body.as_mut() {
        SetExpr::Select(select) => {
            let qualifier = select.from.first().and_then(resolve_qualifier);
            inject_selection(&mut select.selection, tenant, config, qualifier);
        }
        SetExpr::Query(inner) => inject_query_filter(inner, tenant, config),
        SetExpr::SetOperation { left, right, .. } => {
            if let SetExpr::Select(select) = left.as_mut() {
                let qualifier = select.from.first().and_then(resolve_qualifier);
                inject_selection(&mut select.selection, tenant, config, qualifier);
            }
            if let SetExpr::Select(select) = right.as_mut() {
                let qualifier = select.from.first().and_then(resolve_qualifier);
                inject_selection(&mut select.selection, tenant, config, qualifier);
            }
        }
        _ => {}
    }
}

fn inject_delete_filter(delete: &mut Delete, tenant: &TenantContext, config: &ShardingConfig) {
    let qualifier = match &delete.from {
        sqlparser::ast::FromTable::WithFromKeyword(tables)
        | sqlparser::ast::FromTable::WithoutKeyword(tables) => {
            tables.first().and_then(resolve_qualifier)
        }
    };
    inject_selection(&mut delete.selection, tenant, config, qualifier);
}

fn inject_selection(
    selection: &mut Option<Expr>,
    tenant: &TenantContext,
    config: &ShardingConfig,
    qualifier: Option<String>,
) {
    let tenant_expr = tenant_expr(tenant, config, qualifier);
    match selection.take() {
        Some(existing) => {
            *selection = Some(Expr::BinaryOp {
                left: Box::new(existing),
                op: BinaryOperator::And,
                right: Box::new(tenant_expr),
            });
        }
        None => *selection = Some(tenant_expr),
    }
}

fn inject_insert_tenant(insert: &mut Insert, tenant: &TenantContext, config: &ShardingConfig) {
    let tenant_column = config.tenant.row_level.column_name.as_str();
    if insert
        .columns
        .iter()
        .any(|column| column.value.eq_ignore_ascii_case(tenant_column))
    {
        return;
    }

    insert.columns.push(Ident::new(tenant_column));
    if let Some(source) = &mut insert.source {
        if let SetExpr::Values(values) = source.body.as_mut() {
            for row in &mut values.rows {
                row.push(Expr::Value(Value::SingleQuotedString(
                    tenant.tenant_id.clone(),
                )));
            }
        }
    }
}

fn tenant_expr(tenant: &TenantContext, config: &ShardingConfig, qualifier: Option<String>) -> Expr {
    let column = match qualifier {
        Some(qualifier) => Expr::CompoundIdentifier(vec![
            Ident::new(qualifier),
            Ident::new(config.tenant.row_level.column_name.as_str()),
        ]),
        None => Expr::CompoundIdentifier(vec![Ident::new(
            config.tenant.row_level.column_name.as_str(),
        )]),
    };
    Expr::BinaryOp {
        left: Box::new(column),
        op: BinaryOperator::Eq,
        right: Box::new(Expr::Value(Value::SingleQuotedString(
            tenant.tenant_id.clone(),
        ))),
    }
}

fn resolve_qualifier(table: &TableWithJoins) -> Option<String> {
    resolve_table_factor_qualifier(&table.relation)
}

fn resolve_table_factor_qualifier(factor: &TableFactor) -> Option<String> {
    match factor {
        TableFactor::Table { name, alias, .. } => alias
            .as_ref()
            .map(|alias| alias.name.value.clone())
            .or_else(|| name.0.last().map(|ident| ident.value.clone())),
        _ => None,
    }
}
