use sea_orm::QueryResult;

pub fn apply(
    mut rows: Vec<QueryResult>,
    limit: Option<u64>,
    offset: Option<u64>,
) -> Vec<QueryResult> {
    let offset = offset.unwrap_or(0) as usize;
    let limit = limit.map(|value| value as usize);

    if offset > 0 {
        rows = rows.into_iter().skip(offset).collect();
    }
    if let Some(limit) = limit {
        rows.truncate(limit);
    }
    rows
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use sea_orm::{ProxyRow, QueryResult, Value};

    use crate::merge::limit::apply;

    fn row(values: BTreeMap<String, Value>) -> QueryResult {
        ProxyRow::new(values).into()
    }

    #[test]
    fn limit_apply_respects_offset_and_limit() {
        let rows = (1..=5)
            .map(|value| {
                row(BTreeMap::from([(
                    "id".to_string(),
                    Value::Int(Some(value)),
                )]))
            })
            .collect::<Vec<_>>();

        let sliced = apply(rows, Some(2), Some(1));
        let values = sliced
            .iter()
            .map(|row| row.try_get::<Option<i32>>("", "id").expect("id"))
            .collect::<Vec<_>>();

        assert_eq!(values, vec![Some(2), Some(3)]);
    }
}
