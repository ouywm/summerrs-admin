use std::{cmp::Ordering, collections::BTreeMap};

use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime, Utc};
use sea_orm::{ProxyRow, QueryResult, Value};

pub fn from_values(values: BTreeMap<String, Value>) -> QueryResult {
    ProxyRow::new(values).into()
}

pub fn row_value_owned(row: &ProxyRow, column: &str) -> Option<Value> {
    row.values
        .iter()
        .find(|(candidate, _)| candidate.eq_ignore_ascii_case(column))
        .map(|(_, value)| value.clone())
}

pub fn value_as_i64(value: &Value) -> Option<i64> {
    match value {
        Value::TinyInt(value) => value.map(i64::from),
        Value::SmallInt(value) => value.map(i64::from),
        Value::Int(value) => value.map(i64::from),
        Value::BigInt(value) => *value,
        Value::TinyUnsigned(value) => value.map(i64::from),
        Value::SmallUnsigned(value) => value.map(i64::from),
        Value::Unsigned(value) => value.map(i64::from),
        Value::BigUnsigned(value) => value.map(|value| value as i64),
        Value::Float(value) => value.map(|value| value as i64),
        Value::Double(value) => value.map(|value| value as i64),
        Value::String(value) => value.as_ref().and_then(|value| value.parse::<i64>().ok()),
        _ => None,
    }
}

pub fn value_as_f64(value: &Value) -> Option<f64> {
    match value {
        Value::TinyInt(value) => value.map(|value| value as f64),
        Value::SmallInt(value) => value.map(|value| value as f64),
        Value::Int(value) => value.map(|value| value as f64),
        Value::BigInt(value) => value.map(|value| value as f64),
        Value::TinyUnsigned(value) => value.map(|value| value as f64),
        Value::SmallUnsigned(value) => value.map(|value| value as f64),
        Value::Unsigned(value) => value.map(|value| value as f64),
        Value::BigUnsigned(value) => value.map(|value| value as f64),
        Value::Float(value) => value.map(f64::from),
        Value::Double(value) => *value,
        Value::String(value) => value.as_ref().and_then(|value| value.parse::<f64>().ok()),
        _ => None,
    }
}

pub fn value_as_string(value: &Value) -> Option<String> {
    match value {
        Value::Bool(value) => value.map(|value| value.to_string()),
        Value::TinyInt(value) => value.map(|value| value.to_string()),
        Value::SmallInt(value) => value.map(|value| value.to_string()),
        Value::Int(value) => value.map(|value| value.to_string()),
        Value::BigInt(value) => value.map(|value| value.to_string()),
        Value::TinyUnsigned(value) => value.map(|value| value.to_string()),
        Value::SmallUnsigned(value) => value.map(|value| value.to_string()),
        Value::Unsigned(value) => value.map(|value| value.to_string()),
        Value::BigUnsigned(value) => value.map(|value| value.to_string()),
        Value::Float(value) => value.map(|value| value.to_string()),
        Value::Double(value) => value.map(|value| value.to_string()),
        Value::String(value) => value.clone(),
        Value::Char(value) => value.map(|value| value.to_string()),
        Value::ChronoDate(value) => value.map(|value| value.to_string()),
        Value::ChronoDateTime(value) => value.map(|value| value.to_string()),
        Value::ChronoDateTimeUtc(value) => value.map(|value| value.to_rfc3339()),
        Value::ChronoDateTimeLocal(value) => value.map(|value| value.to_rfc3339()),
        Value::ChronoDateTimeWithTimeZone(value) => value.map(|value| value.to_rfc3339()),
        _ => None,
    }
}

pub fn value_sort_key(value: Value) -> String {
    match value {
        Value::Bool(value) => format!("bool:{value:?}"),
        Value::TinyInt(value) => format!("i8:{value:?}"),
        Value::SmallInt(value) => format!("i16:{value:?}"),
        Value::Int(value) => format!("i32:{value:?}"),
        Value::BigInt(value) => format!("i64:{value:?}"),
        Value::TinyUnsigned(value) => format!("u8:{value:?}"),
        Value::SmallUnsigned(value) => format!("u16:{value:?}"),
        Value::Unsigned(value) => format!("u32:{value:?}"),
        Value::BigUnsigned(value) => format!("u64:{value:?}"),
        Value::Float(value) => format!("f32:{value:?}"),
        Value::Double(value) => format!("f64:{value:?}"),
        Value::String(value) => format!("str:{value:?}"),
        Value::Char(value) => format!("char:{value:?}"),
        Value::ChronoDate(value) => format!("date:{value:?}"),
        Value::ChronoDateTime(value) => format!("datetime:{value:?}"),
        Value::ChronoDateTimeUtc(value) => format!("utc:{value:?}"),
        Value::ChronoDateTimeLocal(value) => format!("local:{value:?}"),
        Value::ChronoDateTimeWithTimeZone(value) => format!("tz:{value:?}"),
        other => format!("{other:?}"),
    }
}

pub fn compare_values(left: &Value, right: &Value) -> Ordering {
    if let (Some(left), Some(right)) = (value_as_f64(left), value_as_f64(right)) {
        return left.partial_cmp(&right).unwrap_or(Ordering::Equal);
    }
    if let (Some(left), Some(right)) = (timestamp_millis(left), timestamp_millis(right)) {
        return left.cmp(&right);
    }
    if let (Some(left), Some(right)) = (value_as_string(left), value_as_string(right)) {
        return left.cmp(&right);
    }
    format!("{left:?}").cmp(&format!("{right:?}"))
}

fn timestamp_millis(value: &Value) -> Option<i64> {
    match value {
        Value::ChronoDateTimeWithTimeZone(value) => value.map(|value| value.timestamp_millis()),
        Value::ChronoDateTimeUtc(value) => value.map(|value| value.timestamp_millis()),
        Value::ChronoDateTimeLocal(value) => value.map(|value| value.timestamp_millis()),
        Value::ChronoDateTime(value) => value.map(|value| value.and_utc().timestamp_millis()),
        Value::ChronoDate(value) => value.and_then(|value| {
            value
                .and_hms_opt(0, 0, 0)
                .map(|datetime| datetime.and_utc().timestamp_millis())
        }),
        Value::String(value) => value
            .as_ref()
            .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
            .map(|value| value.timestamp_millis())
            .or_else(|| {
                value.as_ref().and_then(|value| {
                    NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S")
                        .ok()
                        .map(|value| value.and_utc().timestamp_millis())
                })
            })
            .or_else(|| {
                value.as_ref().and_then(|value| {
                    NaiveDate::parse_from_str(value, "%Y-%m-%d")
                        .ok()
                        .and_then(|value| value.and_hms_opt(0, 0, 0))
                        .map(|value| value.and_utc().timestamp_millis())
                })
            }),
        _ => None,
    }
}

#[allow(dead_code)]
fn _keep_imports(_: DateTime<FixedOffset>, _: DateTime<Utc>) {}
