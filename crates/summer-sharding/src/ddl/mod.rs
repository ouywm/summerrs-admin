mod ghost;
mod scheduler;

use std::collections::BTreeMap;

use async_trait::async_trait;
use futures::future::try_join_all;
use parking_lot::RwLock;
use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, Statement, TransactionTrait};

use crate::cdc::{CdcSink, CdcSource, CdcSubscribeRequest, RowTransform};
use crate::error::{Result, ShardingError};

use self::ghost::ghost_table_names;

pub use ghost::{GhostTablePlan, GhostTablePlanner};
pub use scheduler::DdlScheduler;

pub type DdlTaskId = u64;
const DDL_CATCH_UP_MAX_POLLS: usize = 64;
const DDL_FINAL_DRAIN_IDLE_ROUNDS: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DdlTaskStatus {
    Pending,
    Snapshot,
    CatchUp,
    CutOver,
    Cleanup,
    Done,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OnlineDdlTask {
    pub ddl: String,
    pub actual_tables: Vec<String>,
    pub concurrency: usize,
    pub batch_size: usize,
    pub status: DdlTaskStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DdlShardPlan {
    pub table: String,
    pub snapshot_statements: Vec<String>,
    pub catch_up_statements: Vec<String>,
    pub cutover_statements: Vec<String>,
    pub cleanup_statements: Vec<String>,
}

impl DdlShardPlan {
    pub fn statement_count(&self) -> usize {
        self.snapshot_statements.len()
            + self.catch_up_statements.len()
            + self.cutover_statements.len()
            + self.cleanup_statements.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DdlProgress {
    pub id: DdlTaskId,
    pub status: DdlTaskStatus,
    pub total_tables: usize,
    pub completed_tables: usize,
    pub batch_size: usize,
    pub shard_plans: Vec<DdlShardPlan>,
    pub scheduled_batches: Vec<Vec<String>>,
    pub phase_history: Vec<DdlTaskStatus>,
}

#[async_trait]
pub trait OnlineDdlEngine: Send + Sync + 'static {
    async fn submit(&self, task: OnlineDdlTask) -> Result<DdlTaskId>;
    async fn progress(&self, id: DdlTaskId) -> Result<DdlProgress>;
    async fn cancel(&self, id: DdlTaskId) -> Result<()>;
}

#[derive(Debug, Default)]
pub struct InMemoryOnlineDdlEngine {
    planner: GhostTablePlanner,
    scheduler: DdlScheduler,
    tasks: RwLock<BTreeMap<DdlTaskId, DdlProgress>>,
    next_id: RwLock<DdlTaskId>,
}

impl InMemoryOnlineDdlEngine {
    pub fn new() -> Self {
        Self::default()
    }

    fn update_status(&self, id: DdlTaskId, status: DdlTaskStatus) -> Result<()> {
        let mut tasks = self.tasks.write();
        let progress = tasks
            .get_mut(&id)
            .ok_or_else(|| ShardingError::Route(format!("online ddl task `{id}` not found")))?;
        progress.status = status.clone();
        progress.phase_history.push(status);
        Ok(())
    }

    fn update_completed_tables(&self, id: DdlTaskId, completed_tables: usize) -> Result<()> {
        let mut tasks = self.tasks.write();
        let progress = tasks
            .get_mut(&id)
            .ok_or_else(|| ShardingError::Route(format!("online ddl task `{id}` not found")))?;
        progress.completed_tables = completed_tables;
        Ok(())
    }

    fn progress_snapshot(&self, id: DdlTaskId) -> Result<DdlProgress> {
        self.tasks
            .read()
            .get(&id)
            .cloned()
            .ok_or_else(|| ShardingError::Route(format!("online ddl task `{id}` not found")))
    }

    fn scheduled_shard_batches(&self, progress: &DdlProgress) -> Result<Vec<Vec<DdlShardPlan>>> {
        let plans_by_table = progress
            .shard_plans
            .iter()
            .cloned()
            .map(|plan| (plan.table.clone(), plan))
            .collect::<BTreeMap<_, _>>();

        progress
            .scheduled_batches
            .iter()
            .map(|batch| {
                batch
                    .iter()
                    .map(|table| {
                        plans_by_table.get(table).cloned().ok_or_else(|| {
                            ShardingError::Route(format!(
                                "online ddl scheduled table `{table}` missing shard plan"
                            ))
                        })
                    })
                    .collect()
            })
            .collect()
    }

    async fn execute_batch_statements(
        &self,
        connection: &DatabaseConnection,
        batch: &[DdlShardPlan],
        statements: fn(&DdlShardPlan) -> &Vec<String>,
    ) -> Result<()> {
        try_join_all(batch.iter().cloned().map(|plan| {
            let connection = connection.clone();
            async move {
                for statement in statements(&plan) {
                    connection.execute_unprepared(statement).await?;
                }
                Ok::<(), ShardingError>(())
            }
        }))
        .await?;
        Ok(())
    }

    async fn execute_snapshot_batch(
        &self,
        connection: &DatabaseConnection,
        batch: &[DdlShardPlan],
        batch_size: usize,
    ) -> Result<()> {
        try_join_all(batch.iter().cloned().map(|plan| {
            let connection = connection.clone();
            async move { Self::execute_snapshot_plan(&connection, &plan, batch_size).await }
        }))
        .await?;
        Ok(())
    }

    async fn execute_replication_setup_batch(
        &self,
        connection: &DatabaseConnection,
        batch: &[DdlShardPlan],
    ) -> Result<()> {
        self.execute_batch_statements(connection, batch, |plan| &plan.catch_up_statements)
            .await
    }

    async fn execute_snapshot_plan(
        connection: &DatabaseConnection,
        plan: &DdlShardPlan,
        batch_size: usize,
    ) -> Result<()> {
        for statement in &plan.snapshot_statements {
            if statement.contains(":start") && statement.contains(":end") {
                Self::execute_snapshot_copy_template(
                    connection,
                    plan.table.as_str(),
                    statement,
                    batch_size,
                )
                .await?;
            } else {
                connection.execute_unprepared(statement).await?;
            }
        }
        Ok(())
    }

    async fn execute_snapshot_copy_template(
        connection: &DatabaseConnection,
        table: &str,
        statement: &str,
        batch_size: usize,
    ) -> Result<()> {
        let Some((min_id, max_id)) = Self::table_id_bounds(connection, table).await? else {
            return Ok(());
        };

        let step = batch_size.max(1) as i64;
        let mut start = min_id;
        while start <= max_id {
            let end = (start + step - 1).min(max_id);
            let start_token = start.to_string();
            let end_token = end.to_string();
            let sql = statement
                .replace(":start", start_token.as_str())
                .replace(":end", end_token.as_str());
            connection.execute_unprepared(sql.as_str()).await?;
            start += step;
        }
        Ok(())
    }

    async fn table_id_bounds(
        connection: &DatabaseConnection,
        table: &str,
    ) -> Result<Option<(i64, i64)>> {
        let row = connection
            .query_one_raw(Statement::from_string(
                DbBackend::Postgres,
                format!("SELECT MIN(id) AS min_id, MAX(id) AS max_id FROM {table}"),
            ))
            .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let min_id = row.try_get::<Option<i64>>("", "min_id")?;
        let max_id = row.try_get::<Option<i64>>("", "max_id")?;
        Ok(min_id.zip(max_id))
    }

    async fn execute_catch_up_batch(
        &self,
        batch: &[DdlShardPlan],
        batch_size: usize,
        cdc: Option<(&dyn CdcSource, &dyn RowTransform, &dyn CdcSink)>,
    ) -> Result<BTreeMap<String, Option<String>>> {
        let Some((source, transformer, sink)) = cdc else {
            return Ok(BTreeMap::new());
        };

        Ok(try_join_all(batch.iter().cloned().map(|plan| async move {
            let last_position = Self::drain_cdc_changes(
                source,
                transformer,
                sink,
                plan.table.as_str(),
                batch_size,
                None,
                DDL_CATCH_UP_MAX_POLLS,
                1,
            )
            .await?;
            Ok::<(String, Option<String>), ShardingError>((plan.table, last_position))
        }))
        .await?
        .into_iter()
        .collect())
    }

    async fn execute_cutover_batch(
        &self,
        connection: &DatabaseConnection,
        batch: &[DdlShardPlan],
        batch_size: usize,
        cdc: Option<(&dyn CdcSource, &dyn RowTransform, &dyn CdcSink)>,
        resume_positions: &BTreeMap<String, Option<String>>,
    ) -> Result<()> {
        try_join_all(batch.iter().cloned().map(|plan| {
            let connection = connection.clone();
            let start_position = resume_positions.get(plan.table.as_str()).cloned().flatten();
            async move {
                let txn = connection.begin().await?;
                let table = plan.table.clone();
                let (lock_statement, rename_statements) =
                    split_cutover_statements(&plan.cutover_statements);
                if let Some(lock_statement) = lock_statement.as_deref() {
                    txn.execute_unprepared(lock_statement).await?;
                }
                if let Some((source, transformer, sink)) = cdc {
                    Self::drain_cdc_changes(
                        source,
                        transformer,
                        sink,
                        table.as_str(),
                        batch_size,
                        start_position,
                        DDL_CATCH_UP_MAX_POLLS,
                        DDL_FINAL_DRAIN_IDLE_ROUNDS,
                    )
                    .await?;
                }
                for statement in &rename_statements {
                    txn.execute_unprepared(statement).await?;
                }
                txn.commit().await.map_err(ShardingError::from)
            }
        }))
        .await?;
        Ok(())
    }

    async fn drain_cdc_changes(
        source: &dyn CdcSource,
        transformer: &dyn RowTransform,
        sink: &dyn CdcSink,
        table: &str,
        batch_size: usize,
        from_position: Option<String>,
        max_polls: usize,
        idle_rounds_to_stop: usize,
    ) -> Result<Option<String>> {
        let names = ghost_table_names(table);
        let mut subscription = source
            .subscribe(CdcSubscribeRequest {
                slot: names.slot,
                publication: names.publication,
                source_tables: vec![table.to_string()],
                from_position,
            })
            .await?;
        let mut last_position = subscription.position();
        let mut idle_rounds = 0usize;

        for _ in 0..max_polls.max(1) {
            let batch = subscription.next_batch(batch_size.max(1)).await?;
            if batch.records.is_empty() {
                idle_rounds += 1;
                last_position = batch.next_position.or_else(|| subscription.position());
                if idle_rounds >= idle_rounds_to_stop.max(1) {
                    break;
                }
                continue;
            }

            idle_rounds = 0;
            for record in batch.records {
                let transformed = transformer.transform(record)?;
                sink.apply_change(&transformed).await?;
            }
            last_position = batch.next_position.or_else(|| subscription.position());
        }

        Ok(last_position)
    }

    pub async fn execute_task(&self, connection: &DatabaseConnection, id: DdlTaskId) -> Result<()> {
        self.execute_internal(connection, id, None).await
    }

    pub async fn execute_task_with_cdc(
        &self,
        connection: &DatabaseConnection,
        id: DdlTaskId,
        source: &dyn CdcSource,
        transformer: &dyn RowTransform,
        sink: &dyn CdcSink,
        _slot: &str,
        _publication: &str,
    ) -> Result<()> {
        self.execute_internal(connection, id, Some((source, transformer, sink)))
        .await
    }

    async fn execute_internal(
        &self,
        connection: &DatabaseConnection,
        id: DdlTaskId,
        cdc: Option<(&dyn CdcSource, &dyn RowTransform, &dyn CdcSink)>,
    ) -> Result<()> {
        let progress = self.progress_snapshot(id)?;
        let scheduled_batches = self.scheduled_shard_batches(&progress)?;

        self.update_status(id, DdlTaskStatus::Snapshot)?;
        for batch in &scheduled_batches {
            self.execute_replication_setup_batch(connection, batch).await?;
            self.execute_snapshot_batch(connection, batch, progress.batch_size)
                .await?;
        }

        self.update_status(id, DdlTaskStatus::CatchUp)?;
        let mut catch_up_positions = BTreeMap::new();
        for batch in &scheduled_batches {
            catch_up_positions.extend(
                self.execute_catch_up_batch(batch, progress.batch_size, cdc)
                    .await?,
            );
        }

        self.update_status(id, DdlTaskStatus::CutOver)?;
        for batch in &scheduled_batches {
            self.execute_cutover_batch(
                connection,
                batch,
                progress.batch_size,
                cdc,
                &catch_up_positions,
            )
            .await?;
        }

        self.update_status(id, DdlTaskStatus::Cleanup)?;
        let mut completed_tables = 0usize;
        for batch in &scheduled_batches {
            self.execute_batch_statements(connection, batch, |plan| &plan.cleanup_statements)
                .await?;
            completed_tables += batch.len();
            self.update_completed_tables(id, completed_tables)?;
        }

        self.update_status(id, DdlTaskStatus::Done)?;
        Ok(())
    }
}

#[async_trait]
impl OnlineDdlEngine for InMemoryOnlineDdlEngine {
    async fn submit(&self, task: OnlineDdlTask) -> Result<DdlTaskId> {
        let mut next_id = self.next_id.write();
        *next_id += 1;
        let id = *next_id;

        let shard_plans = task
            .actual_tables
            .iter()
            .cloned()
            .map(|table| {
                let staged_plan =
                    self.planner
                        .plan_staged(table.as_str(), task.ddl.as_str(), task.batch_size);
                DdlShardPlan {
                    table: table.clone(),
                    snapshot_statements: staged_plan.snapshot_statements,
                    catch_up_statements: staged_plan.catch_up_statements,
                    cutover_statements: staged_plan.cutover_statements,
                    cleanup_statements: staged_plan.cleanup_statements,
                }
            })
            .collect::<Vec<_>>();
        let scheduled_batches = self
            .scheduler
            .schedule_batches(task.actual_tables.as_slice(), task.concurrency.max(1));

        self.tasks.write().insert(
            id,
            DdlProgress {
                id,
                status: if shard_plans.is_empty() {
                    DdlTaskStatus::Done
                } else {
                    DdlTaskStatus::Pending
                },
                total_tables: shard_plans.len(),
                completed_tables: 0,
                batch_size: task.batch_size,
                shard_plans,
                scheduled_batches,
                phase_history: vec![DdlTaskStatus::Pending],
            },
        );
        Ok(id)
    }

    async fn progress(&self, id: DdlTaskId) -> Result<DdlProgress> {
        self.tasks
            .read()
            .get(&id)
            .cloned()
            .ok_or_else(|| ShardingError::Route(format!("online ddl task `{id}` not found")))
    }

    async fn cancel(&self, id: DdlTaskId) -> Result<()> {
        let mut tasks = self.tasks.write();
        let task = tasks
            .get_mut(&id)
            .ok_or_else(|| ShardingError::Route(format!("online ddl task `{id}` not found")))?;
        task.status = DdlTaskStatus::Cancelled;
        task.phase_history.push(DdlTaskStatus::Cancelled);
        Ok(())
    }
}

fn split_cutover_statements(statements: &[String]) -> (Option<String>, Vec<String>) {
    let Some(first) = statements.first() else {
        return (None, Vec::new());
    };
    if first
        .trim_start()
        .to_ascii_uppercase()
        .starts_with("LOCK TABLE ")
    {
        return (Some(first.clone()), statements.iter().skip(1).cloned().collect());
    }
    (None, statements.to_vec())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use crate::ddl::{DdlTaskStatus, InMemoryOnlineDdlEngine, OnlineDdlEngine, OnlineDdlTask};
    use crate::{
        CdcOperation, CdcRecord, InMemoryCdcSource, PgCdcSource, PostgresTableSink, TableSink,
        cdc::{CdcBatch, CdcSource, CdcSubscribeRequest, CdcSubscription, RowTransformer, test_support::LogicalReplicationTestDatabase},
    };
    use sea_orm::{
        ConnectionTrait, Database, DbBackend, MockDatabase, MockExecResult, QueryResult, Statement,
    };

    #[derive(Debug, Default)]
    struct LateCatchUpSource {
        subscribe_calls: Arc<Mutex<usize>>,
    }

    #[derive(Debug)]
    struct LateCatchUpSubscription {
        call_index: usize,
    }

    #[async_trait]
    impl CdcSubscription for LateCatchUpSubscription {
        async fn next_batch(&mut self, _limit: usize) -> crate::Result<CdcBatch> {
            self.call_index += 1;
            let records = match self.call_index {
                1 => Vec::new(),
                2 => vec![CdcRecord {
                    table: "ai.log_202603".to_string(),
                    key: "9".to_string(),
                    payload: serde_json::json!({"name":"late"}),
                    operation: CdcOperation::Insert,
                    source_lsn: Some("0/9".to_string()),
                }],
                _ => Vec::new(),
            };
            Ok(CdcBatch {
                records,
                next_position: Some(format!("0/{}", self.call_index)),
            })
        }

        fn position(&self) -> Option<String> {
            Some(format!("0/{}", self.call_index))
        }
    }

    #[async_trait]
    impl CdcSource for LateCatchUpSource {
        async fn snapshot(
            &self,
            _table: &str,
            _cursor: Option<&str>,
            _limit: i64,
        ) -> crate::Result<CdcBatch> {
            Ok(CdcBatch::default())
        }

        async fn subscribe(
            &self,
            _request: CdcSubscribeRequest,
        ) -> crate::Result<Box<dyn CdcSubscription>> {
            let mut subscribe_calls = self.subscribe_calls.lock().unwrap();
            *subscribe_calls += 1;
            Ok(Box::new(LateCatchUpSubscription {
                call_index: (*subscribe_calls).saturating_sub(1),
            }))
        }
    }

    #[tokio::test]
    async fn online_ddl_engine_tracks_task_progress() {
        let engine = InMemoryOnlineDdlEngine::new();
        let id = engine
            .submit(OnlineDdlTask {
                ddl: "ALTER TABLE ai.log ADD COLUMN extra text".to_string(),
                actual_tables: vec!["ai.log_202603".to_string(), "ai.log_202604".to_string()],
                concurrency: 1,
                batch_size: 1000,
                status: DdlTaskStatus::Pending,
            })
            .await
            .expect("submit");

        let progress = engine.progress(id).await.expect("progress");
        assert_eq!(progress.total_tables, 2);
        assert_eq!(progress.status, DdlTaskStatus::Pending);

        engine.cancel(id).await.expect("cancel");
        assert_eq!(
            engine.progress(id).await.expect("progress").status,
            DdlTaskStatus::Cancelled
        );
    }

    #[tokio::test]
    async fn online_ddl_engine_executes_plan_statements() {
        let engine = InMemoryOnlineDdlEngine::new();
        let id = engine
            .submit(OnlineDdlTask {
                ddl: "ALTER TABLE ai.log ADD COLUMN extra text".to_string(),
                actual_tables: vec!["ai.log_202603".to_string(), "ai.log_202604".to_string()],
                concurrency: 1,
                batch_size: 1000,
                status: DdlTaskStatus::Pending,
            })
            .await
            .expect("submit");

        let statement_count = {
            let tasks = engine.tasks.read();
            tasks
                .get(&id)
                .expect("task")
                .shard_plans
                .iter()
                .map(|plan| plan.statement_count())
                .sum::<usize>()
        };

        let connection = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([
                [BTreeMap::from([
                    ("min_id".to_string(), 1_i64.into()),
                    ("max_id".to_string(), 1_i64.into()),
                ])],
                [BTreeMap::from([
                    ("min_id".to_string(), 1_i64.into()),
                    ("max_id".to_string(), 1_i64.into()),
                ])],
            ])
            .append_exec_results(vec![
                MockExecResult {
                    rows_affected: 1,
                    last_insert_id: 0,
                };
                statement_count
            ])
            .into_connection();
        let log_connection = connection.clone();

        engine.execute_task(&connection, id).await.expect("execute");

        let logs = log_connection.into_transaction_log();
        let logged_statements = logs.iter().map(|item| item.statements().len()).sum::<usize>();
        assert!(logged_statements >= statement_count);
    }

    #[tokio::test]
    async fn online_ddl_engine_transitions_snapshot_catchup_cutover_cleanup() {
        let engine = InMemoryOnlineDdlEngine::new();
        let id = engine
            .submit(OnlineDdlTask {
                ddl: "ALTER TABLE ai.log ADD COLUMN extra text".to_string(),
                actual_tables: vec!["ai.log_202603".to_string()],
                concurrency: 1,
                batch_size: 1000,
                status: DdlTaskStatus::Pending,
            })
            .await
            .expect("submit");

        let source = InMemoryCdcSource::new()
            .with_replication("ai_log_202603_ddl_slot", "ai_log_202603_ddl_pub")
            .with_change(CdcRecord {
                table: "ai.log_202603".to_string(),
                key: "1".to_string(),
                payload: serde_json::json!({"k":"v"}),
                operation: CdcOperation::Insert,
                source_lsn: Some("0/9".to_string()),
            });
        let sink = TableSink::default();
        let connection = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([[BTreeMap::from([
                ("min_id".to_string(), 1_i64.into()),
                ("max_id".to_string(), 1_i64.into()),
            ])]])
            .append_exec_results(vec![
                MockExecResult {
                    rows_affected: 1,
                    last_insert_id: 0,
                };
                16
            ])
            .into_connection();

        engine
            .execute_task_with_cdc(
                &connection,
                id,
                &source,
                &RowTransformer,
                &sink,
                "ddl_slot",
                "ddl_pub",
            )
            .await
            .expect("execute");

        let progress = engine.progress(id).await.expect("progress");
        assert_eq!(progress.status, DdlTaskStatus::Done);
        assert_eq!(progress.completed_tables, 1);
        assert_eq!(
            progress.phase_history,
            vec![
                DdlTaskStatus::Pending,
                DdlTaskStatus::Snapshot,
                DdlTaskStatus::CatchUp,
                DdlTaskStatus::CutOver,
                DdlTaskStatus::Cleanup,
                DdlTaskStatus::Done,
            ]
        );
    }

    #[tokio::test]
    async fn online_ddl_engine_uses_scheduler_batches_for_execution() {
        let engine = InMemoryOnlineDdlEngine::new();
        let id = engine
            .submit(OnlineDdlTask {
                ddl: "ALTER TABLE ai.log ADD COLUMN extra text".to_string(),
                actual_tables: vec![
                    "ai.log_202603".to_string(),
                    "ai.log_202604".to_string(),
                    "ai.log_202605".to_string(),
                ],
                concurrency: 2,
                batch_size: 1000,
                status: DdlTaskStatus::Pending,
            })
            .await
            .expect("submit");

        let progress = engine.progress(id).await.expect("progress");
        let batches = engine
            .scheduled_shard_batches(&progress)
            .expect("scheduled shard batches");
        let batch_tables = batches
            .into_iter()
            .map(|batch| batch.into_iter().map(|plan| plan.table).collect::<Vec<_>>())
            .collect::<Vec<_>>();

        assert_eq!(
            batch_tables,
            vec![
                vec!["ai.log_202603".to_string(), "ai.log_202604".to_string()],
                vec!["ai.log_202605".to_string()],
            ]
        );
    }

    #[tokio::test]
    async fn online_ddl_engine_replays_final_catch_up_before_cutover() {
        let engine = InMemoryOnlineDdlEngine::new();
        let id = engine
            .submit(OnlineDdlTask {
                ddl: "ALTER TABLE ai.log ADD COLUMN extra text".to_string(),
                actual_tables: vec!["ai.log_202603".to_string()],
                concurrency: 1,
                batch_size: 1000,
                status: DdlTaskStatus::Pending,
            })
            .await
            .expect("submit");

        let source = LateCatchUpSource::default();
        let sink = TableSink::default();
        let connection = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([[BTreeMap::from([
                ("min_id".to_string(), sea_orm::Value::BigInt(None)),
                ("max_id".to_string(), sea_orm::Value::BigInt(None)),
            ])]])
            .append_exec_results(vec![
                MockExecResult {
                    rows_affected: 1,
                    last_insert_id: 0,
                };
                16
            ])
            .into_connection();

        engine
            .execute_task_with_cdc(
                &connection,
                id,
                &source,
                &RowTransformer,
                &sink,
                "ddl_slot",
                "ddl_pub",
            )
            .await
            .expect("execute");

        let rows = sink.rows();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].key, "9");
    }

    #[tokio::test]
    #[ignore = "requires docker or SUMMER_SHARDING_CDC_E2E_DATABASE_URL"]
    async fn online_ddl_engine_executes_real_pg_ghost_cutover_and_cleanup() {
        let test_db = LogicalReplicationTestDatabase::start()
            .await
            .expect("start logical replication database");
        let database_url = test_db.database_url().to_string();
        let connection = Database::connect(&database_url)
            .await
            .expect("connect logical replication database");

        connection
            .execute_unprepared(
                r#"
                DROP TABLE IF EXISTS public.ddl_probe_0 CASCADE;
                DROP TABLE IF EXISTS public.ddl_probe_1 CASCADE;
                DROP TABLE IF EXISTS public.ddl_probe_0__ghost CASCADE;
                DROP TABLE IF EXISTS public.ddl_probe_1__ghost CASCADE;
                DROP TABLE IF EXISTS public.ddl_probe_0__old CASCADE;
                DROP TABLE IF EXISTS public.ddl_probe_1__old CASCADE;

                CREATE TABLE public.ddl_probe_0 (
                    id BIGINT PRIMARY KEY,
                    tenant_id VARCHAR(64) NOT NULL,
                    body JSONB NOT NULL,
                    create_time TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
                );

                CREATE TABLE public.ddl_probe_1 (
                    id BIGINT PRIMARY KEY,
                    tenant_id VARCHAR(64) NOT NULL,
                    body JSONB NOT NULL,
                    create_time TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
                );

                INSERT INTO public.ddl_probe_0(id, tenant_id, body)
                VALUES
                    (1, 'T-001', '{"name":"alpha"}'::jsonb),
                    (2, 'T-001', '{"name":"beta"}'::jsonb);

                INSERT INTO public.ddl_probe_1(id, tenant_id, body)
                VALUES
                    (11, 'T-002', '{"name":"delta"}'::jsonb),
                    (12, 'T-002', '{"name":"echo"}'::jsonb);
                "#,
            )
            .await
            .expect("seed ddl probe tables");

        let source = PgCdcSource::new(std::sync::Arc::new(connection.clone()), &database_url)
            .expect("build pg cdc source");
        let sink = PostgresTableSink::with_table_map(
            std::sync::Arc::new(connection.clone()),
            [
                ("public.ddl_probe_0".to_string(), "public.ddl_probe_0__ghost".to_string()),
                ("public.ddl_probe_1".to_string(), "public.ddl_probe_1__ghost".to_string()),
            ],
        );
        let engine = std::sync::Arc::new(InMemoryOnlineDdlEngine::new());
        let task_id = engine
            .submit(OnlineDdlTask {
                ddl: "ALTER TABLE public.ddl_probe ADD COLUMN extra TEXT NOT NULL DEFAULT ''"
                    .to_string(),
                actual_tables: vec![
                    "public.ddl_probe_0".to_string(),
                    "public.ddl_probe_1".to_string(),
                ],
                concurrency: 2,
                batch_size: 1,
                status: DdlTaskStatus::Pending,
            })
            .await
            .expect("submit ddl task");

        let execute = {
            let engine = engine.clone();
            let connection = connection.clone();
            async move {
                engine
                    .execute_task_with_cdc(
                        &connection,
                        task_id,
                        &source,
                        &RowTransformer,
                        &sink,
                        "summer_online_ddl_slot",
                        "summer_online_ddl_pub",
                    )
                    .await
            }
        };

        let execute_handle = tokio::spawn(execute);
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        connection
            .execute_unprepared(
                r#"
                INSERT INTO public.ddl_probe_0(id, tenant_id, body)
                VALUES (3, 'T-001', '{"name":"gamma"}'::jsonb);
                UPDATE public.ddl_probe_1
                SET body = '{"name":"echo-2"}'::jsonb
                WHERE id = 12;
                DELETE FROM public.ddl_probe_0
                WHERE id = 1;
                "#,
            )
            .await
            .expect("apply incremental source changes");

        execute_handle
            .await
            .expect("join ddl task")
            .expect("execute ddl task");

        let probe_0_rows = connection
            .query_all_raw(Statement::from_string(
                DbBackend::Postgres,
                "SELECT id, tenant_id, body::text AS body, extra FROM public.ddl_probe_0 ORDER BY id",
            ))
            .await
            .expect("query ddl_probe_0");
        let probe_1_rows = connection
            .query_all_raw(Statement::from_string(
                DbBackend::Postgres,
                "SELECT id, tenant_id, body::text AS body, extra FROM public.ddl_probe_1 ORDER BY id",
            ))
            .await
            .expect("query ddl_probe_1");

        assert_eq!(probe_0_rows.len(), 2);
        assert_eq!(probe_1_rows.len(), 2);
        assert_eq!(
            probe_0_rows
                .iter()
                .map(row_body_name)
                .collect::<Vec<_>>(),
            vec!["beta".to_string(), "gamma".to_string()]
        );
        assert_eq!(
            probe_1_rows
                .iter()
                .map(row_body_name)
                .collect::<Vec<_>>(),
            vec!["delta".to_string(), "echo-2".to_string()]
        );
        assert!(probe_0_rows.iter().all(|row| row_text(row, "extra").is_empty()));
        assert!(probe_1_rows.iter().all(|row| row_text(row, "extra").is_empty()));

        let ghost_count = scalar_i64(
            &connection,
            "SELECT COUNT(*) AS count FROM information_schema.tables WHERE table_schema = 'public' AND table_name IN ('ddl_probe_0__ghost', 'ddl_probe_1__ghost', 'ddl_probe_0__old', 'ddl_probe_1__old')",
        )
        .await;
        assert_eq!(ghost_count, 0);

        let publication_count = scalar_i64(
            &connection,
            "SELECT COUNT(*) AS count FROM pg_publication WHERE pubname IN ('public_ddl_probe_0_ddl_pub', 'public_ddl_probe_1_ddl_pub')",
        )
        .await;
        let slot_count = scalar_i64(
            &connection,
            "SELECT COUNT(*) AS count FROM pg_replication_slots WHERE slot_name IN ('public_ddl_probe_0_ddl_slot', 'public_ddl_probe_1_ddl_slot')",
        )
        .await;
        assert_eq!(publication_count, 0);
        assert_eq!(slot_count, 0);
    }

    fn row_body_name(row: &QueryResult) -> String {
        let body = row_text(row, "body");
        serde_json::from_str::<serde_json::Value>(&body)
            .expect("body json")["name"]
            .as_str()
            .expect("body name")
            .to_string()
    }

    fn row_text(row: &QueryResult, column: &str) -> String {
        row.try_get("", column).expect("column text")
    }

    async fn scalar_i64(connection: &sea_orm::DatabaseConnection, sql: &str) -> i64 {
        connection
            .query_one_raw(Statement::from_string(DbBackend::Postgres, sql))
            .await
            .expect("scalar query")
            .expect("scalar row")
            .try_get("", "count")
            .expect("count")
    }
}
