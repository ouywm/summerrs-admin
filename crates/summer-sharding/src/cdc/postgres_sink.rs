use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use async_trait::async_trait;
use parking_lot::Mutex;
use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, Statement, Value};
use serde_json::{Map, Value as JsonValue};

use crate::{
    cdc::{CdcOperation, CdcRecord, CdcSink, CdcSinkKind},
    error::{Result, ShardingError},
};

#[derive(Debug, Clone, Default)]
struct TableMetadata {
    primary_keys: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct PostgresTableSink {
    connection: Arc<DatabaseConnection>,
    table_map: Arc<BTreeMap<String, String>>,
    metadata: Arc<Mutex<BTreeMap<String, TableMetadata>>>,
}

impl PostgresTableSink {
    pub fn new(connection: Arc<DatabaseConnection>) -> Self {
        Self::with_table_map(connection, [])
    }

    pub fn with_table_map(
        connection: Arc<DatabaseConnection>,
        table_map: impl IntoIterator<Item = (String, String)>,
    ) -> Self {
        Self {
            connection,
            table_map: Arc::new(table_map.into_iter().collect()),
            metadata: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    fn target_table(&self, source_table: &str) -> String {
        self.table_map
            .get(source_table)
            .cloned()
            .unwrap_or_else(|| source_table.to_string())
    }

    async fn metadata_for(&self, table: &str) -> Result<TableMetadata> {
        if let Some(metadata) = self.metadata.lock().get(table).cloned() {
            return Ok(metadata);
        }

        let (schema, relation) = split_qualified_table(table)?;
        let rows = self
            .connection
            .query_all_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                r#"
                SELECT a.attname
                FROM pg_index i
                JOIN pg_class c ON c.oid = i.indrelid
                JOIN pg_namespace n ON n.oid = c.relnamespace
                JOIN unnest(i.indkey) WITH ORDINALITY AS keys(attnum, ord) ON true
                JOIN pg_attribute a ON a.attrelid = c.oid AND a.attnum = keys.attnum
                WHERE i.indisprimary
                  AND n.nspname = $1
                  AND c.relname = $2
                ORDER BY keys.ord
                "#,
                [schema.into(), relation.into()],
            ))
            .await?;
        let primary_keys = rows
            .into_iter()
            .map(|row| row.try_get::<String>("", "attname"))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        if primary_keys.is_empty() {
            return Err(ShardingError::Config(format!(
                "postgres table sink requires primary key columns for `{table}`"
            )));
        }

        let metadata = TableMetadata { primary_keys };
        self.metadata
            .lock()
            .insert(table.to_string(), metadata.clone());
        Ok(metadata)
    }

    async fn upsert(&self, record: &CdcRecord) -> Result<()> {
        let target_table = self.target_table(record.table.as_str());
        self.upsert_into(record, target_table.as_str()).await
    }

