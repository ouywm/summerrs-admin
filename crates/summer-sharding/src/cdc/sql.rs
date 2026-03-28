use std::{collections::VecDeque, sync::Arc};

use async_trait::async_trait;
use parking_lot::Mutex;
use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, QueryResult, Statement};
use serde_json::Value as JsonValue;

use crate::{
    cdc::{
        CdcBatch, CdcCutover, CdcOperation, CdcRecord, CdcSink, CdcSource, CdcSubscribeRequest,
        CdcSubscription,
    },
    error::{Result, ShardingError},
};

#[derive(Debug, Clone)]
pub struct SqlStatementTemplate {
    pub snapshot_sql: String,
    pub catch_up_sql: String,
    pub ensure_slot_sql: Option<String>,
    pub ensure_publication_sql: Option<String>,
    pub supports_from_position: bool,
}

impl SqlStatementTemplate {
    fn render_snapshot(&self, table: &str, cursor: Option<&str>, limit: i64) -> String {
        let mut sql = self.snapshot_sql.clone();
        let offset = cursor
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or_default();
        sql = sql.replace("{table}", table);
        sql = sql.replace("{cursor}", cursor.unwrap_or_default());
        sql = sql.replace("{offset}", &offset.to_string());
        sql = sql.replace("{limit}", &limit.to_string());
        sql
    }

    fn render_catch_up(
        &self,
        tables: &str,
        slot: &str,
        publication: &str,
        position: Option<&str>,
        limit: usize,
    ) -> String {
        let mut sql = self.catch_up_sql.clone();
        sql = sql.replace("{tables}", tables);
        sql = sql.replace("{slot}", slot);
        sql = sql.replace("{publication}", publication);
        sql = sql.replace("{position}", position.unwrap_or("0/0"));
        sql = sql.replace("{limit}", &limit.max(1).to_string());
        sql
    }

    fn render_ensure_slot(&self, slot: &str) -> Option<String> {
        self.ensure_slot_sql
            .as_ref()
            .map(|sql| sql.replace("{slot}", slot))
    }

    fn render_ensure_publication(&self, publication: &str, tables: &str) -> Option<String> {
        self.ensure_publication_sql.as_ref().map(|sql| {
            sql.replace("{publication}", publication)
                .replace("{tables}", tables)
        })
    }

    pub fn build_snapshot(
        &self,
        backend: DbBackend,
        table: &str,
        cursor: Option<&str>,
        limit: i64,
    ) -> Statement {
        Statement::from_string(backend, self.render_snapshot(table, cursor, limit))
    }

    pub fn build_catch_up(
        &self,
        backend: DbBackend,
        tables: &str,
        slot: &str,
        publication: &str,
        position: Option<&str>,
        limit: usize,
    ) -> Statement {
        Statement::from_string(
            backend,
            self.render_catch_up(tables, slot, publication, position, limit),
        )
    }

    pub fn build_ensure_slot(&self, backend: DbBackend, slot: &str) -> Option<Statement> {
        self.render_ensure_slot(slot)
            .map(|sql| Statement::from_string(backend, sql))
    }

    pub fn build_ensure_publication(
        &self,
        backend: DbBackend,
        publication: &str,
        tables: &str,
    ) -> Option<Statement> {
        self.render_ensure_publication(publication, tables)
            .map(|sql| Statement::from_string(backend, sql))
    }

    fn snapshot_uses_offset_cursor(&self) -> bool {
        self.snapshot_sql.contains("{offset}") && !self.snapshot_sql.contains("{cursor}")
    }
}

pub struct SqlCdcSource {
    connection: Arc<DatabaseConnection>,
    template: SqlStatementTemplate,
    source_tables: Vec<String>,
    position: Arc<Mutex<Option<String>>>,
}

impl SqlCdcSource {
    pub fn new(
        connection: Arc<DatabaseConnection>,
        template: SqlStatementTemplate,
        source_tables: Vec<String>,
    ) -> Self {
        Self {
            connection,
            template,
            source_tables,
            position: Arc::new(Mutex::new(None)),
        }
    }

    pub fn position(&self) -> Option<String> {
        self.position.lock().clone()
    }

    fn backend(&self) -> DbBackend {
        self.connection.get_database_backend()
    }

