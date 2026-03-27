use serde_json::Value as JsonValue;
use sqlparser::{
    ast::{
        BinaryOperator, Expr, Query, SetExpr, Statement as AstStatement, Value as SqlValue,
    },
    dialect::PostgreSqlDialect,
    parser::Parser,
};

use crate::{
    cdc::CdcRecord,
    error::{Result, ShardingError},
};

#[derive(Debug, Clone)]
pub(crate) struct RowFilter {
    expression: Expr,
}

impl RowFilter {
    pub(crate) fn parse(filter: &str) -> Result<Self> {
        let dialect = PostgreSqlDialect {};
        let wrapped = format!("SELECT 1 WHERE {filter}");
        let mut statements = Parser::parse_sql(&dialect, wrapped.as_str())?;
        let statement = statements
            .drain(..)
            .next()
            .ok_or_else(|| ShardingError::Parse("row filter is empty".to_string()))?;
        let AstStatement::Query(query) = statement else {
            return Err(ShardingError::Parse("row filter must parse as query".to_string()));
        };
        let Some(selection) = extract_selection(&query) else {
            return Err(ShardingError::Parse(
                "row filter does not contain a WHERE clause".to_string(),
            ));
        };
        Ok(Self { expression: selection })
    }

    pub(crate) fn matches(&self, record: &CdcRecord) -> Result<bool> {
        let row = match &record.payload {
            JsonValue::Object(object) => object,
            _ => {
                return Err(ShardingError::Unsupported(format!(
                    "row filter requires object payload for `{}`",
                    record.table
                )));
            }
        };
        eval_bool(&self.expression, row)
    }
}

fn extract_selection(query: &Query) -> Option<Expr> {
    let SetExpr::Select(select) = query.body.as_ref() else {
        return None;
    };
    select.selection.clone()
}

fn eval_bool(
    expression: &Expr,
    row: &serde_json::Map<String, JsonValue>,
) -> Result<bool> {
    match expression {
        Expr::BinaryOp { left, op, right } => match op {
            BinaryOperator::And => Ok(eval_bool(left, row)? && eval_bool(right, row)?),
            BinaryOperator::Or => Ok(eval_bool(left, row)? || eval_bool(right, row)?),
            BinaryOperator::Eq => Ok(compare_json(&eval_value(left, row)?, &eval_value(right, row)?)?
                == std::cmp::Ordering::Equal),
            BinaryOperator::NotEq => Ok(compare_json(&eval_value(left, row)?, &eval_value(right, row)?)?
                != std::cmp::Ordering::Equal),
            BinaryOperator::Gt => Ok(compare_json(&eval_value(left, row)?, &eval_value(right, row)?)?
                == std::cmp::Ordering::Greater),
            BinaryOperator::GtEq => Ok(matches!(
                compare_json(&eval_value(left, row)?, &eval_value(right, row)?)?,
                std::cmp::Ordering::Greater | std::cmp::Ordering::Equal
            )),
            BinaryOperator::Lt => Ok(compare_json(&eval_value(left, row)?, &eval_value(right, row)?)?
                == std::cmp::Ordering::Less),
            BinaryOperator::LtEq => Ok(matches!(
                compare_json(&eval_value(left, row)?, &eval_value(right, row)?)?,
                std::cmp::Ordering::Less | std::cmp::Ordering::Equal
            )),
            other => Err(ShardingError::Unsupported(format!(
                "unsupported row filter operator `{other}`"
            ))),
        },
        Expr::Nested(expression) => eval_bool(expression, row),
        Expr::IsNull(expression) => Ok(eval_value(expression, row)?.is_null()),
        Expr::IsNotNull(expression) => Ok(!eval_value(expression, row)?.is_null()),
        Expr::InList {
            expr,
            list,
            negated,
        } => {
            let value = eval_value(expr, row)?;
            let mut matched = false;
            for candidate in list {
                if compare_json(&value, &eval_value(candidate, row)?)?
                    == std::cmp::Ordering::Equal
                {
                    matched = true;
                    break;
                }
            }
            Ok(if *negated { !matched } else { matched })
        }
        other => Err(ShardingError::Unsupported(format!(
            "unsupported row filter expression `{other}`"
        ))),
    }
}

fn eval_value(expression: &Expr, row: &serde_json::Map<String, JsonValue>) -> Result<JsonValue> {
    match expression {
        Expr::Identifier(ident) => Ok(row.get(ident.value.as_str()).cloned().unwrap_or(JsonValue::Null)),
        Expr::CompoundIdentifier(parts) => Ok(parts
            .last()
            .and_then(|ident| row.get(ident.value.as_str()))
            .cloned()
            .unwrap_or(JsonValue::Null)),
        Expr::Value(value) => sql_value_to_json(value),
        Expr::Nested(expression) => eval_value(expression, row),
        other => Err(ShardingError::Unsupported(format!(
            "unsupported row filter value expression `{other}`"
        ))),
    }
}

