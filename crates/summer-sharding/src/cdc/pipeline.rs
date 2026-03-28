use async_trait::async_trait;

use crate::{
    cdc::{RowFilter, RowTransform},
    error::Result,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CdcOperation {
    Snapshot,
    Insert,
    Update,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CdcRecord {
    pub table: String,
    pub key: String,
    pub payload: serde_json::Value,
    pub operation: CdcOperation,
    pub source_lsn: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CdcTask {
    pub name: String,
    pub source_tables: Vec<String>,
    pub source_filter: Option<String>,
    pub batch_size: i64,
    pub slot: Option<String>,
    pub publication: Option<String>,
    pub start_position: Option<String>,
    pub max_catch_up_polls: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CdcRunReport {
    pub snapshot_written: usize,
    pub catch_up_written: usize,
    pub last_position: Option<String>,
    pub cutover_complete: bool,
    pub phases: Vec<CdcPhase>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CdcPhase {
    Snapshot,
    CatchUp,
    CutOver,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CdcSubscribeRequest {
    pub slot: String,
    pub publication: String,
    pub source_tables: Vec<String>,
    pub from_position: Option<String>,
}

impl CdcSubscribeRequest {
    pub fn from_task(task: &CdcTask) -> Self {
        Self {
            slot: task
                .slot
                .clone()
                .unwrap_or_else(|| format!("{}_slot", task.name)),
            publication: task
                .publication
                .clone()
                .unwrap_or_else(|| format!("{}_pub", task.name)),
            source_tables: task.source_tables.clone(),
            from_position: task.start_position.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CdcBatch {
    pub records: Vec<CdcRecord>,
    pub next_position: Option<String>,
}

impl CdcOperation {
    pub fn from_name(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "snapshot" => Some(CdcOperation::Snapshot),
            "insert" => Some(CdcOperation::Insert),
            "update" => Some(CdcOperation::Update),
            "delete" => Some(CdcOperation::Delete),
            _ => None,
        }
    }
}

#[async_trait]
pub trait CdcCutover: Send + Sync + 'static {
    async fn cutover(&self) -> Result<()>;
}

#[async_trait]
pub trait CdcSource: Send + Sync + 'static {
    async fn snapshot(&self, table: &str, cursor: Option<&str>, limit: i64) -> Result<CdcBatch>;
    async fn subscribe(&self, request: CdcSubscribeRequest) -> Result<Box<dyn CdcSubscription>>;
}

#[async_trait]
pub trait CdcSubscription: Send + Sync {
    async fn next_batch(&mut self, limit: usize) -> Result<CdcBatch>;
    fn position(&self) -> Option<String>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CdcSinkKind {
    DirectTable,
    HashSharded,
    ClickHouse,
}

#[async_trait]
pub trait CdcSink: Send + Sync + 'static {
    async fn write_batch(&self, records: &[CdcRecord]) -> Result<usize>;
    async fn apply_change(&self, record: &CdcRecord) -> Result<()>;

    fn kind(&self) -> CdcSinkKind {
        CdcSinkKind::DirectTable
    }

    fn descriptor(&self) -> &'static str {
        std::any::type_name::<Self>()
    }
}

#[derive(Debug, Clone, Default)]
pub struct CdcPipeline;

impl CdcPipeline {
    pub async fn run(
        &self,
        task: &CdcTask,
        source: &dyn CdcSource,
        transformer: &dyn RowTransform,
        sink: &dyn CdcSink,
        cutover: Option<&dyn CdcCutover>,
    ) -> Result<CdcRunReport> {
        let mut snapshot_written = 0usize;
        let mut catch_up_written = 0usize;
        let mut last_position = task.start_position.clone();
        let mut phases = Vec::new();
        let row_filter = task
            .source_filter
            .as_deref()
            .map(RowFilter::parse)
            .transpose()?;
        let mut subscription = source
            .subscribe(CdcSubscribeRequest::from_task(task))
            .await?;

        phases.push(CdcPhase::Snapshot);

        for table in &task.source_tables {
            let mut cursor = None::<String>;
            loop {
                let batch = source
                    .snapshot(table.as_str(), cursor.as_deref(), task.batch_size)
                    .await?;
                let next_cursor = batch.next_position.clone();
                let records = filter_records(batch.records, row_filter.as_ref())?;
                if records.is_empty() && next_cursor.is_none() {
                    break;
                }
                let transformed = records
                    .into_iter()
                    .map(|record| transformer.transform(record))
                    .collect::<Result<Vec<_>>>()?;
                snapshot_written += sink.write_batch(transformed.as_slice()).await?;
                cursor = next_cursor;
                if cursor.is_none() {
                    break;
                }
            }
        }

        for _ in 0..task.max_catch_up_polls.max(1) {
            let batch = subscription
                .next_batch(task.batch_size.max(1) as usize)
                .await?;
            let saw_source_records = !batch.records.is_empty();
            let records = filter_records(batch.records, row_filter.as_ref())?;
            if records.is_empty() {
                last_position = batch.next_position.or_else(|| subscription.position());
                if saw_source_records {
                    continue;
                }
                break;
            }
            let transformed = records
                .into_iter()
                .map(|record| transformer.transform(record))
                .collect::<Result<Vec<_>>>()?;
            for record in &transformed {
                sink.apply_change(record).await?;
            }
            catch_up_written += transformed.len();
            last_position = batch.next_position.or_else(|| subscription.position());
        }

        phases.push(CdcPhase::CatchUp);
        let cutover_complete = cutover.is_some();
        if let Some(cutover) = cutover {
            phases.push(CdcPhase::CutOver);
            cutover.cutover().await?;
        }

        Ok(CdcRunReport {
            snapshot_written,
            catch_up_written,
            last_position,
            cutover_complete,
            phases,
        })
    }
}

fn filter_records(
    records: Vec<CdcRecord>,
    row_filter: Option<&RowFilter>,
) -> Result<Vec<CdcRecord>> {
    let Some(row_filter) = row_filter else {
        return Ok(records);
    };
    records
        .into_iter()
        .filter_map(|record| match row_filter.matches(&record) {
            Ok(true) => Some(Ok(record)),
            Ok(false) => None,
            Err(error) => Some(Err(error)),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use chrono::Utc;
    use rand::random;
    use sea_orm::{ConnectionTrait, Database};

    use crate::cdc::{
        CdcBatch, CdcCutover, CdcOperation, CdcPhase, CdcPipeline, CdcRecord, CdcSource,
        CdcSubscribeRequest, CdcSubscription, CdcTask, InMemoryCdcSource, PgCdcSource,
        RowTransform, RowTransformer, TableSink, test_support::LogicalReplicationTestDatabase,
    };
    use crate::error::Result;

    #[derive(Debug, Default)]
    struct FlagCutover {
        called: Arc<Mutex<bool>>,
    }

    impl FlagCutover {
        fn new() -> Self {
            Self {
                called: Arc::new(Mutex::new(false)),
            }
        }

        fn was_called(&self) -> bool {
            *self.called.lock().unwrap()
        }
    }

    #[async_trait]
    impl CdcCutover for FlagCutover {
        async fn cutover(&self) -> Result<()> {
            let mut guard = self.called.lock().unwrap();
            *guard = true;
            Ok(())
        }
    }

    #[derive(Debug, Default, Clone)]
    struct RecordingSource {
        calls: Arc<Mutex<Vec<&'static str>>>,
    }

    #[derive(Debug, Default)]
    struct DriftingSnapshotSource {
        snapshot_calls: Arc<Mutex<usize>>,
    }

    #[derive(Debug, Default)]
    struct RecordingSubscription {
        calls: Arc<Mutex<Vec<&'static str>>>,
    }

    #[derive(Debug, Default)]
    struct FilteredCatchUpSource;

    #[derive(Debug, Default)]
    struct FilteredCatchUpSubscription {
        call_index: usize,
    }

    #[async_trait]
    impl CdcSource for RecordingSource {
        async fn snapshot(
            &self,
            _table: &str,
            _cursor: Option<&str>,
            _limit: i64,
        ) -> Result<CdcBatch> {
            self.calls.lock().unwrap().push("snapshot");
            Ok(CdcBatch::default())
        }

        async fn subscribe(
            &self,
            _request: CdcSubscribeRequest,
        ) -> Result<Box<dyn CdcSubscription>> {
            self.calls.lock().unwrap().push("subscribe");
            Ok(Box::new(RecordingSubscription {
                calls: self.calls.clone(),
            }))
        }
    }

    #[async_trait]
    impl CdcSource for DriftingSnapshotSource {
        async fn snapshot(
            &self,
            _table: &str,
            cursor: Option<&str>,
            limit: i64,
        ) -> Result<CdcBatch> {
            let mut snapshot_calls = self.snapshot_calls.lock().unwrap();
            *snapshot_calls += 1;

            let visible_rows = if *snapshot_calls == 1 {
                vec![
                    CdcRecord {
                        table: "ai.log".to_string(),
                        key: "1".to_string(),
                        payload: serde_json::json!({"id":1}),
                        operation: CdcOperation::Snapshot,
                        source_lsn: None,
                    },
                    CdcRecord {
                        table: "ai.log".to_string(),
                        key: "2".to_string(),
                        payload: serde_json::json!({"id":2}),
                        operation: CdcOperation::Snapshot,
                        source_lsn: None,
                    },
                ]
            } else {
                vec![
                    CdcRecord {
                        table: "ai.log".to_string(),
                        key: "2".to_string(),
                        payload: serde_json::json!({"id":2}),
                        operation: CdcOperation::Snapshot,
                        source_lsn: None,
                    },
                    CdcRecord {
                        table: "ai.log".to_string(),
                        key: "3".to_string(),
                        payload: serde_json::json!({"id":3}),
                        operation: CdcOperation::Snapshot,
                        source_lsn: None,
                    },
                ]
            };

            let records = visible_rows
                .into_iter()
                .skip_while(|record| {
                    cursor
                        .map(|value| record.key.as_str() <= value)
                        .unwrap_or(false)
                })
                .take(limit.max(0) as usize)
                .collect::<Vec<_>>();
            let next_position = records.last().map(|record| record.key.clone());

            Ok(CdcBatch {
                records,
                next_position,
            })
        }

        async fn subscribe(
            &self,
            _request: CdcSubscribeRequest,
        ) -> Result<Box<dyn CdcSubscription>> {
            Ok(Box::new(RecordingSubscription::default()))
        }
    }

    #[async_trait]
    impl CdcSubscription for RecordingSubscription {
        async fn next_batch(&mut self, _limit: usize) -> Result<CdcBatch> {
            self.calls.lock().unwrap().push("next_batch");
            Ok(CdcBatch::default())
        }

        fn position(&self) -> Option<String> {
            None
        }
    }

    #[async_trait]
    impl CdcSource for FilteredCatchUpSource {
        async fn snapshot(
            &self,
            _table: &str,
            _cursor: Option<&str>,
            _limit: i64,
        ) -> Result<CdcBatch> {
            Ok(CdcBatch::default())
        }

        async fn subscribe(
            &self,
            _request: CdcSubscribeRequest,
        ) -> Result<Box<dyn CdcSubscription>> {
            Ok(Box::new(FilteredCatchUpSubscription::default()))
        }
    }

    #[async_trait]
    impl CdcSubscription for FilteredCatchUpSubscription {
        async fn next_batch(&mut self, _limit: usize) -> Result<CdcBatch> {
            self.call_index += 1;
            let (records, next_position) = match self.call_index {
                1 => (
                    vec![CdcRecord {
                        table: "ai.log".to_string(),
                        key: "1".to_string(),
                        payload: serde_json::json!({"tenant_id":"T-OTHER","name":"ignored"}),
                        operation: CdcOperation::Insert,
                        source_lsn: Some("0/1".to_string()),
                    }],
                    Some("0/1".to_string()),
                ),
                2 => (
                    vec![CdcRecord {
                        table: "ai.log".to_string(),
                        key: "2".to_string(),
                        payload: serde_json::json!({"tenant_id":"T-KEEP","name":"kept"}),
                        operation: CdcOperation::Insert,
                        source_lsn: Some("0/2".to_string()),
                    }],
                    Some("0/2".to_string()),
                ),
                _ => (Vec::new(), Some(format!("0/{}", self.call_index))),
            };
            Ok(CdcBatch {
                records,
                next_position,
            })
        }

        fn position(&self) -> Option<String> {
            Some(format!("0/{}", self.call_index))
        }
    }

    #[derive(Debug, Default)]
    struct PassthroughTransform;

    impl RowTransform for PassthroughTransform {
        fn transform(&self, row: CdcRecord) -> Result<CdcRecord> {
            Ok(row)
        }
    }

    #[tokio::test]
    async fn cdc_pipeline_runs_snapshot_and_delta() {
        let source = InMemoryCdcSource::new()
            .with_replication("expand_log_slot", "expand_log_pub")
            .with_snapshot(
                "ai.log",
                vec![
                    CdcRecord {
                        table: "ai.log".to_string(),
                        key: "1".to_string(),
                        payload: serde_json::json!({"name":"alpha"}),
                        operation: CdcOperation::Snapshot,
                        source_lsn: Some("0/1".to_string()),
                    },
                    CdcRecord {
                        table: "ai.log".to_string(),
                        key: "2".to_string(),
                        payload: serde_json::json!({"name":"beta"}),
                        operation: CdcOperation::Snapshot,
                        source_lsn: Some("0/2".to_string()),
                    },
                ],
            )
            .with_change(CdcRecord {
                table: "ai.log".to_string(),
                key: "3".to_string(),
                payload: serde_json::json!({"name":"gamma"}),
                operation: CdcOperation::Insert,
                source_lsn: Some("0/3".to_string()),
            });
        let sink = TableSink::default();
        let report = CdcPipeline
            .run(
                &CdcTask {
                    name: "expand_log".to_string(),
                    source_tables: vec!["ai.log".to_string()],
                    source_filter: None,
                    batch_size: 1,
                    slot: Some("expand_log_slot".to_string()),
                    publication: Some("expand_log_pub".to_string()),
                    start_position: None,
                    max_catch_up_polls: 8,
                },
                &source,
                &RowTransformer,
                &sink,
                None,
            )
            .await
            .expect("pipeline");
        assert!((2..=3).contains(&report.snapshot_written));
        assert_eq!(report.catch_up_written, 1);
        assert_eq!(report.last_position.as_deref(), Some("0/3"));
        assert_eq!(report.cutover_complete, false);
        assert_eq!(report.phases, vec![CdcPhase::Snapshot, CdcPhase::CatchUp]);
        assert_eq!(sink.rows().len(), 3);
    }

    #[tokio::test]
    async fn cdc_pipeline_invokes_cutover_when_provided() {
        let source = InMemoryCdcSource::new()
            .with_replication("cutover_slot", "cutover_pub")
            .with_snapshot("ai.log", vec![])
            .with_change(CdcRecord {
                table: "ai.log".to_string(),
                key: "1".to_string(),
                payload: serde_json::json!({"name":"delta"}),
                operation: CdcOperation::Insert,
                source_lsn: Some("0/5".to_string()),
            });
        let sink = TableSink::default();
        let cutover = FlagCutover::new();
        let report = CdcPipeline
            .run(
                &CdcTask {
                    name: "cutover_test".to_string(),
                    source_tables: vec!["ai.log".to_string()],
                    source_filter: None,
                    batch_size: 10,
                    slot: Some("cutover_slot".to_string()),
                    publication: Some("cutover_pub".to_string()),
                    start_position: None,
                    max_catch_up_polls: 8,
                },
                &source,
                &RowTransformer,
                &sink,
                Some(&cutover),
            )
            .await
            .expect("pipeline with cutover");

        assert!(cutover.was_called());
        assert!(report.cutover_complete);
        assert_eq!(
            report.phases,
            vec![CdcPhase::Snapshot, CdcPhase::CatchUp, CdcPhase::CutOver]
        );
    }

    #[tokio::test]
    async fn cdc_pipeline_subscribes_before_snapshot_to_avoid_lost_changes() {
        let source = RecordingSource::default();
        let sink = TableSink::default();

        let report = CdcPipeline
            .run(
                &CdcTask {
                    name: "order_test".to_string(),
                    source_tables: vec!["ai.log".to_string()],
                    source_filter: None,
                    batch_size: 1,
                    slot: Some("order_slot".to_string()),
                    publication: Some("order_pub".to_string()),
                    start_position: None,
                    max_catch_up_polls: 1,
                },
                &source,
                &PassthroughTransform,
                &sink,
                None,
            )
            .await
            .expect("pipeline");

        assert!(!report.cutover_complete);
        assert_eq!(
            source.calls.lock().unwrap().clone(),
            vec!["subscribe", "snapshot", "next_batch"]
        );
    }

    #[tokio::test]
    #[ignore = "requires docker or SUMMER_SHARDING_CDC_E2E_DATABASE_URL"]
    async fn cdc_pipeline_runs_against_real_logical_slot_source() {
        let test_db = LogicalReplicationTestDatabase::start()
            .await
            .expect("start logical replication test db");
        let database_url = test_db.database_url().to_string();
        let connection = Arc::new(Database::connect(&database_url).await.expect("connect"));
        let writer = Database::connect(&database_url).await.expect("writer");
        let suffix =
            Utc::now().timestamp_micros().unsigned_abs() * 1000 + u64::from(random::<u16>());
        let table = format!("cdc_pipeline_src_{suffix}");
        let full_table = format!("public.{table}");
        let slot = format!("summer_cdc_pipeline_slot_{suffix}");

        connection
            .execute_unprepared(
                format!(
                    "DROP TABLE IF EXISTS {full_table};
                     CREATE TABLE {full_table} (
                         id BIGINT PRIMARY KEY,
                         tenant_id TEXT NOT NULL,
                         payload JSONB NOT NULL,
                         create_time TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
                     );
                     ALTER TABLE {full_table} REPLICA IDENTITY FULL;
                     INSERT INTO {full_table}(id, tenant_id, payload) VALUES
                        (1, 'T-001', '{{\"name\":\"alpha\"}}'::jsonb),
                        (2, 'T-001', '{{\"name\":\"beta\"}}'::jsonb);"
                )
                .as_str(),
            )
            .await
            .expect("prepare source table");

        let source = PgCdcSource::new(connection.clone(), database_url.as_str()).expect("source");
        let sink = TableSink::default();
        let mutation_table = full_table.clone();
        let mutation_task = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            writer
                .execute_unprepared(
                    format!(
                        "INSERT INTO {mutation_table}(id, tenant_id, payload) VALUES (3, 'T-001', '{{\"name\":\"gamma\"}}'::jsonb);
                         UPDATE {mutation_table} SET payload = '{{\"name\":\"beta-2\"}}'::jsonb WHERE id = 2;
                         DELETE FROM {mutation_table} WHERE id = 1;"
                    )
                    .as_str(),
                )
                .await
                .expect("apply delta changes");
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
            writer
                .execute_unprepared(
                    format!(
                        "INSERT INTO {mutation_table}(id, tenant_id, payload) VALUES (4, 'T-001', '{{\"name\":\"delta\"}}'::jsonb);"
                    )
                    .as_str(),
                )
                .await
                .expect("apply catch-up only change");
        });

        let report = CdcPipeline
            .run(
                &CdcTask {
                    name: format!("pipeline_{suffix}"),
                    source_tables: vec![full_table.clone()],
                    source_filter: None,
                    batch_size: 1,
                    slot: Some(slot.clone()),
                    publication: Some(format!("{slot}_pub")),
                    start_position: None,
                    max_catch_up_polls: 8,
                },
                &source,
                &RowTransformer,
                &sink,
                None,
            )
            .await
            .expect("pipeline");
        mutation_task.await.expect("mutation join");

        assert!((2..=3).contains(&report.snapshot_written));
        assert!(report.catch_up_written >= 1);
        assert!(report.last_position.is_some());

        let rows = sink.rows();
        assert_eq!(rows.len(), 3, "rows: {rows:?}");
        assert!(
            rows.iter()
                .any(|row| row.key == "2" && row.payload["payload"]["name"] == "beta-2"),
            "rows: {rows:?}"
        );
        assert!(
            rows.iter()
                .any(|row| row.key == "3" && row.payload["payload"]["name"] == "gamma"),
            "rows: {rows:?}"
        );
        assert!(
            rows.iter()
                .any(|row| row.key == "4" && row.payload["payload"]["name"] == "delta"),
            "rows: {rows:?}"
        );
        assert!(rows.iter().all(|row| row.key != "1"), "rows: {rows:?}");

        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        connection
            .execute_unprepared(
                format!(
                    "DROP PUBLICATION IF EXISTS \"{slot}_pub\";
                     DO $$ BEGIN
                       IF EXISTS (SELECT 1 FROM pg_replication_slots WHERE slot_name = '{slot}') THEN
                         PERFORM pg_drop_replication_slot('{slot}');
                       END IF;
                     END $$;
                     DROP TABLE IF EXISTS {full_table};"
                )
                .as_str(),
            )
            .await
            .expect("cleanup");
    }

    #[tokio::test]
    async fn cdc_pipeline_snapshot_pagination_must_not_skip_rows_when_snapshot_view_shifts() {
        let source = DriftingSnapshotSource::default();
        let sink = TableSink::default();

        let report = CdcPipeline
            .run(
                &CdcTask {
                    name: "drift".to_string(),
                    source_tables: vec!["ai.log".to_string()],
                    source_filter: None,
                    batch_size: 2,
                    slot: Some("drift_slot".to_string()),
                    publication: Some("drift_pub".to_string()),
                    start_position: None,
                    max_catch_up_polls: 1,
                },
                &source,
                &PassthroughTransform,
                &sink,
                None,
            )
            .await
            .expect("pipeline");

        let mut ids = sink
            .rows()
            .into_iter()
            .map(|row| row.payload["id"].as_i64().expect("id"))
            .collect::<Vec<_>>();
        ids.sort();

        assert_eq!(ids, vec![1, 2, 3]);
        assert_eq!(report.snapshot_written, 3);
    }

    #[tokio::test]
    async fn cdc_pipeline_filter_does_not_stop_catch_up_on_non_matching_batch() {
        let source = FilteredCatchUpSource;
        let sink = TableSink::default();

        let report = CdcPipeline
            .run(
                &CdcTask {
                    name: "filtered".to_string(),
                    source_tables: vec!["ai.log".to_string()],
                    source_filter: Some("tenant_id = 'T-KEEP'".to_string()),
                    batch_size: 1,
                    slot: Some("filtered_slot".to_string()),
                    publication: Some("filtered_pub".to_string()),
                    start_position: None,
                    max_catch_up_polls: 4,
                },
                &source,
                &PassthroughTransform,
                &sink,
                None,
            )
            .await
            .expect("pipeline");

        assert_eq!(report.catch_up_written, 1);
        assert_eq!(report.last_position.as_deref(), Some("0/3"));
        let rows = sink.rows();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].key, "2");
        assert_eq!(rows[0].payload["tenant_id"], "T-KEEP");
    }
}