    fn row_to_record(row: QueryResult) -> Result<CdcRecord> {
        let table: String = row.try_get("", "table")?;
        let key: String = row.try_get("", "key")?;
        let payload = row.try_get::<Option<String>>("", "payload")?;
        let payload = payload.map_or(Ok(JsonValue::Null), |value| {
            serde_json::from_str(&value).map_err(|err| ShardingError::Parse(err.to_string()))
        })?;
        let operation = row.try_get::<String>("", "operation")?;
        let operation = CdcOperation::from_name(operation.as_str()).ok_or_else(|| {
            ShardingError::Unsupported(format!("unsupported cdc operation {operation}"))
        })?;
        let source_lsn = row.try_get::<Option<String>>("", "source_lsn")?;
        Ok(CdcRecord {
            table,
            key,
            payload,
            operation,
            source_lsn,
        })
    }
}

struct SqlPollingSubscription {
    connection: Arc<DatabaseConnection>,
    template: SqlStatementTemplate,
    source_tables: Vec<String>,
    slot: String,
    publication: String,
    position: Arc<Mutex<Option<String>>>,
    pending_records: VecDeque<CdcRecord>,
}

impl SqlPollingSubscription {
    fn backend(&self) -> DbBackend {
        self.connection.get_database_backend()
    }
}

#[async_trait]
impl CdcSubscription for SqlPollingSubscription {
    async fn next_batch(&mut self, limit: usize) -> Result<CdcBatch> {
        let requested = limit.max(1);
        while self.pending_records.len() < requested {
            let tables = self.source_tables.join(",");
            let position = self.position.lock().clone();
            let stmt = self.template.build_catch_up(
                self.backend(),
                tables.as_str(),
                self.slot.as_str(),
                self.publication.as_str(),
                position.as_deref(),
                requested,
            );
            let rows = self.connection.query_all_raw(stmt).await?;
            let records = rows
                .into_iter()
                .map(SqlCdcSource::row_to_record)
                .collect::<Result<Vec<_>>>()?;
            let fetched_count = records.len();
            if records.is_empty() {
                break;
            }
            if let Some(lsn) = records.last().and_then(|record| record.source_lsn.clone()) {
                *self.position.lock() = Some(lsn);
            }
            self.pending_records.extend(records);
            if fetched_count < requested {
                break;
            }
        }

        let drain_count = requested.min(self.pending_records.len());
        let records = self
            .pending_records
            .drain(..drain_count)
            .collect::<Vec<_>>();
        Ok(CdcBatch {
            next_position: self.position(),
            records,
        })
    }

    fn position(&self) -> Option<String> {
        self.position.lock().clone()
    }
}

#[async_trait]
impl CdcSource for SqlCdcSource {
    async fn snapshot(&self, table: &str, cursor: Option<&str>, limit: i64) -> Result<CdcBatch> {
        let stmt = self
            .template
            .build_snapshot(self.backend(), table, cursor, limit);
        let rows = self.connection.query_all_raw(stmt).await?;
        let mut records = Vec::with_capacity(rows.len());
        let mut next_position = None;
        for row in rows {
            let snapshot_cursor = row
                .try_get::<Option<String>>("", "snapshot_cursor")
                .ok()
                .flatten();
            let record = Self::row_to_record(row)?;
            next_position = snapshot_cursor.or_else(|| {
                (!self.template.snapshot_uses_offset_cursor()).then(|| record.key.clone())
            });
            records.push(record);
        }
        if self.template.snapshot_uses_offset_cursor() && !records.is_empty() {
            let offset = cursor
                .and_then(|value| value.parse::<i64>().ok())
                .unwrap_or_default();
            next_position = Some((offset + records.len() as i64).to_string());
        }
        Ok(CdcBatch {
            records,
            next_position,
        })
    }

