use std::collections::VecDeque;

use sea_orm::QueryResult;

#[derive(Debug, Default)]
pub struct MergedRowStream {
    rows: VecDeque<QueryResult>,
}

impl MergedRowStream {
    pub fn new(rows: Vec<QueryResult>) -> Self {
        Self { rows: rows.into() }
    }
}

impl Iterator for MergedRowStream {
    type Item = QueryResult;

    fn next(&mut self) -> Option<Self::Item> {
        self.rows.pop_front()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use sea_orm::Value;

    use crate::merge::{row::from_values, MergedRowStream};

    #[test]
    fn merged_row_stream_yields_rows_in_fifo_order() {
        let rows = vec![
            from_values(BTreeMap::from([("id".to_string(), Value::Int(Some(1)))])),
            from_values(BTreeMap::from([("id".to_string(), Value::Int(Some(2)))])),
        ];
        let mut stream = MergedRowStream::new(rows);

        let first = stream
            .next()
            .and_then(|row| row.try_get::<Option<i32>>("", "id").ok())
            .flatten();
        let second = stream
            .next()
            .and_then(|row| row.try_get::<Option<i32>>("", "id").ok())
            .flatten();

        assert_eq!(first, Some(1));
        assert_eq!(second, Some(2));
        assert!(stream.next().is_none());
    }
}