fn sql_value_to_json(value: &SqlValue) -> Result<JsonValue> {
    match value {
        SqlValue::SingleQuotedString(value)
        | SqlValue::DoubleQuotedString(value)
        | SqlValue::TripleSingleQuotedString(value)
        | SqlValue::TripleDoubleQuotedString(value)
        | SqlValue::SingleQuotedByteStringLiteral(value)
        | SqlValue::DoubleQuotedByteStringLiteral(value)
        | SqlValue::EscapedStringLiteral(value)
        | SqlValue::UnicodeStringLiteral(value)
        | SqlValue::NationalStringLiteral(value)
        | SqlValue::HexStringLiteral(value) => Ok(JsonValue::String(value.clone())),
        SqlValue::Number(value, _) => {
            if let Ok(number) = value.parse::<i64>() {
                return Ok(JsonValue::from(number));
            }
            if let Ok(number) = value.parse::<u64>() {
                return Ok(JsonValue::from(number));
            }
            if let Ok(number) = value.parse::<f64>() {
                return Ok(JsonValue::from(number));
            }
            Err(ShardingError::Parse(format!(
                "invalid numeric row filter literal `{value}`"
            )))
        }
        SqlValue::Boolean(value) => Ok(JsonValue::Bool(*value)),
        SqlValue::Null => Ok(JsonValue::Null),
        other => Err(ShardingError::Unsupported(format!(
            "unsupported row filter literal `{other}`"
        ))),
    }
}

fn compare_json(left: &JsonValue, right: &JsonValue) -> Result<std::cmp::Ordering> {
    use std::cmp::Ordering;

    match (left, right) {
        (JsonValue::Null, JsonValue::Null) => Ok(Ordering::Equal),
        (JsonValue::Bool(left), JsonValue::Bool(right)) => Ok(left.cmp(right)),
        (JsonValue::Number(left), JsonValue::Number(right)) => compare_numbers(left, right),
        (JsonValue::String(left), JsonValue::String(right)) => Ok(left.cmp(right)),
        (JsonValue::String(left), JsonValue::Number(right)) => compare_json(
            &sql_value_to_json(&SqlValue::Number(left.clone(), false))?,
            &JsonValue::Number(right.clone()),
        ),
        (JsonValue::Number(left), JsonValue::String(right)) => compare_json(
            &JsonValue::Number(left.clone()),
            &sql_value_to_json(&SqlValue::Number(right.clone(), false))?,
        ),
        _ => Err(ShardingError::Unsupported(format!(
            "unsupported row filter comparison between `{left}` and `{right}`"
        ))),
    }
}

fn compare_numbers(
    left: &serde_json::Number,
    right: &serde_json::Number,
) -> Result<std::cmp::Ordering> {
    use std::cmp::Ordering;

    if let (Some(left), Some(right)) = (left.as_i64(), right.as_i64()) {
        return Ok(left.cmp(&right));
    }
    if let (Some(left), Some(right)) = (left.as_u64(), right.as_u64()) {
        return Ok(left.cmp(&right));
    }
    if let (Some(left), Some(right)) = (left.as_i64(), right.as_u64()) {
        return Ok(if left < 0 {
            Ordering::Less
        } else {
            (left as u64).cmp(&right)
        });
    }
    if let (Some(left), Some(right)) = (left.as_u64(), right.as_i64()) {
        return Ok(if right < 0 {
            Ordering::Greater
        } else {
            left.cmp(&(right as u64))
        });
    }
    let left = left
        .as_f64()
        .ok_or_else(|| ShardingError::Parse(format!("invalid numeric value `{left}`")))?;
    let right = right
        .as_f64()
        .ok_or_else(|| ShardingError::Parse(format!("invalid numeric value `{right}`")))?;
    left.partial_cmp(&right).ok_or_else(|| {
        ShardingError::Parse(format!("cannot compare numeric values `{left}` and `{right}`"))
    })
}

#[cfg(test)]
mod tests {
    use crate::cdc::{CdcOperation, CdcRecord, filter::RowFilter};

    fn record(payload: serde_json::Value) -> CdcRecord {
        CdcRecord {
            table: "ai.log".to_string(),
            key: "1".to_string(),
            payload,
            operation: CdcOperation::Snapshot,
            source_lsn: None,
        }
    }

    #[test]
    fn row_filter_matches_exact_string_column() {
        let filter = RowFilter::parse("tenant_id = 'T-001'").expect("parse");
        assert!(filter
            .matches(&record(serde_json::json!({"tenant_id":"T-001","id":1})))
            .expect("match"));
        assert!(!filter
            .matches(&record(serde_json::json!({"tenant_id":"T-002","id":1})))
            .expect("mismatch"));
    }

    #[test]
    fn row_filter_supports_boolean_composition() {
        let filter = RowFilter::parse("tenant_id = 'T-001' AND id >= 2").expect("parse");
        assert!(filter
            .matches(&record(serde_json::json!({"tenant_id":"T-001","id":2})))
            .expect("match"));
        assert!(!filter
            .matches(&record(serde_json::json!({"tenant_id":"T-001","id":1})))
            .expect("mismatch"));
    }

    #[test]
    fn row_filter_compares_large_integers_without_precision_loss() {
        let large = 9_007_199_254_740_993_u64;
        let filter = RowFilter::parse(format!("id = {large}").as_str()).expect("parse");

        assert!(filter
            .matches(&record(serde_json::json!({"id": large})))
            .expect("match"));
        assert!(!filter
            .matches(&record(serde_json::json!({"id": large + 1})))
            .expect("mismatch"));
    }
}