    async fn subscribe(&self, request: CdcSubscribeRequest) -> Result<Box<dyn CdcSubscription>> {
        let tables = if request.source_tables.is_empty() {
            self.source_tables.clone()
        } else {
            request.source_tables.clone()
        };
        if tables.is_empty() {
            return Err(ShardingError::Config(
                "cdc subscribe requires at least one source table".to_string(),
            ));
        }
        if request.from_position.is_some() && !self.template.supports_from_position {
            return Err(ShardingError::Unsupported(
                "this cdc source template does not support resuming from an explicit position"
                    .to_string(),
            ));
        }

        let tables_list = tables.join(",");
        if let Some(stmt) = self
            .template
            .build_ensure_slot(self.backend(), request.slot.as_str())
        {
            self.connection.execute_raw(stmt).await?;
        }
        if let Some(stmt) = self.template.build_ensure_publication(
            self.backend(),
            request.publication.as_str(),
            tables_list.as_str(),
        ) {
            self.connection.execute_raw(stmt).await?;
        }
        *self.position.lock() = request.from_position.clone();

        Ok(Box::new(SqlPollingSubscription {
            connection: self.connection.clone(),
            template: self.template.clone(),
            source_tables: tables,
            slot: request.slot,
            publication: request.publication,
            position: self.position.clone(),
            pending_records: VecDeque::new(),
        }))
    }
}

#[async_trait]
pub trait SqlCdcSinkBuilder: Send + Sync {
    fn write_statement(&self, record: &CdcRecord) -> Statement;
    fn apply_statement(&self, record: &CdcRecord) -> Statement;
}

pub struct SqlCdcSink {
    connection: Arc<DatabaseConnection>,
    builder: Arc<dyn SqlCdcSinkBuilder>,
}

impl SqlCdcSink {
    pub fn new(connection: Arc<DatabaseConnection>, builder: Arc<dyn SqlCdcSinkBuilder>) -> Self {
        Self {
            connection,
            builder,
        }
    }

    async fn execute_statement(&self, stmt: Statement) -> Result<()> {
        self.connection.execute_raw(stmt).await?;
        Ok(())
    }
}

#[async_trait]
impl CdcSink for SqlCdcSink {
    async fn write_batch(&self, records: &[CdcRecord]) -> Result<usize> {
        for record in records {
            self.execute_statement(self.builder.write_statement(record))
                .await?;
        }
        Ok(records.len())
    }

    async fn apply_change(&self, record: &CdcRecord) -> Result<()> {
        self.execute_statement(self.builder.apply_statement(record))
            .await?;
        Ok(())
    }
}

pub struct SqlCdcCutover {
    connection: Arc<DatabaseConnection>,
    statement: String,
}

impl SqlCdcCutover {
    pub fn new(connection: Arc<DatabaseConnection>, statement: String) -> Self {
        Self {
            connection,
            statement,
        }
    }
}