    async fn upsert_into(&self, record: &CdcRecord, target_table: &str) -> Result<()> {
        let metadata = self.metadata_for(target_table).await?;
        let mut payload = object_payload(record, &metadata.primary_keys)?;
        for key in &metadata.primary_keys {
            if !payload.contains_key(key) {
                return Err(ShardingError::Config(format!(
                    "cdc payload for `{}` missing primary key column `{key}`",
                    record.table
                )));
            }
        }

        let columns = payload.keys().cloned().collect::<BTreeSet<_>>();
        let columns = columns.into_iter().collect::<Vec<_>>();
        let placeholders = (1..=columns.len())
            .map(|index| format!("${index}"))
            .collect::<Vec<_>>();
        let params = columns
            .iter()
            .map(|column| json_value_to_sql_value(payload.remove(column).expect("column payload")))
            .collect::<Vec<_>>();

        let pk_set = metadata
            .primary_keys
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        let update_columns = columns
            .iter()
            .filter(|column| !pk_set.contains(column.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        let update_clause = if update_columns.is_empty() {
            "DO NOTHING".to_string()
        } else {
            format!(
                "DO UPDATE SET {}",
                update_columns
                    .iter()
                    .map(|column| format!(
                        "{quoted} = EXCLUDED.{quoted}",
                        quoted = quote_ident(column)
                    ))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };

        let sql = format!(
            "INSERT INTO {table} ({columns}) VALUES ({values}) ON CONFLICT ({pk}) {update_clause}",
            table = quote_qualified_table(target_table)?,
            columns = columns
                .iter()
                .map(|column| quote_ident(column))
                .collect::<Vec<_>>()
                .join(", "),
            values = placeholders.join(", "),
            pk = metadata
                .primary_keys
                .iter()
                .map(|column| quote_ident(column))
                .collect::<Vec<_>>()
                .join(", "),
        );
        self.connection
            .execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                sql,
                params,
            ))
            .await?;
        Ok(())
    }

    async fn delete(&self, record: &CdcRecord) -> Result<()> {
        let target_table = self.target_table(record.table.as_str());
        self.delete_from(record, target_table.as_str()).await
    }

    async fn delete_from(&self, record: &CdcRecord, target_table: &str) -> Result<()> {
        let metadata = self.metadata_for(target_table).await?;
        let payload = object_payload(record, &metadata.primary_keys)?;
        let mut params = Vec::with_capacity(metadata.primary_keys.len());
        let predicates = metadata
            .primary_keys
            .iter()
            .enumerate()
            .map(|(index, column)| {
                let value = payload.get(column).cloned().ok_or_else(|| {
                    ShardingError::Config(format!(
                        "cdc delete payload for `{}` missing primary key column `{column}`",
                        record.table
                    ))
                })?;
                params.push(json_value_to_sql_value(value));
                Ok(format!("{} = ${}", quote_ident(column), index + 1))
            })
            .collect::<Result<Vec<_>>>()?;

        let sql = format!(
            "DELETE FROM {} WHERE {}",
            quote_qualified_table(target_table)?,
            predicates.join(" AND ")
        );
        self.connection
            .execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                sql,
                params,
            ))
            .await?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct PostgresHashShardSink {
    table_sink: PostgresTableSink,
    target_tables: Arc<Vec<String>>,
}

impl PostgresHashShardSink {
    pub fn new(connection: Arc<DatabaseConnection>, target_tables: Vec<String>) -> Self {
        Self {
            table_sink: PostgresTableSink::new(connection),
            target_tables: Arc::new(target_tables),
        }
    }

    fn target_table_for(&self, record: &CdcRecord) -> Result<String> {
        let table_count = self.target_tables.len();
        if table_count == 0 {
            return Err(ShardingError::Config(
                "postgres hash shard sink requires at least one target table".to_string(),
            ));
        }
        let hash_value = parse_hash_key(record.key.as_str())?;
        let index = hash_value.rem_euclid(table_count as i64) as usize;
        Ok(self.target_tables[index].clone())
    }
}

#[async_trait]
impl CdcSink for PostgresTableSink {
    async fn write_batch(&self, records: &[CdcRecord]) -> Result<usize> {
        for record in records {
            self.upsert(record).await?;
        }
        Ok(records.len())
    }

    async fn apply_change(&self, record: &CdcRecord) -> Result<()> {
        match record.operation {
            CdcOperation::Delete => self.delete(record).await,
            CdcOperation::Insert | CdcOperation::Update | CdcOperation::Snapshot => {
                self.upsert(record).await
            }
        }
    }
}

#[async_trait]
impl CdcSink for PostgresHashShardSink {
    async fn write_batch(&self, records: &[CdcRecord]) -> Result<usize> {
        for record in records {
            let target_table = self.target_table_for(record)?;
            self.table_sink
                .upsert_into(record, target_table.as_str())
                .await?;
        }
        Ok(records.len())
    }

    async fn apply_change(&self, record: &CdcRecord) -> Result<()> {
        let target_table = self.target_table_for(record)?;
        match record.operation {
            CdcOperation::Delete => {
                self.table_sink
                    .delete_from(record, target_table.as_str())
                    .await
            }
            CdcOperation::Insert | CdcOperation::Update | CdcOperation::Snapshot => {
                self.table_sink
                    .upsert_into(record, target_table.as_str())
                    .await
            }
        }
    }

