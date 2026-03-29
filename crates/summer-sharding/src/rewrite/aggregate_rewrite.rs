use sqlparser::ast::{
    Expr, Function, FunctionArg, FunctionArgExpr, FunctionArgumentList, FunctionArguments, Ident,
    ObjectName, SelectItem, SetExpr, Statement as AstStatement,
};

use crate::connector::statement::{AggregateFunction, ProjectionKind, StatementContext};

pub fn apply_aggregate_rewrite(ast: &mut AstStatement, analysis: &StatementContext) {
    if !analysis.has_aggregate_projection() {
        return;
    }

    let AstStatement::Query(query) = ast else {
        return;
    };
    let SetExpr::Select(select) = query.body.as_mut() else {
        return;
    };

    let mut hidden_items = Vec::new();
    for projection in &analysis.projections {
        let ProjectionKind::Aggregate {
            function: AggregateFunction::Avg,
            source_column,
            avg_sum_column,
            avg_count_column,
        } = &projection.kind
        else {
            continue;
        };
        let Some(source_column) = source_column.as_deref() else {
            continue;
        };
        let Some(sum_alias) = avg_sum_column.as_deref() else {
            continue;
        };
        let Some(count_alias) = avg_count_column.as_deref() else {
            continue;
        };

        hidden_items.push(SelectItem::ExprWithAlias {
            expr: aggregate_expr("sum", Some(source_column)),
            alias: Ident::new(sum_alias),
        });
        hidden_items.push(SelectItem::ExprWithAlias {
            expr: aggregate_expr("count", Some(source_column)),
            alias: Ident::new(count_alias),
        });
    }
    select.projection.extend(hidden_items);
}

fn aggregate_expr(function_name: &str, source_column: Option<&str>) -> Expr {
    let arg = match source_column {
        Some(column) => FunctionArgExpr::Expr(Expr::Identifier(Ident::new(column))),
        None => FunctionArgExpr::Wildcard,
    };
    Expr::Function(Function {
        name: ObjectName(vec![Ident::new(function_name)]),
        uses_odbc_syntax: false,
        parameters: FunctionArguments::None,
        args: FunctionArguments::List(FunctionArgumentList {
            duplicate_treatment: None,
            args: vec![FunctionArg::Unnamed(arg)],
            clauses: Vec::new(),
        }),
        filter: None,
        null_treatment: None,
        over: None,
        within_group: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use sea_orm::{DbBackend, Statement};

    use crate::{
        connector::analyze_statement, rewrite::aggregate_rewrite::apply_aggregate_rewrite,
    };

    #[test]
    fn aggregate_rewrite_adds_hidden_avg_helpers() {
        let stmt = Statement::from_string(
            DbBackend::Postgres,
            "SELECT day, AVG(latency) AS latency FROM ai.log GROUP BY day",
        );
        let analysis = analyze_statement(&stmt).expect("analysis");
        let mut ast = analysis.ast.clone();
        apply_aggregate_rewrite(&mut ast, &analysis);
        let sql = ast.to_string();
        assert!(sql.contains("__summer_avg_sum_latency"));
        assert!(sql.contains("__summer_avg_count_latency"));
    }
}
