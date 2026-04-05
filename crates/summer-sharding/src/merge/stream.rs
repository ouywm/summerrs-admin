use std::collections::VecDeque;

use sea_orm::QueryResult;

use crate::{merge::order_by, router::OrderByItem};

#[derive(Debug, Default)]
pub struct MergedRowStream {
    rows: VecDeque<QueryResult>,
    shards: Vec<VecDeque<QueryResult>>,
    order_by: Vec<OrderByItem>,
}

impl MergedRowStream {
    pub fn new(rows: Vec<QueryResult>) -> Self {
        Self {
            rows: rows.into(),
            shards: Vec::new(),
            order_by: Vec::new(),
        }
    }

    pub fn from_sorted_shards(shards: Vec<Vec<QueryResult>>, order_by: &[OrderByItem]) -> Self {
        Self {
            rows: VecDeque::new(),
            shards: shards.into_iter().map(VecDeque::from).collect(),
            order_by: order_by.to_vec(),
        }
    }
}

impl Iterator for MergedRowStream {
    type Item = QueryResult;

    fn next(&mut self) -> Option<Self::Item> {
        if self.shards.is_empty() {
            return self.rows.pop_front();
        }

        let next_index = self
            .shards
            .iter()
            .enumerate()
            .filter_map(|(index, rows)| rows.front().map(|row| (index, row)))
            .min_by(|(_, left), (_, right)| {
                order_by::compare_rows(left, right, self.order_by.as_slice())
            })
            .map(|(index, _)| index)?;

        self.shards[next_index].pop_front()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use sea_orm::Value;

    use crate::{
        merge::{MergedRowStream, row::from_values},
        router::OrderByItem,
    };

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

    #[test]
    fn merged_row_stream_merges_sorted_shards_by_order_by() {
        let shard_a = vec![
            from_values(BTreeMap::from([("id".to_string(), Value::Int(Some(1)))])),
            from_values(BTreeMap::from([("id".to_string(), Value::Int(Some(3)))])),
        ];
        let shard_b = vec![
            from_values(BTreeMap::from([("id".to_string(), Value::Int(Some(2)))])),
            from_values(BTreeMap::from([("id".to_string(), Value::Int(Some(4)))])),
        ];
        let mut stream = MergedRowStream::from_sorted_shards(
            vec![shard_a, shard_b],
            &[OrderByItem {
                column: "id".to_string(),
                asc: true,
            }],
        );

        let ids = std::iter::from_fn(|| stream.next())
            .map(|row| row.try_get::<Option<i32>>("", "id").expect("id"))
            .collect::<Vec<_>>();

        assert_eq!(ids, vec![Some(1), Some(2), Some(3), Some(4)]);
    }
}