    fn kind(&self) -> CdcSinkKind {
        CdcSinkKind::HashSharded
    }
}

fn object_payload(record: &CdcRecord, primary_keys: &[String]) -> Result<Map<String, JsonValue>> {
    match &record.payload {
        JsonValue::Object(object) => Ok(object.clone()),
        value => {
            if primary_keys.len() != 1 {
                return Err(ShardingError::Unsupported(format!(
                    "non-object cdc payload for `{}` only supports single-column primary keys",
                    record.table
                )));
            }
            let mut object = Map::new();
            object.insert(
                primary_keys[0].clone(),
                parse_key_value(record.key.as_str()),
            );
            object.insert("payload".to_string(), value.clone());
            Ok(object)
        }
    }
}

fn parse_key_value(key: &str) -> JsonValue {
    if let Ok(value) = key.parse::<i64>() {
        JsonValue::from(value)
    } else if let Ok(value) = key.parse::<u64>() {
        JsonValue::from(value)
    } else if let Ok(value) = key.parse::<f64>() {
        JsonValue::from(value)
    } else if key.eq_ignore_ascii_case("true") || key.eq_ignore_ascii_case("false") {
        JsonValue::from(key.eq_ignore_ascii_case("true"))
    } else {
        JsonValue::String(key.to_string())
    }
}

fn parse_hash_key(key: &str) -> Result<i64> {
    if let Ok(value) = key.parse::<i64>() {
        return Ok(value);
    }
    if let Ok(value) = key.parse::<u64>() {
        return i64::try_from(value).map_err(|_| {
            ShardingError::Parse(format!(
                "hash shard sink key `{key}` does not fit into signed 64-bit integer"
            ))
        });
    }
    Err(ShardingError::Parse(format!(
        "hash shard sink requires numeric record key, got `{key}`"
    )))
}

fn json_value_to_sql_value(value: JsonValue) -> Value {
    match value {
        JsonValue::Null => Value::Json(None),
        JsonValue::Bool(value) => Value::from(value),
        JsonValue::Number(value) => {
            if let Some(value) = value.as_i64() {
                Value::from(value)
            } else if let Some(value) = value.as_u64() {
                Value::from(value)
            } else if let Some(value) = value.as_f64() {
                Value::from(value)
            } else {
                Value::from(JsonValue::Number(value))
            }
        }
        JsonValue::String(value) => Value::from(value),
        JsonValue::Array(_) | JsonValue::Object(_) => Value::from(value),
    }
}

fn split_qualified_table(table: &str) -> Result<(String, String)> {
    let (schema, relation) = table.rsplit_once('.').ok_or_else(|| {
        ShardingError::Config(format!(
            "postgres table sink requires schema-qualified table name, got `{table}`"
        ))
    })?;
    Ok((unquote_ident(schema), unquote_ident(relation)))
}

fn quote_qualified_table(table: &str) -> Result<String> {
    let (schema, relation) = split_qualified_table(table)?;
    Ok(format!(
        "{}.{}",
        quote_ident(&schema),
        quote_ident(&relation)
    ))
}

fn quote_ident(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn unquote_ident(value: &str) -> String {
    value.trim_matches('"').to_string()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sea_orm::{ConnectionTrait, Database, DbBackend, Statement};

    use crate::{
        CdcOperation, CdcRecord, PostgresTableSink,
        cdc::{CdcSink, test_support::LogicalReplicationTestDatabase},
    };

    #[tokio::test]
    #[ignore = "requires docker or SUMMER_SHARDING_CDC_E2E_DATABASE_URL"]
    async fn postgres_table_sink_upserts_and_deletes_rows() {
        let test_db = LogicalReplicationTestDatabase::start()
            .await
            .expect("start logical replication database");
        let connection = Arc::new(
            Database::connect(test_db.database_url())
                .await
                .expect("connect database"),
        );
        connection
            .execute_unprepared(
                r#"
                DROP TABLE IF EXISTS public.sink_probe;
                CREATE TABLE public.sink_probe (
                    id BIGINT PRIMARY KEY,
                    tenant_id VARCHAR(64) NOT NULL,
                    body JSONB NOT NULL
                );
                "#,
            )
            .await
            .expect("create sink probe");
        let sink = PostgresTableSink::new(connection.clone());

        sink.write_batch(&[CdcRecord {
            table: "public.sink_probe".to_string(),
            key: "1".to_string(),
            payload: serde_json::json!({"id":1,"tenant_id":"T-001","body":{"name":"alpha"}}),
            operation: CdcOperation::Snapshot,
            source_lsn: None,
        }])
        .await
        .expect("write batch");
        sink.apply_change(&CdcRecord {
            table: "public.sink_probe".to_string(),
            key: "1".to_string(),
            payload: serde_json::json!({"id":1,"tenant_id":"T-001","body":{"name":"beta"}}),
            operation: CdcOperation::Update,
            source_lsn: None,
        })
        .await
        .expect("update");
        sink.apply_change(&CdcRecord {
            table: "public.sink_probe".to_string(),
            key: "1".to_string(),
            payload: serde_json::json!({"id":1}),
            operation: CdcOperation::Delete,
            source_lsn: None,
        })
        .await
        .expect("delete");

        let row = connection
            .query_one_raw(Statement::from_string(
                DbBackend::Postgres,
                "SELECT COUNT(*) AS count FROM public.sink_probe",
            ))
            .await
            .expect("count rows")
            .expect("count row");
        let count: i64 = row.try_get("", "count").expect("count");
        assert_eq!(count, 0);
    }
}
