use std::collections::BTreeMap;

use sea_orm::{QueryResult, Value};

use crate::{
    connector::statement::{AggregateFunction, ProjectionKind, StatementContext},
    error::Result,
    merge::row::{
        compare_values, from_values, row_value_owned, value_as_f64, value_as_i64, value_sort_key,
    },
};

#[derive(Debug, Clone, Default)]
struct AverageAccumulator {
    sum: f64,
    count: i64,
}

#[derive(Debug, Clone, Default)]
struct GroupAccumulator {
    values: BTreeMap<String, Value>,
    averages: BTreeMap<String, AverageAccumulator>,
}

pub fn merge(
    shards: Vec<Vec<QueryResult>>,
    analysis: &StatementContext,
) -> Result<Vec<QueryResult>> {
    let mut groups = BTreeMap::<Vec<String>, GroupAccumulator>::new();

    for row in shards.into_iter().flatten() {
        let proxy_row = sea_orm::from_query_result_to_proxy_row(&row);
        let group_key = if analysis.group_by.is_empty() {
            vec!["__summer_global_group__".to_string()]
        } else {
            analysis
                .group_by
                .iter()
                .map(|column| {
                    row_value_owned(&proxy_row, column)
                        .map(value_sort_key)
                        .unwrap_or_else(|| "null".to_string())
                })
                .collect()
        };

        let accumulator = groups.entry(group_key).or_default();

        for projection in &analysis.projections {
            match &projection.kind {
                ProjectionKind::Wildcard => {}
                ProjectionKind::Expression { .. } => {
                    if let Some(value) =
                        row_value_owned(&proxy_row, projection.output_column.as_str())
                    {
                        accumulator
                            .values
                            .entry(projection.output_column.clone())
                            .or_insert(value);
                    }
                }
                ProjectionKind::Column { source_column } => {
                    let Some(value) =
                        row_value_owned(&proxy_row, projection.output_column.as_str())
                            .or_else(|| row_value_owned(&proxy_row, source_column.as_str()))
                    else {
                        continue;
                    };
                    accumulator
                        .values
                        .entry(projection.output_column.clone())
                        .or_insert(value);
                }
                ProjectionKind::Aggregate {
                    function,
                    avg_sum_column,
                    avg_count_column,
                    ..
                } => match function {
                    AggregateFunction::Count => merge_sum_like(
                        &mut accumulator.values,
                        projection.output_column.as_str(),
                        row_value_owned(&proxy_row, projection.output_column.as_str()),
                    ),
                    AggregateFunction::Sum => merge_sum_like(
                        &mut accumulator.values,
                        projection.output_column.as_str(),
                        row_value_owned(&proxy_row, projection.output_column.as_str()),
                    ),
                    AggregateFunction::Min => merge_min_max(
                        &mut accumulator.values,
                        projection.output_column.as_str(),
                        row_value_owned(&proxy_row, projection.output_column.as_str()),
                        true,
                    ),
                    AggregateFunction::Max => merge_min_max(
                        &mut accumulator.values,
                        projection.output_column.as_str(),
                        row_value_owned(&proxy_row, projection.output_column.as_str()),
                        false,
                    ),
                    AggregateFunction::Avg => {
                        let Some(sum_column) = avg_sum_column.as_ref() else {
                            continue;
                        };
                        let Some(count_column) = avg_count_column.as_ref() else {
                            continue;
                        };
                        let Some(sum_value) = row_value_owned(&proxy_row, sum_column.as_str())
                        else {
                            continue;
                        };
                        let Some(count_value) = row_value_owned(&proxy_row, count_column.as_str())
                        else {
                            continue;
                        };
                        let entry = accumulator
                            .averages
                            .entry(projection.output_column.clone())
                            .or_default();
                        entry.sum += value_as_f64(&sum_value).unwrap_or(0.0);
                        entry.count += value_as_i64(&count_value).unwrap_or(0);
                    }
                },
            }
        }
    }

    let mut rows = Vec::with_capacity(groups.len());
    for (_, mut accumulator) in groups {
        for (column, average) in accumulator.averages {
            let value = if average.count == 0 {
                Value::Double(None)
            } else {
                Value::Double(Some(average.sum / average.count as f64))
            };
            accumulator.values.insert(column, value);
        }
        rows.push(from_values(accumulator.values));
    }
    Ok(rows)
}

fn merge_sum_like(values: &mut BTreeMap<String, Value>, column: &str, incoming: Option<Value>) {
    let Some(incoming) = incoming else {
        return;
    };
    let Some(next) = value_as_f64(&incoming) else {
        return;
    };
    match values.get(column).and_then(value_as_f64) {
        Some(current) => {
            values.insert(column.to_string(), Value::Double(Some(current + next)));
        }
        None => {
            values.insert(column.to_string(), incoming);
        }
    }
}

fn merge_min_max(
    values: &mut BTreeMap<String, Value>,
    column: &str,
    incoming: Option<Value>,
    choose_min: bool,
) {
    let Some(incoming) = incoming else {
        return;
    };
    match values.get(column) {
        Some(existing) => {
            let ordering = compare_values(existing, &incoming);
            let replace = if choose_min {
                ordering.is_gt()
            } else {
                ordering.is_lt()
            };
            if replace {
                values.insert(column.to_string(), incoming);
            }
        }
        None => {
            values.insert(column.to_string(), incoming);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use sea_orm::Value;

    use crate::{connector::analyze_statement, merge::group_by::merge};

    #[test]
    fn group_merge_aggregates_count_sum_and_avg() {
        let analysis = analyze_statement(&sea_orm::Statement::from_string(
            sea_orm::DbBackend::Postgres,
            "SELECT day, COUNT(*) AS total, SUM(amount) AS amount, AVG(latency) AS latency FROM ai.log GROUP BY day",
        ))
        .expect("analysis");

        let shards = vec![
            vec![crate::merge::row::from_values(BTreeMap::from([
                (
                    "day".to_string(),
                    Value::String(Some("2026-03-25".to_string())),
                ),
                ("total".to_string(), Value::BigInt(Some(2))),
                ("amount".to_string(), Value::Double(Some(5.0))),
                (
                    "__summer_avg_sum_latency".to_string(),
                    Value::Double(Some(30.0)),
                ),
                (
                    "__summer_avg_count_latency".to_string(),
                    Value::BigInt(Some(2)),
                ),
            ]))],
            vec![crate::merge::row::from_values(BTreeMap::from([
                (
                    "day".to_string(),
                    Value::String(Some("2026-03-25".to_string())),
                ),
                ("total".to_string(), Value::BigInt(Some(3))),
                ("amount".to_string(), Value::Double(Some(7.0))),
                (
                    "__summer_avg_sum_latency".to_string(),
                    Value::Double(Some(60.0)),
                ),
                (
                    "__summer_avg_count_latency".to_string(),
                    Value::BigInt(Some(3)),
                ),
            ]))],
        ];

        let rows = merge(shards, &analysis).expect("merged");
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0]
                .try_get::<Option<f64>>("", "amount")
                .expect("amount"),
            Some(12.0)
        );
        assert_eq!(
            rows[0]
                .try_get::<Option<f64>>("", "latency")
                .expect("latency"),
            Some(18.0)
        );
    }
}