#[async_trait]
impl CdcCutover for SqlCdcCutover {
    async fn cutover(&self) -> Result<()> {
        self.connection
            .execute_unprepared(self.statement.as_str())
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cdc::CdcSubscribeRequest;
    use sea_orm::{DbBackend, MockDatabase, MockExecResult, Value};

    struct DummySinkBuilder;

    impl SqlCdcSinkBuilder for DummySinkBuilder {
        fn write_statement(&self, record: &CdcRecord) -> Statement {
            Statement::from_string(
                DbBackend::Postgres,
                format!("INSERT INTO sink_table VALUES ('{}')", record.key),
            )
        }

        fn apply_statement(&self, record: &CdcRecord) -> Statement {
            Statement::from_string(
                DbBackend::Postgres,
                format!(
                    "UPDATE sink_table SET payload = '{}' WHERE key = '{}'",
                    record.payload, record.key
                ),
            )
        }
    }

    #[tokio::test]
    async fn sql_cdc_source_reads_snapshot_and_subscribe_catch_up() {
        let snapshot_rows = vec![
            std::collections::BTreeMap::from([
                (
                    "table".to_string(),
                    Value::String(Some("ai.log".to_string())),
                ),
                ("key".to_string(), Value::String(Some("1".to_string()))),
                (
                    "payload".to_string(),
                    Value::String(Some(r#"{"name":"alpha"}"#.to_string())),
                ),
                (
                    "operation".to_string(),
                    Value::String(Some("snapshot".to_string())),
                ),
                (
                    "source_lsn".to_string(),
                    Value::String(Some("0/1".to_string())),
                ),
            ]),
            std::collections::BTreeMap::from([
                (
                    "table".to_string(),
                    Value::String(Some("ai.log".to_string())),
                ),
                ("key".to_string(), Value::String(Some("2".to_string()))),
                (
                    "payload".to_string(),
                    Value::String(Some(r#"{"name":"beta"}"#.to_string())),
                ),
                (
                    "operation".to_string(),
                    Value::String(Some("snapshot".to_string())),
                ),
                (
                    "source_lsn".to_string(),
                    Value::String(Some("0/2".to_string())),
                ),
            ]),
        ];
        let catchup_rows = vec![std::collections::BTreeMap::from([
            (
                "table".to_string(),
                Value::String(Some("ai.log".to_string())),
            ),
            ("key".to_string(), Value::String(Some("3".to_string()))),
            (
                "payload".to_string(),
                Value::String(Some(r#"{"name":"gamma"}"#.to_string())),
            ),
            (
                "operation".to_string(),
                Value::String(Some("insert".to_string())),
            ),
            (
                "source_lsn".to_string(),
                Value::String(Some("0/3".to_string())),
            ),
        ])];

        let conn = Arc::new(
            MockDatabase::new(DbBackend::Postgres)
                .append_exec_results(vec![
                    MockExecResult {
                        rows_affected: 0,
                        last_insert_id: 0,
                    };
                    2
                ])
                .append_query_results([snapshot_rows, catchup_rows])
                .into_connection(),
        );
        let template = SqlStatementTemplate {
            snapshot_sql: "SELECT * FROM {table} OFFSET {offset} LIMIT {limit}".to_string(),
            catch_up_sql:
                "SELECT * FROM {tables} WHERE lsn > '{position}' LIMIT {limit} /* {slot}/{publication} */"
                    .to_string(),
            ensure_slot_sql: Some(
                "SELECT pg_create_logical_replication_slot('{slot}', 'pgoutput')".to_string(),
            ),
            ensure_publication_sql: Some(
                "CREATE PUBLICATION {publication} FOR TABLE {tables}".to_string(),
            ),
            supports_from_position: true,
        };
        let source = SqlCdcSource::new(conn, template, vec!["ai.log".to_string()]);

        let snapshot = source.snapshot("ai.log", None, 10).await.unwrap();
        assert_eq!(snapshot.records.len(), 2);
        assert_eq!(snapshot.next_position.as_deref(), Some("2"));
        let mut subscription = source
            .subscribe(CdcSubscribeRequest {
                slot: "summer_cdc_slot".to_string(),
                publication: "summer_cdc_pub".to_string(),
                source_tables: vec!["ai.log".to_string()],
                from_position: Some("0/2".to_string()),
            })
            .await
            .unwrap();
        let catchup = subscription.next_batch(10).await.unwrap();
        assert_eq!(catchup.records.len(), 1);
        assert_eq!(source.position().as_deref(), Some("0/3"));
    }

    #[tokio::test]
    async fn sql_cdc_sink_writes_sql_statements() {
        let conn = Arc::new(
            MockDatabase::new(DbBackend::Postgres)
                .append_exec_results(vec![
                    MockExecResult {
                        rows_affected: 1,
                        last_insert_id: 0,
                    };
                    3
                ])
                .into_connection(),
        );
        let sink = SqlCdcSink::new(conn.clone(), Arc::new(DummySinkBuilder));
        let records = vec![CdcRecord {
            table: "ai.log".to_string(),
            key: "1".to_string(),
            payload: serde_json::json!({"name":"alpha"}),
            operation: CdcOperation::Insert,
            source_lsn: None,
        }];
        sink.write_batch(&records).await.unwrap();
        sink.apply_change(&records[0]).await.unwrap();

        let logs = conn.as_ref().clone().into_transaction_log();
        assert_eq!(logs.len(), 2);
    }

    #[tokio::test]
    async fn sql_cdc_cutover_executes_statement() {
        let conn = Arc::new(
            MockDatabase::new(DbBackend::Postgres)
                .append_exec_results(vec![MockExecResult {
                    rows_affected: 1,
                    last_insert_id: 0,
                }])
                .into_connection(),
        );
        let cutover = SqlCdcCutover::new(conn.clone(), "SELECT pg_switch_wal()".to_string());
        cutover.cutover().await.unwrap();
        let logs = conn.as_ref().clone().into_transaction_log();
        assert_eq!(logs.len(), 1);
    }
}
