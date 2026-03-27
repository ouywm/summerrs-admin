use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use async_trait::async_trait;
use reqwest::Client;
use serde_json::{Map, Value as JsonValue};

use crate::{
    cdc::{CdcOperation, CdcRecord, CdcSink, CdcSinkKind},
    error::{Result, ShardingError},
};

#[derive(Debug, Clone)]
pub struct ClickHouseHttpSink {
    client: Client,
    base_url: String,
    table_map: Arc<BTreeMap<String, String>>,
    key_columns: Arc<BTreeMap<String, Vec<String>>>,
    version_column: Option<String>,
    version: Arc<AtomicU64>,
}

impl ClickHouseHttpSink {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self::with_table_map(base_url, [])
    }

    pub fn with_table_map(
        base_url: impl Into<String>,
        table_map: impl IntoIterator<Item = (String, String)>,
    ) -> Self {
        Self::with_table_map_and_keys(base_url, table_map, [])
    }

    pub fn with_table_map_and_keys(
        base_url: impl Into<String>,
        table_map: impl IntoIterator<Item = (String, String)>,
        key_columns: impl IntoIterator<Item = (String, Vec<String>)>,
    ) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.into(),
            table_map: Arc::new(table_map.into_iter().collect()),
            key_columns: Arc::new(key_columns.into_iter().collect()),
            version_column: Some("version".to_string()),
            version: Arc::new(AtomicU64::new(1)),
        }
    }

    fn target_table(&self, source_table: &str) -> String {
        self.table_map
            .get(source_table)
            .cloned()
            .unwrap_or_else(|| unqualified_table_name(source_table))
    }

    fn key_columns_for(&self, table: &str) -> Vec<String> {
        self.key_columns
            .get(table)
            .cloned()
            .unwrap_or_else(|| vec!["id".to_string()])
    }

    async fn insert_rows(&self, table: &str, records: &[CdcRecord]) -> Result<usize> {
        if records.is_empty() {
            return Ok(0);
        }

        let body = records
            .iter()
            .map(|record| record_to_clickhouse_row(record, self.next_version(), self.version_column.as_deref()))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .map(|row| serde_json::to_string(&row).map_err(|err| ShardingError::Parse(err.to_string())))
            .collect::<Result<Vec<_>>>()?
            .join("\n");
        let sql = format!("INSERT INTO {table} FORMAT JSONEachRow\n{body}");
        self.execute_sql(sql.as_str()).await?;
        Ok(records.len())
    }

    async fn delete_row(&self, table: &str, record: &CdcRecord) -> Result<()> {
        self.execute_sql(self.delete_sql(table, record)?.as_str())
            .await
    }

    fn delete_sql(&self, table: &str, record: &CdcRecord) -> Result<String> {
        let key_columns = self.key_columns_for(table);
        let key_values = delete_key_values(record, key_columns.as_slice())?;
        let predicates = key_values
            .into_iter()
            .map(|(column, value)| {
                Ok(format!(
                    "{column} = {}",
                    clickhouse_literal(&value)?
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(format!(
            "ALTER TABLE {table} DELETE WHERE {}",
            predicates.join(" AND ")
        ))
    }

    async fn execute_sql(&self, sql: &str) -> Result<()> {
        let response = self
            .client
            .post(self.base_url.as_str())
            .body(sql.to_string())
            .send()
            .await
            .map_err(|err| ShardingError::Route(err.to_string()))?;
        if response.status().is_success() {
            Ok(())
        } else {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<unable to read clickhouse response body>".to_string());
            Err(ShardingError::Route(format!(
                "clickhouse request failed with status {status}: {body}"
            )))
        }
    }

    fn next_version(&self) -> u64 {
        self.version.fetch_add(1, Ordering::Relaxed)
    }
}

#[async_trait]
impl CdcSink for ClickHouseHttpSink {
    async fn write_batch(&self, records: &[CdcRecord]) -> Result<usize> {
        let mut by_table = BTreeMap::<String, Vec<CdcRecord>>::new();
        for record in records {
            by_table
                .entry(self.target_table(record.table.as_str()))
                .or_default()
                .push(record.clone());
        }

        let mut written = 0usize;
        for (table, records) in by_table {
            written += self.insert_rows(table.as_str(), records.as_slice()).await?;
        }
        Ok(written)
    }

    async fn apply_change(&self, record: &CdcRecord) -> Result<()> {
        let target_table = self.target_table(record.table.as_str());
        match record.operation {
            CdcOperation::Delete => self.delete_row(target_table.as_str(), record).await,
            CdcOperation::Insert | CdcOperation::Snapshot => {
                self.insert_rows(target_table.as_str(), std::slice::from_ref(record))
                    .await
                    .map(|_| ())
            }
            CdcOperation::Update => {
                self.delete_row(target_table.as_str(), record).await?;
                self.insert_rows(target_table.as_str(), std::slice::from_ref(record))
                    .await
                    .map(|_| ())
            }
        }
    }

    fn kind(&self) -> CdcSinkKind {
        CdcSinkKind::ClickHouse
    }
}

fn record_to_clickhouse_row(
    record: &CdcRecord,
    version: u64,
    version_column: Option<&str>,
) -> Result<JsonValue> {
    let mut payload = record_to_object(record)?;
    if let Some(version_column) = version_column {
        payload.insert(version_column.to_string(), JsonValue::from(version));
    }
    Ok(JsonValue::Object(
        payload
            .into_iter()
            .map(|(key, value)| (key, normalize_clickhouse_value(value)))
            .collect(),
    ))
}

fn record_to_object(record: &CdcRecord) -> Result<Map<String, JsonValue>> {
    match &record.payload {
        JsonValue::Object(object) => Ok(object.clone()),
        value => {
            let mut object = Map::new();
            object.insert("id".to_string(), parse_key_value(record.key.as_str()));
            object.insert("payload".to_string(), value.clone());
            Ok(object)
        }
    }
}

fn normalize_clickhouse_value(value: JsonValue) -> JsonValue {
    match value {
        JsonValue::Array(_) | JsonValue::Object(_) => JsonValue::String(value.to_string()),
        other => other,
    }
}

fn delete_key_values(record: &CdcRecord, key_columns: &[String]) -> Result<Vec<(String, JsonValue)>> {
    match &record.payload {
        JsonValue::Object(object) => key_columns
            .iter()
            .map(|column| {
                object
                    .get(column)
                    .cloned()
                    .map(|value| (column.clone(), value))
                    .ok_or_else(|| {
                        ShardingError::Config(format!(
                            "clickhouse delete requires key column `{column}` in payload for `{}`",
                            record.table
                        ))
                    })
            })
            .collect(),
        value => {
            if key_columns.len() != 1 {
                return Err(ShardingError::Unsupported(format!(
                    "clickhouse delete for `{}` requires object payload when multiple key columns are configured; got `{value}`",
                    record.table
                )));
            }
            Ok(vec![(
                key_columns[0].clone(),
                parse_key_value(record.key.as_str()),
            )])
        }
    }
}

fn clickhouse_literal(value: &JsonValue) -> Result<String> {
    Ok(match value {
        JsonValue::Null => "NULL".to_string(),
        JsonValue::Bool(value) => {
            if *value {
                "1".to_string()
            } else {
                "0".to_string()
            }
        }
        JsonValue::Number(value) => value.to_string(),
        JsonValue::String(value) => format!("'{}'", value.replace('\'', "\\'")),
        JsonValue::Array(_) | JsonValue::Object(_) => {
            return Err(ShardingError::Unsupported(
                "clickhouse delete predicate does not support composite literal values".to_string(),
            ))
        }
    })
}

fn parse_key_value(key: &str) -> JsonValue {
    if let Ok(value) = key.parse::<i64>() {
        JsonValue::from(value)
    } else if let Ok(value) = key.parse::<u64>() {
        JsonValue::from(value)
    } else {
        JsonValue::String(key.to_string())
    }
}

fn unqualified_table_name(table: &str) -> String {
    table
        .rsplit_once('.')
        .map(|(_, table)| table.to_string())
        .unwrap_or_else(|| table.to_string())
}

#[cfg(test)]
mod tests {
    use super::ClickHouseHttpSink;
    use crate::cdc::{CdcOperation, CdcRecord};

    #[test]
    fn clickhouse_delete_sql_uses_configured_non_id_key_columns() {
        let sink = ClickHouseHttpSink::with_table_map_and_keys(
            "http://127.0.0.1:8123",
            [("public.events".to_string(), "default.events".to_string())],
            [(
                "default.events".to_string(),
                vec!["tenant_id".to_string(), "event_code".to_string()],
            )],
        );
        let record = CdcRecord {
            table: "public.events".to_string(),
            key: "ignored".to_string(),
            payload: serde_json::json!({
                "tenant_id": "T-001",
                "event_code": "evt_1",
                "payload": {"name":"alpha"}
            }),
            operation: CdcOperation::Delete,
            source_lsn: None,
        };

        let sql = sink
            .delete_sql("default.events", &record)
            .expect("delete sql");

        assert_eq!(
            sql,
            "ALTER TABLE default.events DELETE WHERE tenant_id = 'T-001' AND event_code = 'evt_1'"
        );
    }
}
