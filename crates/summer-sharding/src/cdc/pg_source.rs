use std::{
    collections::{BTreeMap, VecDeque},
    sync::Arc,
    time::Duration,
};

use parking_lot::{Mutex, RwLock};
use pgwire_replication::{
    Lsn, PgWireError, ReplicationClient, ReplicationConfig, ReplicationEvent, TlsConfig,
};
use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, Statement};
use serde_json::Value as JsonValue;
use url::Url;

use crate::{
    cdc::{CdcBatch, CdcRecord, CdcSource, CdcSubscribeRequest, CdcSubscription, PgOutputDecoder},
    error::{Result, ShardingError},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PgSourcePosition {
    pub lsn: String,
}

impl Default for PgSourcePosition {
    fn default() -> Self {
        Self {
            lsn: "0/0".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
struct ReplicationEndpoint {
    host: String,
    port: u16,
    user: String,
    password: String,
    database: String,
    tls: TlsConfig,
}

#[derive(Debug, Clone)]
struct SnapshotKeyColumn {
    name: String,
    data_type: String,
}

#[derive(Debug, Clone)]
struct SnapshotKeyInfo {
    canonical_table: String,
    columns: Vec<SnapshotKeyColumn>,
}

impl ReplicationEndpoint {
    fn parse(database_url: &str) -> Result<Self> {
        let parsed = Url::parse(database_url)
            .map_err(|err| ShardingError::Config(format!("invalid replication url: {err}")))?;
        let host = parsed
            .host_str()
            .ok_or_else(|| ShardingError::Config("replication url missing host".to_string()))?
            .to_string();
        let user = parsed.username().to_string();
        if user.is_empty() {
            return Err(ShardingError::Config(
                "replication url missing username".to_string(),
            ));
        }
        let password = parsed.password().unwrap_or_default().to_string();
        let database = parsed
            .path_segments()
            .and_then(|mut segments| segments.next())
            .filter(|segment| !segment.is_empty())
            .ok_or_else(|| ShardingError::Config("replication url missing database".to_string()))?
            .to_string();
        let tls = parse_tls_config(&parsed)?;

        Ok(Self {
            host,
            port: parsed.port().unwrap_or(5432),
            user,
            password,
            database,
            tls,
        })
    }

    fn to_replication_config(
        &self,
        slot: &str,
        publication: &str,
        start_lsn: Lsn,
    ) -> ReplicationConfig {
        ReplicationConfig {
            host: self.host.clone(),
            port: self.port,
            user: self.user.clone(),
            password: self.password.clone(),
            database: self.database.clone(),
            tls: self.tls.clone(),
            slot: slot.to_string(),
            publication: publication.to_string(),
            start_lsn,
            stop_at_lsn: None,
            status_interval: Duration::from_secs(1),
            idle_wakeup_interval: Duration::from_secs(5),
            buffer_events: 1024,
        }
    }
}

#[derive(Debug)]
pub struct PgCdcSource {
    snapshot_connection: Arc<DatabaseConnection>,
    endpoint: ReplicationEndpoint,
    position: Arc<RwLock<PgSourcePosition>>,
    snapshot_keys: Mutex<std::collections::BTreeMap<String, SnapshotKeyInfo>>,
}

impl PgCdcSource {
    async fn connect_replication_client(
        &self,
        request: &CdcSubscribeRequest,
        start_lsn: Lsn,
    ) -> Result<ReplicationClient> {
        const RETRY_ATTEMPTS: usize = 20;
        const RETRY_DELAY_MS: u64 = 250;

        let config = self.endpoint.to_replication_config(
            request.slot.as_str(),
            request.publication.as_str(),
            start_lsn,
        );

        for attempt in 0..RETRY_ATTEMPTS {
            match ReplicationClient::connect(config.clone()).await {
                Ok(client) => return Ok(client),
                Err(error)
                    if attempt + 1 < RETRY_ATTEMPTS && replication_artifact_not_ready(&error) =>
                {
                    self.ensure_replication_slot(request.slot.as_str()).await?;
                    self.ensure_publication(request.publication.as_str(), &request.source_tables)
                        .await?;
                    tokio::time::sleep(Duration::from_millis(RETRY_DELAY_MS)).await;
                }
                Err(error) => return Err(pgwire_error(error)),
            }
        }

        unreachable!("replication client connect loop must return on success or terminal error");
    }

    pub fn new(
        snapshot_connection: Arc<DatabaseConnection>,
        replication_database_url: &str,
    ) -> Result<Self> {
        Ok(Self {
            snapshot_connection,
            endpoint: ReplicationEndpoint::parse(replication_database_url)?,
            position: Arc::new(RwLock::new(PgSourcePosition::default())),
            snapshot_keys: Mutex::new(std::collections::BTreeMap::new()),
        })
    }

    pub fn position(&self) -> PgSourcePosition {
        self.position.read().clone()
    }

    async fn ensure_replication_slot(&self, slot: &str) -> Result<()> {
        let exists = self
            .snapshot_connection
            .query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "SELECT slot_name FROM pg_replication_slots WHERE slot_name = $1",
                [slot.into()],
            ))
            .await?;
        if exists.is_none() {
            self.snapshot_connection
                .query_one_raw(Statement::from_sql_and_values(
                    DbBackend::Postgres,
                    "SELECT slot_name FROM pg_create_logical_replication_slot($1, 'pgoutput')",
                    [slot.into()],
                ))
                .await?;
        }
        self.wait_for_replication_slot(slot).await
    }

    async fn wait_for_replication_slot(&self, slot: &str) -> Result<()> {
        for _ in 0..20 {
            let exists = self
                .snapshot_connection
                .query_one_raw(Statement::from_sql_and_values(
                    DbBackend::Postgres,
                    "SELECT slot_name FROM pg_replication_slots WHERE slot_name = $1",
                    [slot.into()],
                ))
                .await?;
            if exists.is_some() {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        Err(ShardingError::Route(format!(
            "replication slot `{slot}` was not visible after creation"
        )))
    }

    async fn ensure_publication(&self, publication: &str, tables: &[String]) -> Result<()> {
        let exists = self
            .snapshot_connection
            .query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "SELECT pubname FROM pg_publication WHERE pubname = $1",
                [publication.into()],
            ))
            .await?;
        if exists.is_none() {
            let table_list = tables
                .iter()
                .map(|table| quote_qualified_table(table))
                .collect::<Result<Vec<_>>>()?
                .join(", ");
            self.snapshot_connection
                .execute_unprepared(
                    format!(
                        "CREATE PUBLICATION {} FOR TABLE {}",
                        quote_ident(publication),
                        table_list
                    )
                    .as_str(),
                )
                .await?;
        }
        self.wait_for_publication(publication).await
    }

    async fn wait_for_publication(&self, publication: &str) -> Result<()> {
        for _ in 0..20 {
            let exists = self
                .snapshot_connection
                .query_one_raw(Statement::from_sql_and_values(
                    DbBackend::Postgres,
                    "SELECT pubname FROM pg_publication WHERE pubname = $1",
                    [publication.into()],
                ))
                .await?;
            if exists.is_some() {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        Err(ShardingError::Route(format!(
            "publication `{publication}` was not visible after creation"
        )))
    }

    async fn primary_key_columns(&self, table: &str) -> Result<Vec<SnapshotKeyColumn>> {
        Ok(self.primary_key_info(table).await?.columns.clone())
    }

    async fn primary_key_info(&self, table: &str) -> Result<SnapshotKeyInfo> {
        {
            let guard = self.snapshot_keys.lock();
            if let Some(info) = guard.get(table).cloned() {
                return Ok(info);
            }
        }
        let info = self.load_primary_key_info(table).await?;
        self.snapshot_keys
            .lock()
            .insert(table.to_string(), info.clone());
        Ok(info)
    }

    async fn load_primary_key_info(&self, table: &str) -> Result<SnapshotKeyInfo> {
        let (schema, relation) = split_qualified_table(table)?;
        let rows = self
            .snapshot_connection
            .query_all_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                r#"
                SELECT n.nspname AS schema
                     , c.relname AS relation
                     , a.attname
                     , pg_catalog.format_type(a.atttypid, a.atttypmod) AS data_type
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
        let mut columns = Vec::new();
        let mut canonical_schema = None;
        let mut canonical_relation = None;
        for row in rows {
            if canonical_schema.is_none() {
                canonical_schema = Some(row.try_get::<String>("", "schema")?);
                canonical_relation = Some(row.try_get::<String>("", "relation")?);
            }
            columns.push(SnapshotKeyColumn {
                name: row.try_get::<String>("", "attname")?,
                data_type: row.try_get::<String>("", "data_type")?,
            });
        }
        if columns.is_empty() {
            return Err(ShardingError::Config(format!(
                "cdc snapshot requires primary key columns for `{table}`"
            )));
        }
        let canonical_schema = canonical_schema.expect("primary key should produce schema");
        let canonical_relation = canonical_relation.expect("primary key should produce relation");
        Ok(SnapshotKeyInfo {
            canonical_table: format!("{canonical_schema}.{canonical_relation}"),
            columns,
        })
    }

    async fn primary_key_map(&self, tables: &[String]) -> Result<BTreeMap<String, Vec<String>>> {
        let mut primary_keys = BTreeMap::new();
        for table in tables {
            let info = self.primary_key_info(table).await?;
            primary_keys.insert(
                info.canonical_table.clone(),
                info.columns.into_iter().map(|column| column.name).collect(),
            );
        }
        Ok(primary_keys)
    }
}

#[async_trait::async_trait]
impl CdcSource for PgCdcSource {
    async fn snapshot(&self, table: &str, cursor: Option<&str>, limit: i64) -> Result<CdcBatch> {
        let key_columns = self.primary_key_columns(table).await?;
        let quoted_table = quote_qualified_table(table)?;
        let key_expr = composite_key_expression("source", &key_columns);
        let order_expr = qualified_columns("source", &key_columns).join(", ");
        let snapshot_cursor_expr = snapshot_cursor_expression("source", &key_columns);
        let mut sql = format!(
            "SELECT '{}' AS \"table\", {key_expr} AS key, to_jsonb(source)::text AS payload, 'snapshot' AS operation, NULL::text AS source_lsn, {snapshot_cursor_expr} AS snapshot_cursor FROM {quoted_table} AS source",
            escape_literal(table),
        );
        if let Some(predicate) = snapshot_cursor_predicate("source", &key_columns, cursor)? {
            sql.push_str(" WHERE ");
            sql.push_str(predicate.as_str());
        }
        sql.push_str(format!(" ORDER BY {order_expr} LIMIT {limit}").as_str());
        let rows = self
            .snapshot_connection
            .query_all_raw(Statement::from_string(DbBackend::Postgres, sql))
            .await?;
        let mut records = Vec::with_capacity(rows.len());
        let mut next_position = None;
        for row in rows {
            let table: String = row.try_get("", "table")?;
            let key: String = row.try_get("", "key")?;
            let payload = row.try_get::<Option<String>>("", "payload")?;
            let payload = payload.map_or(Ok(serde_json::Value::Null), |value| {
                serde_json::from_str(&value).map_err(|err| ShardingError::Parse(err.to_string()))
            })?;
            let operation = row.try_get::<String>("", "operation")?;
            let operation =
                crate::cdc::CdcOperation::from_name(operation.as_str()).ok_or_else(|| {
                    ShardingError::Unsupported(format!("unsupported cdc operation {operation}"))
                })?;
            let source_lsn = row.try_get::<Option<String>>("", "source_lsn")?;
            next_position = row.try_get::<Option<String>>("", "snapshot_cursor")?;
            records.push(CdcRecord {
                table,
                key,
                payload,
                operation,
                source_lsn,
            });
        }
        Ok(CdcBatch {
            records,
            next_position,
        })
    }

    async fn subscribe(&self, request: CdcSubscribeRequest) -> Result<Box<dyn CdcSubscription>> {
        if request.source_tables.is_empty() {
            return Err(ShardingError::Config(
                "cdc subscribe requires at least one source table".to_string(),
            ));
        }

        self.ensure_replication_slot(request.slot.as_str()).await?;
        self.ensure_publication(request.publication.as_str(), &request.source_tables)
            .await?;
        let primary_keys = self.primary_key_map(&request.source_tables).await?;

        let start_lsn = if let Some(position) = request.from_position.as_deref() {
            position
                .parse::<Lsn>()
                .map_err(|err| ShardingError::Parse(err.to_string()))?
        } else {
            Lsn::ZERO
        };

        let client = self.connect_replication_client(&request, start_lsn).await?;

        Ok(Box::new(PgReplicationSubscription {
            client,
            decoder: PgOutputDecoder::with_primary_keys(&request.source_tables, primary_keys),
            pending_records: VecDeque::new(),
            position: new_subscription_position(start_lsn),
        }))
    }
}

struct PgReplicationSubscription {
    client: ReplicationClient,
    decoder: PgOutputDecoder,
    pending_records: VecDeque<CdcRecord>,
    position: Arc<RwLock<PgSourcePosition>>,
}

fn new_subscription_position(start_lsn: Lsn) -> Arc<RwLock<PgSourcePosition>> {
    Arc::new(RwLock::new(PgSourcePosition {
        lsn: start_lsn.to_string(),
    }))
}

impl Drop for PgReplicationSubscription {
    fn drop(&mut self) {
        self.client.stop();
    }
}

#[async_trait::async_trait]
impl CdcSubscription for PgReplicationSubscription {
    async fn next_batch(&mut self, limit: usize) -> Result<CdcBatch> {
        let requested = limit.max(1);

        while self.pending_records.len() < requested {
            let recv_result =
                tokio::time::timeout(Duration::from_millis(500), self.client.recv()).await;
            let event = match recv_result {
                Ok(result) => match result.map_err(pgwire_error)? {
                    Some(event) => event,
                    None => break,
                },
                Err(_) => break,
            };
            match event {
                ReplicationEvent::XLogData { wal_end, data, .. } => {
                    self.pending_records
                        .extend(self.decoder.decode_chunk(&data, wal_end)?);
                }
                ReplicationEvent::Commit { end_lsn, .. }
                | ReplicationEvent::StoppedAt { reached: end_lsn } => {
                    self.position.write().lsn = end_lsn.to_string();
                    if self.pending_records.len() >= requested {
                        break;
                    }
                }
                ReplicationEvent::Message { lsn, .. } => {
                    self.position.write().lsn = lsn.to_string();
                }
                ReplicationEvent::KeepAlive { .. } | ReplicationEvent::Begin { .. } => {}
            }
        }

        let drain_count = requested.min(self.pending_records.len());
        let records = self
            .pending_records
            .drain(..drain_count)
            .collect::<Vec<_>>();
        if let Some(last_lsn) = records
            .last()
            .and_then(|record| record.source_lsn.as_deref())
            .and_then(|lsn| lsn.parse::<Lsn>().ok())
        {
            self.client.update_applied_lsn(last_lsn);
            self.position.write().lsn = last_lsn.to_string();
        }

        Ok(CdcBatch {
            next_position: self.position(),
            records,
        })
    }

    fn position(&self) -> Option<String> {
        Some(self.position.read().lsn.clone())
    }
}

fn composite_key_expression(alias: &str, columns: &[SnapshotKeyColumn]) -> String {
    let qualified = qualified_columns(alias, columns);
    if qualified.len() == 1 {
        format!("{}::text", qualified[0])
    } else {
        format!(
            "concat_ws(':', {})",
            qualified
                .into_iter()
                .map(|column| format!("{column}::text"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

fn qualified_columns(alias: &str, columns: &[SnapshotKeyColumn]) -> Vec<String> {
    columns
        .iter()
        .map(|column| format!("{alias}.{}", quote_ident(column.name.as_str())))
        .collect()
}

fn snapshot_cursor_expression(alias: &str, columns: &[SnapshotKeyColumn]) -> String {
    format!(
        "jsonb_build_array({})::text",
        qualified_columns(alias, columns).join(", ")
    )
}

fn snapshot_cursor_predicate(
    alias: &str,
    columns: &[SnapshotKeyColumn],
    cursor: Option<&str>,
) -> Result<Option<String>> {
    let Some(cursor) = cursor else {
        return Ok(None);
    };
    let values = serde_json::from_str::<Vec<JsonValue>>(cursor).map_err(|err| {
        ShardingError::Parse(format!("invalid snapshot cursor `{cursor}`: {err}"))
    })?;
    if values.len() != columns.len() {
        return Err(ShardingError::Parse(format!(
            "snapshot cursor column count mismatch for `{cursor}`"
        )));
    }

    if columns.len() == 1 {
        return Ok(Some(format!(
            "{} > {}",
            qualified_columns(alias, columns)[0],
            typed_sql_literal(&values[0], columns[0].data_type.as_str())?
        )));
    }

    let lhs = qualified_columns(alias, columns).join(", ");
    let rhs = values
        .iter()
        .zip(columns)
        .map(|(value, column)| typed_sql_literal(value, column.data_type.as_str()))
        .collect::<Result<Vec<_>>>()?
        .join(", ");
    Ok(Some(format!("ROW({lhs}) > ROW({rhs})")))
}

fn typed_sql_literal(value: &JsonValue, data_type: &str) -> Result<String> {
    let literal = match value {
        JsonValue::Null => "NULL".to_string(),
        JsonValue::Bool(value) => {
            if *value {
                "TRUE".to_string()
            } else {
                "FALSE".to_string()
            }
        }
        JsonValue::Number(value) => value.to_string(),
        JsonValue::String(value) => format!("'{}'", escape_literal(value)),
        JsonValue::Array(_) | JsonValue::Object(_) => {
            return Err(ShardingError::Parse(format!(
                "snapshot cursor does not support compound primary-key value `{value}`"
            )));
        }
    };
    Ok(format!("{literal}::{data_type}"))
}

fn parse_tls_config(url: &Url) -> Result<TlsConfig> {
    let Some(mode) = url
        .query_pairs()
        .find(|(key, _)| key.eq_ignore_ascii_case("sslmode"))
        .map(|(_, value)| value.to_ascii_lowercase())
    else {
        return Ok(TlsConfig::disabled());
    };

    let tls = match mode.as_str() {
        "disable" => TlsConfig::disabled(),
        "prefer" => TlsConfig {
            mode: pgwire_replication::SslMode::Prefer,
            ..Default::default()
        },
        "require" => TlsConfig::require(),
        "verify-ca" => TlsConfig::verify_ca(None),
        "verify-full" => TlsConfig::verify_full(None),
        other => {
            return Err(ShardingError::Config(format!(
                "unsupported replication sslmode `{other}`"
            )));
        }
    };
    Ok(tls)
}

fn split_qualified_table(table: &str) -> Result<(String, String)> {
    table
        .split_once('.')
        .map(|(schema, relation)| (schema.to_string(), relation.to_string()))
        .ok_or_else(|| {
            ShardingError::Config(format!("qualified table name required, got `{table}`"))
        })
}

fn quote_qualified_table(table: &str) -> Result<String> {
    let (schema, relation) = split_qualified_table(table)?;
    Ok(format!(
        "{}.{}",
        quote_ident(schema.as_str()),
        quote_ident(relation.as_str())
    ))
}

fn quote_ident(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn escape_literal(value: &str) -> String {
    value.replace('\'', "''")
}

fn pgwire_error(error: PgWireError) -> ShardingError {
    ShardingError::Db(sea_orm::DbErr::Custom(error.to_string()))
}

fn replication_artifact_not_ready(error: &PgWireError) -> bool {
    let message = error.to_string();
    message.contains("does not exist")
        && (message.contains("publication") || message.contains("replication slot"))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::Utc;
    use pgwire_replication::Lsn;
    use rand::random;
    use sea_orm::{ConnectionTrait, Database};

    use crate::cdc::{
        CdcOperation, CdcSource, CdcSubscribeRequest, PgCdcSource,
        test_support::LogicalReplicationTestDatabase,
    };

    #[test]
    fn subscription_positions_are_isolated_per_subscriber() {
        let first = super::new_subscription_position(Lsn::from(1_u64));
        let second = super::new_subscription_position(Lsn::from(2_u64));

        assert!(!Arc::ptr_eq(&first, &second));
        first.write().lsn = "0/99".to_string();

        assert_eq!(first.read().lsn, "0/99");
        assert_eq!(second.read().lsn, "0/2");
    }

    #[tokio::test]
    #[ignore = "requires docker or SUMMER_SHARDING_CDC_E2E_DATABASE_URL"]
    async fn pg_cdc_source_streams_real_pgoutput_changes_from_dedicated_instance() {
        let test_db = LogicalReplicationTestDatabase::start()
            .await
            .expect("start logical replication test db");
        let connection = Arc::new(
            Database::connect(test_db.database_url())
                .await
                .expect("connect database"),
        );
        let suffix =
            Utc::now().timestamp_micros().unsigned_abs() * 1000 + u64::from(random::<u16>());
        let table = format!("cdc_probe_src_{suffix}");
        let full_table = format!("public.{table}");
        let slot = format!("summer_cdc_slot_{suffix}");
        let publication = format!("summer_cdc_pub_{suffix}");

        connection
            .execute_unprepared(
                format!(
                    "CREATE TABLE {full_table} (
                        id BIGINT PRIMARY KEY,
                        tenant_id TEXT NOT NULL,
                        payload JSONB NOT NULL,
                        create_time TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
                    );
                    ALTER TABLE {full_table} REPLICA IDENTITY FULL;"
                )
                .as_str(),
            )
            .await
            .expect("create source table");

        let source =
            PgCdcSource::new(connection.clone(), test_db.database_url()).expect("build pg source");

        connection
            .execute_unprepared(
                format!(
                    "INSERT INTO {full_table}(id, tenant_id, payload) VALUES
                        (1, 'T-001', '{{\"name\":\"alpha\"}}'::jsonb),
                        (2, 'T-001', '{{\"name\":\"beta\"}}'::jsonb);"
                )
                .as_str(),
            )
            .await
            .expect("seed snapshot rows");

        let snapshot = source
            .snapshot(full_table.as_str(), None, 10)
            .await
            .expect("snapshot");
        assert_eq!(snapshot.records.len(), 2);
        assert!(snapshot.next_position.is_some());

        let mut subscription = source
            .subscribe(CdcSubscribeRequest {
                slot: slot.clone(),
                publication: publication.clone(),
                source_tables: vec![full_table.clone()],
                from_position: None,
            })
            .await
            .expect("subscribe");

        connection
            .execute_unprepared(
                format!(
                    "INSERT INTO {full_table}(id, tenant_id, payload) VALUES (3, 'T-001', '{{\"name\":\"gamma\"}}'::jsonb);
                     UPDATE {full_table} SET payload = '{{\"name\":\"beta-2\"}}'::jsonb WHERE id = 2;
                     DELETE FROM {full_table} WHERE id = 1;"
                )
                .as_str(),
            )
            .await
            .expect("apply changes");

        let batch = subscription.next_batch(3).await.expect("next batch");
        assert_eq!(batch.records.len(), 3);
        assert_eq!(
            batch
                .records
                .iter()
                .map(|record| record.operation.clone())
                .collect::<Vec<_>>(),
            vec![
                CdcOperation::Insert,
                CdcOperation::Update,
                CdcOperation::Delete
            ]
        );
        assert_eq!(
            batch
                .records
                .iter()
                .map(|record| record.key.as_str())
                .collect::<Vec<_>>(),
            vec!["3", "2", "1"]
        );

        drop(subscription);
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        connection
            .execute_unprepared(
                format!(
                    "DROP PUBLICATION IF EXISTS {};
                     SELECT pg_drop_replication_slot('{}')
                     FROM pg_replication_slots
                     WHERE slot_name = '{}';
                     DROP TABLE IF EXISTS {full_table};",
                    super::quote_ident(publication.as_str()),
                    super::escape_literal(slot.as_str()),
                    super::escape_literal(slot.as_str()),
                )
                .as_str(),
            )
            .await
            .expect("cleanup");
    }
}
