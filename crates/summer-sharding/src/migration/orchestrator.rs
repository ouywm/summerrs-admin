use chrono::{DateTime, FixedOffset};

use crate::{
    config::{CdcTaskConfig, TableRuleConfig},
    error::{Result, ShardingError},
    migration::{
        ArchiveCandidate, ArchivePlanner, AutoTablePlanner, ReshardingMove, ReshardingPlanner,
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrationTaskKind {
    TenantUpgrade,
    Reshard,
    Archive,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrationPhase {
    Snapshot,
    CatchUp,
    CutOver,
    Cleanup,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrationSink {
    Tables { tables: Vec<String> },
    Schema { schema: String, tables: Vec<String> },
    ClickHouse { uri: String },
}

impl MigrationSink {
    pub fn schema_name(&self) -> Option<&str> {
        match self {
            MigrationSink::Schema { schema, .. } => Some(schema.as_str()),
            _ => None,
        }
    }

    pub fn tables(&self) -> &[String] {
        match self {
            MigrationSink::Tables { tables } | MigrationSink::Schema { tables, .. } => {
                tables.as_slice()
            }
            MigrationSink::ClickHouse { .. } => &[],
        }
    }

    pub fn clickhouse_uri(&self) -> Option<&str> {
        match self {
            MigrationSink::ClickHouse { uri } => Some(uri.as_str()),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationExecutionStep {
    pub phase: MigrationPhase,
    pub detail: String,
    pub statement: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationExecutionPlan {
    pub name: String,
    pub kind: MigrationTaskKind,
    pub source_tables: Vec<String>,
    pub source_filter: Option<String>,
    pub sink: MigrationSink,
    pub transformer: Option<String>,
    pub batch_size: usize,
    pub cleanup_delete_source: bool,
    pub steps: Vec<MigrationExecutionStep>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationPlan {
    pub create_statements: Vec<String>,
    pub archive_candidates: Vec<ArchiveCandidate>,
    pub reshard_moves: Vec<ReshardingMove>,
    pub execution_plans: Vec<MigrationExecutionPlan>,
}

#[derive(Debug, Clone, Default)]
pub struct MigrationOrchestrator {
    auto: AutoTablePlanner,
    archive: ArchivePlanner,
    reshard: ReshardingPlanner,
}

impl MigrationOrchestrator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn plan_full_cycle(
        &self,
        rule: &TableRuleConfig,
        now: DateTime<FixedOffset>,
        old_shard_count: usize,
        new_shard_count: usize,
        samples: impl IntoIterator<Item = i64>,
    ) -> Result<MigrationPlan> {
        let create_statements = self
            .auto
            .plan_create_sql(rule, rule.logic_table.as_str(), now)?;
        let archive_candidates = self.archive.plan(rule, now)?;
        let reshard_moves =
            self.reshard
                .plan_hash_mod_expand(old_shard_count, new_shard_count, samples);
        Ok(MigrationPlan {
            create_statements,
            archive_candidates,
            reshard_moves,
            execution_plans: Vec::new(),
        })
    }

    pub fn plan_cdc_task(&self, task: &CdcTaskConfig) -> Result<MigrationExecutionPlan> {
        let kind = Self::task_kind(task);
        let sink = Self::sink_shape(task, &kind)?;
        let slot = Self::slot_name(task.name.as_str());
        let publication = Self::publication_name(task.name.as_str());
        let source_tables = task.source_tables.clone();
        let source_filter = task.source_filter.clone();
        let filter_clause = source_filter
            .as_ref()
            .map(|filter| format!(" WHERE {filter}"))
            .unwrap_or_default();
        let snapshot_step = match &sink {
            MigrationSink::Tables { tables } => MigrationExecutionStep {
                phase: MigrationPhase::Snapshot,
                detail: if task.transformer.is_some() || source_tables.len() != tables.len() {
                    format!(
                        "stream snapshot through transformer {:?} into target shards [{}]",
                        task.transformer,
                        tables.join(", ")
                    )
                } else {
                    format!("snapshot copy into target shards [{}]", tables.join(", "))
                },
                statement: join_statements(Self::snapshot_statements(
                    &source_tables,
                    &sink,
                    task.transformer.as_deref(),
                    filter_clause.as_str(),
                )),
            },
            MigrationSink::Schema { schema, .. } => MigrationExecutionStep {
                phase: MigrationPhase::Snapshot,
                detail: format!("snapshot tenant data into schema `{schema}`"),
                statement: join_statements(Self::snapshot_statements(
                    &source_tables,
                    &sink,
                    task.transformer.as_deref(),
                    filter_clause.as_str(),
                )),
            },
            MigrationSink::ClickHouse { uri } => MigrationExecutionStep {
                phase: MigrationPhase::Snapshot,
                detail: format!("export cold data to clickhouse `{uri}`"),
                statement: join_statements(Self::snapshot_statements(
                    &source_tables,
                    &sink,
                    task.transformer.as_deref(),
                    filter_clause.as_str(),
                )),
            },
        };
        let catch_up_step = MigrationExecutionStep {
            phase: MigrationPhase::CatchUp,
            detail: format!(
                "prepare logical replication slot `{slot}` and publication `{publication}`"
            ),
            statement: join_statements(Self::catch_up_statements(
                &source_tables,
                slot.as_str(),
                publication.as_str(),
            )),
        };
        let cutover_step = MigrationExecutionStep {
            phase: MigrationPhase::CutOver,
            detail: "switch route to migrated sink target".to_string(),
            statement: None,
        };
        let cleanup_step = MigrationExecutionStep {
            phase: MigrationPhase::Cleanup,
            detail: if task.delete_after_migrate {
                "delete source rows after migration verification".to_string()
            } else {
                "keep source rows for rollback window".to_string()
            },
            statement: join_statements(if task.delete_after_migrate {
                Self::cleanup_statements(&source_tables, filter_clause.as_str())
            } else {
                Default::default()
            }),
        };

        Ok(MigrationExecutionPlan {
            name: task.name.clone(),
            kind,
            source_tables,
            source_filter,
            sink,
            transformer: task.transformer.clone(),
            batch_size: task.batch_size,
            cleanup_delete_source: task.delete_after_migrate,
            steps: vec![snapshot_step, catch_up_step, cutover_step, cleanup_step],
        })
    }

    pub fn plan_cdc_tasks(
        &self,
        tasks: impl IntoIterator<Item = CdcTaskConfig>,
    ) -> Result<Vec<MigrationExecutionPlan>> {
        tasks
            .into_iter()
            .map(|task| self.plan_cdc_task(&task))
            .collect()
    }

    fn task_kind(task: &CdcTaskConfig) -> MigrationTaskKind {
        if task
            .sink_type
            .as_deref()
            .map(|kind| kind.eq_ignore_ascii_case("clickhouse"))
            .unwrap_or(false)
        {
            MigrationTaskKind::Archive
        } else if task.sink_schema.is_some() {
            MigrationTaskKind::TenantUpgrade
        } else {
            MigrationTaskKind::Reshard
        }
    }

    fn sink_shape(task: &CdcTaskConfig, kind: &MigrationTaskKind) -> Result<MigrationSink> {
        match kind {
            MigrationTaskKind::Archive => {
                let uri = task.sink_uri.clone().ok_or_else(|| {
                    ShardingError::Config(format!(
                        "cdc task `{}` uses archive sink but missing `sink_uri`",
                        task.name
                    ))
                })?;
                Ok(MigrationSink::ClickHouse { uri })
            }
            MigrationTaskKind::TenantUpgrade => {
                let schema = task.sink_schema.clone().ok_or_else(|| {
                    ShardingError::Config(format!(
                        "cdc task `{}` tenant upgrade missing `sink_schema`",
                        task.name
                    ))
                })?;
                let tables = if task.sink_tables.is_empty() {
                    task.source_tables
                        .iter()
                        .map(|table| Self::unqualified_table_name(table))
                        .collect()
                } else {
                    task.sink_tables.clone()
                };
                Ok(MigrationSink::Schema { schema, tables })
            }
            MigrationTaskKind::Reshard => {
                let tables = if task.sink_tables.is_empty() {
                    task.source_tables.clone()
                } else {
                    task.sink_tables.clone()
                };
                Ok(MigrationSink::Tables { tables })
            }
        }
    }

    fn slot_name(task_name: &str) -> String {
        format!("{}_slot", sanitize_identifier(task_name))
    }

    fn publication_name(task_name: &str) -> String {
        format!("{}_pub", sanitize_identifier(task_name))
    }

    fn snapshot_statements(
        source_tables: &[String],
        sink: &MigrationSink,
        transformer: Option<&str>,
        filter_clause: &str,
    ) -> Vec<String> {
        match sink {
            MigrationSink::Tables { tables } => {
                if transformer.is_some() || source_tables.len() != tables.len() {
                    Vec::new()
                } else {
                    source_tables
                        .iter()
                        .zip(tables)
                        .map(|(source, target)| {
                            format!("INSERT INTO {target} SELECT * FROM {source}{filter_clause}")
                        })
                        .collect()
                }
            }
            MigrationSink::Schema { schema, tables } => source_tables
                .iter()
                .zip(tables)
                .map(|(source, target)| {
                    format!(
                        "INSERT INTO {} SELECT * FROM {source}{filter_clause}",
                        qualify_schema_table(schema, target)
                    )
                })
                .collect(),
            MigrationSink::ClickHouse { uri } => {
                let uri = escape_literal(uri);
                source_tables
                    .iter()
                    .map(|source| {
                        format!(
                            "INSERT INTO FUNCTION remote('{uri}') SELECT * FROM {source}{filter_clause}"
                        )
                    })
                    .collect()
            }
        }
    }

    fn catch_up_statements(source_tables: &[String], slot: &str, publication: &str) -> Vec<String> {
        let source_list = source_tables.join(", ");
        let slot_literal = escape_literal(slot);
        vec![
            source_tables
                .iter()
                .map(|table| format!("ALTER TABLE {table} REPLICA IDENTITY FULL"))
                .collect::<Vec<_>>(),
            vec![format!(
                "SELECT pg_drop_replication_slot('{slot_literal}') FROM pg_replication_slots WHERE slot_name = '{slot_literal}' AND NOT active"
            )],
            vec![format!(
                "SELECT CASE WHEN EXISTS (SELECT 1 FROM pg_replication_slots WHERE slot_name = '{slot_literal}') THEN '{slot_literal}' ELSE (SELECT slot_name FROM pg_create_logical_replication_slot('{slot_literal}', 'pgoutput')) END"
            )],
            vec![format!("DROP PUBLICATION IF EXISTS {publication}")],
            vec![format!("CREATE PUBLICATION {publication} FOR TABLE {source_list}")],
        ]
        .into_iter()
        .flatten()
        .collect()
    }

    fn cleanup_statements(source_tables: &[String], filter_clause: &str) -> Vec<String> {
        source_tables
            .iter()
            .map(|source| format!("DELETE FROM {source}{filter_clause}"))
            .collect()
    }

    fn unqualified_table_name(table: &str) -> String {
        table
            .rsplit_once('.')
            .map(|(_, table)| table.to_string())
            .unwrap_or_else(|| table.to_string())
    }
}

fn join_statements(statements: Vec<String>) -> Option<String> {
    (!statements.is_empty()).then(|| statements.join(";\n"))
}

fn sanitize_identifier(value: &str) -> String {
    let mut sanitized = value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect::<String>();
    if sanitized.is_empty() {
        sanitized.push('_');
    }
    if sanitized
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_digit())
    {
        sanitized.insert(0, '_');
    }
    sanitized
}

fn qualify_schema_table(schema: &str, table: &str) -> String {
    if table.contains('.') {
        table.to_string()
    } else {
        format!("{schema}.{table}")
    }
}

fn escape_literal(value: &str) -> String {
    value.replace('\'', "''")
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::TimeZone;
    use reqwest::Client;

    use crate::{
        CdcOperation, CdcRecord, ClickHouseHttpSink, InMemoryCdcSource, PgCdcSource,
        PostgresHashShardSink, RowTransformer, SqlMigrationCleanup, TableSink,
        cdc::test_support::{ClickHouseTestServer, LogicalReplicationTestDatabase},
        config::{CdcTaskConfig, ShardingConfig},
        migration::{MigrationExecutor, MigrationOrchestrator, MigrationTaskKind},
    };
    use sea_orm::{ConnectionTrait, Database, DbBackend, MockDatabase, MockExecResult, Statement};

    #[test]
    fn orchestrator_builds_full_plan() {
        let config = ShardingConfig::from_test_str(
            r#"
            [datasources.ds]
            uri = "mock://db"
            role = "primary"

            [[sharding.tables]]
            logic_table = "ai.log"
            actual_tables = "ai.log_${yyyyMM}"
            sharding_column = "create_time"
            algorithm = "time_range"

              [sharding.tables.algorithm_props]
              granularity = "month"
              retention_months = 1
            "#,
        )
        .expect("config");
        let rule = &config.sharding.tables[0];
        let now = chrono::FixedOffset::east_opt(0)
            .unwrap()
            .with_ymd_and_hms(2026, 3, 1, 0, 0, 0)
            .unwrap();
        let plan = MigrationOrchestrator::new()
            .plan_full_cycle(rule, now, 2, 4, [1, 2, 3, 4])
            .expect("plan");
        assert!(!plan.create_statements.is_empty());
        assert!(!plan.archive_candidates.is_empty());
        assert!(!plan.reshard_moves.is_empty());
    }

    #[test]
    fn orchestrator_builds_tenant_upgrade_execution_plan() {
        let task = CdcTaskConfig {
            name: "migrate_tenant_ent01".to_string(),
            source_tables: vec!["ai.log".to_string()],
            source_filter: Some("tenant_id = 'T-ENT-01'".to_string()),
            sink_schema: Some("tenant_ent01".to_string()),
            delete_after_migrate: true,
            ..CdcTaskConfig::default()
        };
        let plan = MigrationOrchestrator::new()
            .plan_cdc_task(&task)
            .expect("tenant upgrade plan");
        assert_eq!(plan.kind, MigrationTaskKind::TenantUpgrade);
        assert_eq!(plan.sink.schema_name(), Some("tenant_ent01"));
        assert_eq!(
            plan.steps[0].statement.as_deref(),
            Some("INSERT INTO tenant_ent01.log SELECT * FROM ai.log WHERE tenant_id = 'T-ENT-01'")
        );
        assert!(
            plan.steps[1]
                .statement
                .as_deref()
                .is_some_and(|statement| statement.contains("pg_create_logical_replication_slot"))
        );
        assert_eq!(
            plan.steps[3].statement.as_deref(),
            Some("DELETE FROM ai.log WHERE tenant_id = 'T-ENT-01'")
        );
    }

    #[test]
    fn orchestrator_builds_reshard_execution_plan() {
        let task = CdcTaskConfig {
            name: "expand_log_shards".to_string(),
            source_tables: vec!["ai.log_202601".to_string()],
            sink_tables: vec![
                "ai.log_0".to_string(),
                "ai.log_1".to_string(),
                "ai.log_2".to_string(),
                "ai.log_3".to_string(),
            ],
            transformer: Some("rehash".to_string()),
            ..CdcTaskConfig::default()
        };
        let plan = MigrationOrchestrator::new()
            .plan_cdc_task(&task)
            .expect("reshard plan");
        assert_eq!(plan.kind, MigrationTaskKind::Reshard);
        assert_eq!(plan.sink.tables().len(), 4);
        assert!(plan.steps[0].statement.is_none());
    }

    #[test]
    fn orchestrator_builds_clickhouse_archive_execution_plan() {
        let task = CdcTaskConfig {
            name: "archive_old_logs".to_string(),
            source_tables: vec!["ai.log_202401".to_string()],
            sink_type: Some("clickhouse".to_string()),
            sink_uri: Some("http://clickhouse:8123".to_string()),
            delete_after_migrate: true,
            ..CdcTaskConfig::default()
        };
        let plan = MigrationOrchestrator::new()
            .plan_cdc_task(&task)
            .expect("archive plan");
        assert_eq!(plan.kind, MigrationTaskKind::Archive);
        assert_eq!(plan.sink.clickhouse_uri(), Some("http://clickhouse:8123"));
        assert!(plan.cleanup_delete_source);
        assert!(
            plan.steps[0]
                .statement
                .as_deref()
                .is_some_and(|statement| statement.contains("INSERT INTO FUNCTION remote('http://clickhouse:8123') SELECT * FROM ai.log_202401"))
        );
        assert_eq!(
            plan.steps[3].statement.as_deref(),
            Some("DELETE FROM ai.log_202401")
        );
    }

    #[tokio::test]
    async fn migration_executor_runs_pipeline_and_sql_cleanup() {
        let task = CdcTaskConfig {
            name: "migrate_old_logs".to_string(),
            source_tables: vec!["ai.log_202401".to_string()],
            delete_after_migrate: true,
            ..CdcTaskConfig::default()
        };
        let plan = MigrationOrchestrator::new()
            .plan_cdc_task(&task)
            .expect("migration plan");

        let source = InMemoryCdcSource::new()
            .with_replication("migrate_old_logs_slot", "migrate_old_logs_pub")
            .with_snapshot(
                "ai.log_202401",
                vec![CdcRecord {
                    table: "ai.log_202401".to_string(),
                    key: "1".to_string(),
                    payload: serde_json::json!({"name":"alpha"}),
                    operation: CdcOperation::Snapshot,
                    source_lsn: Some("0/1".to_string()),
                }],
            )
            .with_change(CdcRecord {
                table: "ai.log_202401".to_string(),
                key: "2".to_string(),
                payload: serde_json::json!({"name":"beta"}),
                operation: CdcOperation::Insert,
                source_lsn: Some("0/2".to_string()),
            });
        let sink = TableSink::default();
        let cleanup_connection = Arc::new(
            MockDatabase::new(DbBackend::Postgres)
                .append_exec_results(vec![MockExecResult {
                    rows_affected: 1,
                    last_insert_id: 0,
                }])
                .into_connection(),
        );
        let cleanup = SqlMigrationCleanup::new(cleanup_connection.clone());

        let report = MigrationExecutor::new()
            .execute(&plan, &source, &RowTransformer, &sink, None, Some(&cleanup))
            .await
            .expect("execute migration");

        assert_eq!(report.snapshot_written, 1);
        assert_eq!(report.catch_up_written, 1);
        assert_eq!(report.cleanup_statements, 1);
        assert_eq!(sink.rows().len(), 2);

        let cleanup_log = cleanup_connection.as_ref().clone().into_transaction_log();
        assert_eq!(cleanup_log.len(), 1);
    }

    #[tokio::test]
    async fn migration_executor_requires_cleanup_handler_when_delete_after_migrate_enabled() {
        let task = CdcTaskConfig {
            name: "archive_old_logs".to_string(),
            source_tables: vec!["ai.log_202401".to_string()],
            sink_type: Some("clickhouse".to_string()),
            sink_uri: Some("http://clickhouse:8123".to_string()),
            delete_after_migrate: true,
            ..CdcTaskConfig::default()
        };
        let plan = MigrationOrchestrator::new()
            .plan_cdc_task(&task)
            .expect("archive plan");
        let source = InMemoryCdcSource::new()
            .with_replication("archive_old_logs_slot", "archive_old_logs_pub");
        let sink = ClickHouseHttpSink::new("http://127.0.0.1:8123");

        let error = MigrationExecutor::new()
            .execute(&plan, &source, &RowTransformer, &sink, None, None)
            .await
            .expect_err("missing cleanup should fail");

        assert!(error.to_string().contains("cleanup"));
    }

    #[tokio::test]
    async fn migration_executor_rejects_sink_runtime_mismatch() {
        let task = CdcTaskConfig {
            name: "archive_mismatch".to_string(),
            source_tables: vec!["ai.log_202401".to_string()],
            sink_type: Some("clickhouse".to_string()),
            sink_uri: Some("http://clickhouse:8123".to_string()),
            ..CdcTaskConfig::default()
        };
        let plan = MigrationOrchestrator::new()
            .plan_cdc_task(&task)
            .expect("archive plan");
        let source = InMemoryCdcSource::new()
            .with_replication("archive_mismatch_slot", "archive_mismatch_pub");
        let sink = TableSink::default();

        let error = MigrationExecutor::new()
            .execute(&plan, &source, &RowTransformer, &sink, None, None)
            .await
            .expect_err("sink mismatch should fail");

        assert!(error.to_string().contains("expects a ClickHouse sink"));
    }

    #[tokio::test]
    async fn migration_executor_rejects_transformer_runtime_mismatch() {
        let task = CdcTaskConfig {
            name: "rehash_mismatch".to_string(),
            source_tables: vec!["ai.log".to_string()],
            sink_tables: vec!["ai.log_0".to_string(), "ai.log_1".to_string()],
            transformer: Some("rehash".to_string()),
            ..CdcTaskConfig::default()
        };
        let plan = MigrationOrchestrator::new()
            .plan_cdc_task(&task)
            .expect("rehash plan");
        let source = InMemoryCdcSource::new()
            .with_replication("rehash_mismatch_slot", "rehash_mismatch_pub");
        let sink = TableSink::default();

        let error = MigrationExecutor::new()
            .execute(&plan, &source, &RowTransformer, &sink, None, None)
            .await
            .expect_err("transformer mismatch should fail");

        assert!(
            error
                .to_string()
                .contains("expects a hash-sharded sink for transformer `rehash`")
        );
    }

    #[tokio::test]
    async fn migration_executor_rejects_schema_plan_with_hash_sharded_sink() {
        let task = CdcTaskConfig {
            name: "tenant_upgrade_mismatch".to_string(),
            source_tables: vec!["public.ai_log".to_string()],
            sink_schema: Some("tenant_t001".to_string()),
            ..CdcTaskConfig::default()
        };
        let plan = MigrationOrchestrator::new()
            .plan_cdc_task(&task)
            .expect("tenant upgrade plan");
        let source = InMemoryCdcSource::new().with_replication(
            "tenant_upgrade_mismatch_slot",
            "tenant_upgrade_mismatch_pub",
        );
        let sink = PostgresHashShardSink::new(
            Arc::new(MockDatabase::new(DbBackend::Postgres).into_connection()),
            vec![
                "tenant_t001.ai_log_0".to_string(),
                "tenant_t001.ai_log_1".to_string(),
            ],
        );

        let error = MigrationExecutor::new()
            .execute(&plan, &source, &RowTransformer, &sink, None, None)
            .await
            .expect_err("schema plan with hash sink should fail");

        assert!(
            error
                .to_string()
                .contains("expects a direct relational sink")
        );
    }

    #[tokio::test]
    async fn migration_executor_applies_source_filter_to_snapshot_and_catch_up() {
        let task = CdcTaskConfig {
            name: "tenant_filter".to_string(),
            source_tables: vec!["ai.log".to_string()],
            source_filter: Some("tenant_id = 'T-001'".to_string()),
            ..CdcTaskConfig::default()
        };
        let plan = MigrationOrchestrator::new()
            .plan_cdc_task(&task)
            .expect("filtered plan");
        let source = InMemoryCdcSource::new()
            .with_replication("tenant_filter_slot", "tenant_filter_pub")
            .with_snapshot(
                "ai.log",
                vec![
                    CdcRecord {
                        table: "ai.log".to_string(),
                        key: "1".to_string(),
                        payload: serde_json::json!({"id":1,"tenant_id":"T-001","body":{"name":"alpha"}}),
                        operation: CdcOperation::Snapshot,
                        source_lsn: Some("0/1".to_string()),
                    },
                    CdcRecord {
                        table: "ai.log".to_string(),
                        key: "2".to_string(),
                        payload: serde_json::json!({"id":2,"tenant_id":"T-002","body":{"name":"beta"}}),
                        operation: CdcOperation::Snapshot,
                        source_lsn: Some("0/2".to_string()),
                    },
                ],
            )
            .with_change(CdcRecord {
                table: "ai.log".to_string(),
                key: "3".to_string(),
                payload: serde_json::json!({"id":3,"tenant_id":"T-002","body":{"name":"gamma"}}),
                operation: CdcOperation::Insert,
                source_lsn: Some("0/3".to_string()),
            })
            .with_change(CdcRecord {
                table: "ai.log".to_string(),
                key: "4".to_string(),
                payload: serde_json::json!({"id":4,"tenant_id":"T-001","body":{"name":"delta"}}),
                operation: CdcOperation::Insert,
                source_lsn: Some("0/4".to_string()),
            });
        let sink = TableSink::default();

        let report = MigrationExecutor::new()
            .execute(&plan, &source, &RowTransformer, &sink, None, None)
            .await
            .expect("execute filtered migration");

        let rows = sink.rows();
        assert_eq!(report.snapshot_written, 1);
        assert_eq!(report.catch_up_written, 1);
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|row| row.payload["tenant_id"] == "T-001"));
        assert!(rows.iter().any(|row| row.key == "1"));
        assert!(rows.iter().any(|row| row.key == "4"));
    }

    #[tokio::test]
    #[ignore = "requires docker or SUMMER_SHARDING_CDC_E2E_DATABASE_URL"]
    async fn migration_executor_reshards_rows_into_real_postgres_targets() {
        let test_db = LogicalReplicationTestDatabase::start()
            .await
            .expect("start logical replication database");
        let database_url = test_db.database_url().to_string();
        let connection = Arc::new(
            Database::connect(&database_url)
                .await
                .expect("connect logical replication database"),
        );

        connection
            .execute_unprepared(
                r#"
                DROP TABLE IF EXISTS public.reshard_source CASCADE;
                DROP TABLE IF EXISTS public.reshard_target_0 CASCADE;
                DROP TABLE IF EXISTS public.reshard_target_1 CASCADE;
                DROP TABLE IF EXISTS public.reshard_target_2 CASCADE;
                DROP TABLE IF EXISTS public.reshard_target_3 CASCADE;

                CREATE TABLE public.reshard_source (
                    id BIGINT PRIMARY KEY,
                    tenant_id VARCHAR(64) NOT NULL,
                    body JSONB NOT NULL
                );

                CREATE TABLE public.reshard_target_0 (LIKE public.reshard_source INCLUDING ALL);
                CREATE TABLE public.reshard_target_1 (LIKE public.reshard_source INCLUDING ALL);
                CREATE TABLE public.reshard_target_2 (LIKE public.reshard_source INCLUDING ALL);
                CREATE TABLE public.reshard_target_3 (LIKE public.reshard_source INCLUDING ALL);

                INSERT INTO public.reshard_source(id, tenant_id, body)
                VALUES
                    (1, 'T-001', '{"name":"alpha"}'::jsonb),
                    (2, 'T-001', '{"name":"beta"}'::jsonb),
                    (3, 'T-002', '{"name":"gamma"}'::jsonb),
                    (4, 'T-002', '{"name":"delta"}'::jsonb);
                "#,
            )
            .await
            .expect("seed source and target shard tables");

        let task = CdcTaskConfig {
            name: "expand_log_shards_real".to_string(),
            source_tables: vec!["public.reshard_source".to_string()],
            sink_tables: vec![
                "public.reshard_target_0".to_string(),
                "public.reshard_target_1".to_string(),
                "public.reshard_target_2".to_string(),
                "public.reshard_target_3".to_string(),
            ],
            transformer: Some("rehash".to_string()),
            batch_size: 1,
            ..CdcTaskConfig::default()
        };
        let plan = MigrationOrchestrator::new()
            .plan_cdc_task(&task)
            .expect("build reshard plan");
        if let Some(catch_up_sql) = plan.steps[1].statement.as_deref() {
            execute_postgres_statements(connection.as_ref(), catch_up_sql)
                .await
                .expect("prepare reshard publication and slot");
        }
        let source = PgCdcSource::new(connection.clone(), &database_url).expect("build pg source");
        let sink = PostgresHashShardSink::new(
            connection.clone(),
            vec![
                "public.reshard_target_0".to_string(),
                "public.reshard_target_1".to_string(),
                "public.reshard_target_2".to_string(),
                "public.reshard_target_3".to_string(),
            ],
        );
        let executor = MigrationExecutor::new();

        let execute_handle = tokio::spawn({
            let plan = plan.clone();
            let source = source;
            let sink = sink;
            async move {
                executor
                    .execute(&plan, &source, &RowTransformer, &sink, None, None)
                    .await
            }
        });

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        connection
            .execute_unprepared(
                r#"
                INSERT INTO public.reshard_source(id, tenant_id, body)
                VALUES (5, 'T-003', '{"name":"epsilon"}'::jsonb);
                UPDATE public.reshard_source
                SET body = '{"name":"beta-2"}'::jsonb
                WHERE id = 2;
                DELETE FROM public.reshard_source
                WHERE id = 3;
                "#,
            )
            .await
            .expect("apply reshard catch-up mutations");

        execute_handle
            .await
            .expect("join reshard execution")
            .expect("execute real reshard plan");

        assert_eq!(
            shard_ids(connection.as_ref(), "public.reshard_target_0").await,
            vec![4]
        );
        assert_eq!(
            shard_ids(connection.as_ref(), "public.reshard_target_1").await,
            vec![1, 5]
        );
        assert_eq!(
            shard_ids(connection.as_ref(), "public.reshard_target_2").await,
            vec![2]
        );
        assert_eq!(
            shard_ids(connection.as_ref(), "public.reshard_target_3").await,
            Vec::<i64>::new()
        );
        assert_eq!(
            shard_body_name(connection.as_ref(), "public.reshard_target_2", 2).await,
            "beta-2".to_string()
        );
    }

    async fn shard_ids(connection: &sea_orm::DatabaseConnection, table: &str) -> Vec<i64> {
        connection
            .query_all_raw(Statement::from_string(
                DbBackend::Postgres,
                format!("SELECT id FROM {table} ORDER BY id"),
            ))
            .await
            .expect("query shard ids")
            .into_iter()
            .map(|row| row.try_get("", "id").expect("id"))
            .collect()
    }

    async fn shard_body_name(
        connection: &sea_orm::DatabaseConnection,
        table: &str,
        id: i64,
    ) -> String {
        let row = connection
            .query_one_raw(Statement::from_string(
                DbBackend::Postgres,
                format!("SELECT body::text AS body FROM {table} WHERE id = {id}"),
            ))
            .await
            .expect("query shard row")
            .expect("shard row");
        serde_json::from_str::<serde_json::Value>(
            row.try_get::<String>("", "body").expect("body").as_str(),
        )
        .expect("body json")["name"]
            .as_str()
            .expect("body name")
            .to_string()
    }

    #[tokio::test]
    #[ignore = "requires docker or SUMMER_SHARDING_CDC_E2E_DATABASE_URL and clickhouse docker or SUMMER_SHARDING_CLICKHOUSE_E2E_URL"]
    async fn migration_executor_archives_rows_into_real_clickhouse_targets() {
        let test_db = LogicalReplicationTestDatabase::start()
            .await
            .expect("start logical replication database");
        let clickhouse = ClickHouseTestServer::start()
            .await
            .expect("start clickhouse test server");
        let database_url = test_db.database_url().to_string();
        let clickhouse_url = clickhouse.http_url().to_string();

        let connection = Arc::new(
            Database::connect(&database_url)
                .await
                .expect("connect logical replication database"),
        );

        connection
            .execute_unprepared(
                r#"
                DROP TABLE IF EXISTS public.archive_source CASCADE;

                CREATE TABLE public.archive_source (
                    id BIGINT PRIMARY KEY,
                    tenant_id VARCHAR(64) NOT NULL,
                    body JSONB NOT NULL
                );

                INSERT INTO public.archive_source(id, tenant_id, body)
                VALUES
                    (1, 'T-001', '{"name":"alpha"}'::jsonb),
                    (2, 'T-001', '{"name":"beta"}'::jsonb);
                "#,
            )
            .await
            .expect("seed archive source");

        let client = Client::new();
        clickhouse_exec(
            &client,
            &clickhouse_url,
            "DROP TABLE IF EXISTS default.archive_source",
        )
        .await;
        clickhouse_exec(
            &client,
            &clickhouse_url,
            r#"
            CREATE TABLE default.archive_source (
                id Int64,
                tenant_id String,
                body String,
                version UInt64
            )
            ENGINE = ReplacingMergeTree(version)
            ORDER BY id
            "#,
        )
        .await;

        let task = CdcTaskConfig {
            name: "archive_old_logs_real".to_string(),
            source_tables: vec!["public.archive_source".to_string()],
            sink_type: Some("clickhouse".to_string()),
            sink_uri: Some(clickhouse_url.clone()),
            batch_size: 1,
            ..CdcTaskConfig::default()
        };
        let plan = MigrationOrchestrator::new()
            .plan_cdc_task(&task)
            .expect("build archive plan");
        if let Some(catch_up_sql) = plan.steps[1].statement.as_deref() {
            execute_postgres_statements(connection.as_ref(), catch_up_sql)
                .await
                .expect("prepare archive publication and slot");
        }
        let source = PgCdcSource::new(connection.clone(), &database_url).expect("build pg source");
        let sink = ClickHouseHttpSink::with_table_map(
            clickhouse_url.clone(),
            [(
                "public.archive_source".to_string(),
                "default.archive_source".to_string(),
            )],
        );
        let executor = MigrationExecutor::new();

        let execute_handle = tokio::spawn({
            let plan = plan.clone();
            let source = source;
            let sink = sink;
            async move {
                executor
                    .execute(&plan, &source, &RowTransformer, &sink, None, None)
                    .await
            }
        });

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        connection
            .execute_unprepared(
                r#"
                INSERT INTO public.archive_source(id, tenant_id, body)
                VALUES (3, 'T-002', '{"name":"gamma"}'::jsonb);
                UPDATE public.archive_source
                SET body = '{"name":"beta-2"}'::jsonb
                WHERE id = 2;
                "#,
            )
            .await
            .expect("apply archive catch-up mutations");

        execute_handle
            .await
            .expect("join archive execution")
            .expect("execute real archive plan");

        let rows = clickhouse_query_json_each_row(
            &client,
            &clickhouse_url,
            "SELECT id, tenant_id, body FROM default.archive_source FINAL ORDER BY id FORMAT JSONEachRow",
        )
        .await;
        assert_eq!(rows.len(), 3);
        assert_eq!(json_i64(&rows[0]["id"]), 1);
        assert_eq!(json_i64(&rows[1]["id"]), 2);
        assert_eq!(json_i64(&rows[2]["id"]), 3);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(
                rows[1]["body"].as_str().expect("body string")
            )
            .expect("body json")["name"],
            "beta-2"
        );
    }

    async fn clickhouse_exec(client: &Client, base_url: &str, sql: &str) {
        let response = client
            .post(base_url)
            .body(sql.to_string())
            .send()
            .await
            .expect("clickhouse exec response");
        assert!(
            response.status().is_success(),
            "clickhouse exec failed: {}",
            response.text().await.expect("clickhouse error body")
        );
    }

    async fn clickhouse_query_json_each_row(
        client: &Client,
        base_url: &str,
        sql: &str,
    ) -> Vec<serde_json::Value> {
        let response = client
            .post(base_url)
            .body(sql.to_string())
            .send()
            .await
            .expect("clickhouse query response");
        let status = response.status();
        let body = response.text().await.expect("clickhouse query body");
        assert!(status.is_success(), "clickhouse query failed: {body}");
        body.lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).expect("json each row"))
            .collect()
    }

    async fn execute_postgres_statements(
        connection: &sea_orm::DatabaseConnection,
        sql: &str,
    ) -> crate::Result<()> {
        for statement in sql
            .split(';')
            .map(str::trim)
            .filter(|stmt| !stmt.is_empty())
        {
            connection.execute_unprepared(statement).await?;
        }
        Ok(())
    }

    fn json_i64(value: &serde_json::Value) -> i64 {
        if let Some(number) = value.as_i64() {
            number
        } else if let Some(text) = value.as_str() {
            text.parse().expect("json integer string")
        } else {
            panic!("unexpected json integer value: {value}");
        }
    }
}
