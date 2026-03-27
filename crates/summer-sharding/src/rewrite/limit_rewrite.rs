pub fn inflate_limit_for_fanout(
    statement: &mut sqlparser::ast::Statement,
    limit: Option<u64>,
    offset: Option<u64>,
) {
    let Some(limit) = limit else {
        return;
    };
    let inflated_limit = limit.saturating_add(offset.unwrap_or(0));

    if let sqlparser::ast::Statement::Query(query) = statement {
        query.limit = Some(sqlparser::ast::Expr::Value(sqlparser::ast::Value::Number(
            inflated_limit.to_string(),
            false,
        )));
        query.offset = None;
    }
}
