use std::cmp::Ordering;

use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime, Utc};
use sea_orm::QueryResult;

use crate::router::OrderByItem;

#[derive(Debug, Clone, PartialEq)]
enum SortValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Timestamp(i64),
}

pub fn merge(mut rows: Vec<QueryResult>, order_by: &[OrderByItem]) -> Vec<QueryResult> {
    if order_by.is_empty() {
        return rows;
    }

    rows.sort_by(|left, right| compare_rows(left, right, order_by));
    rows
}

fn compare_rows(left: &QueryResult, right: &QueryResult, order_by: &[OrderByItem]) -> Ordering {
    for item in order_by {
        let left_value = sort_value(left, item.column.as_str());
        let right_value = sort_value(right, item.column.as_str());
        let ordering = compare_values(&left_value, &right_value);
        if ordering != Ordering::Equal {
            return if item.asc {
                ordering
            } else {
                ordering.reverse()
            };
        }
    }
    Ordering::Equal
}

fn compare_values(left: &SortValue, right: &SortValue) -> Ordering {
    match (left, right) {
        (SortValue::Null, SortValue::Null) => Ordering::Equal,
        (SortValue::Null, _) => Ordering::Greater,
        (_, SortValue::Null) => Ordering::Less,
        (SortValue::Bool(left), SortValue::Bool(right)) => left.cmp(right),
        (SortValue::Int(left), SortValue::Int(right)) => left.cmp(right),
        (SortValue::Float(left), SortValue::Float(right)) => {
            left.partial_cmp(right).unwrap_or(Ordering::Equal)
        }
        (SortValue::String(left), SortValue::String(right)) => left.cmp(right),
        (SortValue::Timestamp(left), SortValue::Timestamp(right)) => left.cmp(right),
        (left, right) => {
            let left = format!("{left:?}");
            let right = format!("{right:?}");
            left.cmp(&right)
        }
    }
}

fn sort_value(row: &QueryResult, column: &str) -> SortValue {
    row.try_get::<Option<i64>>("", column)
        .ok()
        .flatten()
        .map(SortValue::Int)
        .or_else(|| {
            row.try_get::<Option<i32>>("", column)
                .ok()
                .flatten()
                .map(|value| SortValue::Int(value as i64))
        })
        .or_else(|| {
            row.try_get::<Option<i16>>("", column)
                .ok()
                .flatten()
                .map(|value| SortValue::Int(value as i64))
        })
        .or_else(|| {
            row.try_get::<Option<u64>>("", column)
                .ok()
                .flatten()
                .map(|value| SortValue::Int(value as i64))
        })
        .or_else(|| {
            row.try_get::<Option<u32>>("", column)
                .ok()
                .flatten()
                .map(|value| SortValue::Int(value as i64))
        })
        .or_else(|| {
            row.try_get::<Option<bool>>("", column)
                .ok()
                .flatten()
                .map(SortValue::Bool)
        })
        .or_else(|| {
            row.try_get::<Option<f64>>("", column)
                .ok()
                .flatten()
                .map(SortValue::Float)
        })
        .or_else(|| {
            row.try_get::<Option<f32>>("", column)
                .ok()
                .flatten()
                .map(|value| SortValue::Float(value as f64))
        })
        .or_else(|| {
            row.try_get::<Option<DateTime<FixedOffset>>>("", column)
                .ok()
                .flatten()
                .map(|value| SortValue::Timestamp(value.timestamp_millis()))
        })
        .or_else(|| {
            row.try_get::<Option<DateTime<Utc>>>("", column)
                .ok()
                .flatten()
                .map(|value| SortValue::Timestamp(value.timestamp_millis()))
        })
        .or_else(|| {
            row.try_get::<Option<NaiveDateTime>>("", column)
                .ok()
                .flatten()
                .map(|value| SortValue::Timestamp(value.and_utc().timestamp_millis()))
        })
        .or_else(|| {
            row.try_get::<Option<NaiveDate>>("", column)
                .ok()
                .flatten()
                .and_then(|value| value.and_hms_opt(0, 0, 0))
                .map(|value| SortValue::Timestamp(value.and_utc().timestamp_millis()))
        })
        .or_else(|| {
            row.try_get::<Option<String>>("", column)
                .ok()
                .flatten()
                .map(SortValue::String)
        })
        .unwrap_or(SortValue::Null)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use sea_orm::Value;

    use crate::{
        merge::{order_by::merge, row::from_values},
        router::OrderByItem,
    };

    #[test]
    fn order_by_merge_sorts_ascending_and_puts_null_last() {
        let rows = vec![
            from_values(BTreeMap::from([("sort".to_string(), Value::Int(Some(3)))])),
            from_values(BTreeMap::from([("sort".to_string(), Value::Int(Some(1)))])),
            from_values(BTreeMap::from([("sort".to_string(), Value::Int(None))])),
        ];

        let merged = merge(
            rows,
            &[OrderByItem {
                column: "sort".to_string(),
                asc: true,
            }],
        );

        let values = merged
            .iter()
            .map(|row| row.try_get::<Option<i32>>("", "sort").expect("sort"))
            .collect::<Vec<_>>();
        assert_eq!(values, vec![Some(1), Some(3), None]);
    }

    #[test]
    fn order_by_merge_sorts_descending() {
        let rows = vec![
            from_values(BTreeMap::from([(
                "name".to_string(),
                Value::String(Some("a".to_string())),
            )])),
            from_values(BTreeMap::from([(
                "name".to_string(),
                Value::String(Some("c".to_string())),
            )])),
            from_values(BTreeMap::from([(
                "name".to_string(),
                Value::String(Some("b".to_string())),
            )])),
        ];

        let merged = merge(
            rows,
            &[OrderByItem {
                column: "name".to_string(),
                asc: false,
            }],
        );

        let values = merged
            .iter()
            .map(|row| row.try_get::<String>("", "name").expect("name"))
            .collect::<Vec<_>>();
        assert_eq!(values, vec!["c", "b", "a"]);
    }
}
