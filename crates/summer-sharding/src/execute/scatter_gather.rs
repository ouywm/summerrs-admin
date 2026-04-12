use async_trait::async_trait;
use futures::future::join_all;
use sea_orm::{DbErr, ProxyExecResult};

use crate::{
    connector::statement::StatementContext,
    error::{Result, ShardingError},
    execute::{ExecutionUnit, Executor, RawStatementExecutor, ensure_units},
    merge::ResultMerger,
    router::RoutePlan,
};

#[derive(Debug, Clone, Default)]
pub struct ScatterGatherExecutor;

async fn execute_measured(
    raw: &dyn RawStatementExecutor,
    units: Vec<ExecutionUnit>,
) -> Result<sea_orm::ExecResult> {
    ensure_units(&units)?;
    if units.len() == 1 {
        return super::simple::SimpleExecutor.execute(raw, units).await;
    }

    let results = join_all(units.into_iter().map(|unit| async move {
        let datasource = unit.datasource.clone();
        let result = raw.execute_for(datasource.as_str(), unit.statement).await;
        (datasource, result)
    }))
    .await;

    let mut errors = Vec::new();
    let mut exec_results = Vec::new();
    for (datasource, result) in results {
        match result {
            Ok(result) => exec_results.push(result),
            Err(error) => errors.push((datasource, error)),
        }
    }
    if !errors.is_empty() {
        return Err(aggregate_db_errors("multi-shard execute", errors));
    }

    let rows_affected = exec_results
        .iter()
        .map(sea_orm::ExecResult::rows_affected)
        .sum();
    let last_insert_id = exec_results
        .last()
        .map(sea_orm::ExecResult::last_insert_id)
        .unwrap_or_default();
    Ok(ProxyExecResult {
        last_insert_id,
        rows_affected,
    }
    .into())
}

async fn query_one_measured(
    raw: &dyn RawStatementExecutor,
    units: Vec<ExecutionUnit>,
    analysis: &StatementContext,
    plan: &RoutePlan,
    merger: &dyn ResultMerger,
) -> Result<Option<sea_orm::QueryResult>> {
    let rows = query_all_measured(raw, units, analysis, plan, merger).await?;
    Ok(rows.into_iter().next())
}

async fn query_all_measured(
    raw: &dyn RawStatementExecutor,
    units: Vec<ExecutionUnit>,
    analysis: &StatementContext,
    plan: &RoutePlan,
    merger: &dyn ResultMerger,
) -> Result<Vec<sea_orm::QueryResult>> {
    ensure_units(&units)?;
    if units.len() == 1 {
        return super::simple::SimpleExecutor
            .query_all(raw, units, analysis, plan, merger)
            .await;
    }

    let results = join_all(units.into_iter().map(|unit| async move {
        let datasource = unit.datasource.clone();
        let result = raw.query_all_for(datasource.as_str(), unit.statement).await;
        (datasource, result)
    }))
    .await;

    let mut errors = Vec::new();
    let mut shards = Vec::new();
    for (datasource, result) in results {
        match result {
            Ok(rows) => shards.push(rows),
            Err(error) => errors.push((datasource, error)),
        }
    }
    if !errors.is_empty() {
        return Err(aggregate_db_errors("multi-shard query", errors));
    }

    merger.merge(shards, analysis, plan)
}

fn aggregate_db_errors(context: &str, errors: Vec<(String, DbErr)>) -> ShardingError {
    ShardingError::Route(format!(
        "{context} failed on {} shard(s): {}",
        errors.len(),
        errors
            .into_iter()
            .map(|(datasource, error)| format!("{datasource}: {error}"))
            .collect::<Vec<_>>()
            .join("; ")
    ))
}

#[async_trait]
impl Executor for ScatterGatherExecutor {
    async fn execute(
        &self,
        raw: &dyn RawStatementExecutor,
        units: Vec<ExecutionUnit>,
    ) -> Result<sea_orm::ExecResult> {
        execute_measured(raw, units).await
    }

    async fn query_one(
        &self,
        raw: &dyn RawStatementExecutor,
        units: Vec<ExecutionUnit>,
        analysis: &StatementContext,
        plan: &RoutePlan,
        merger: &dyn ResultMerger,
    ) -> Result<Option<sea_orm::QueryResult>> {
        query_one_measured(raw, units, analysis, plan, merger).await
    }

    async fn query_all(
        &self,
        raw: &dyn RawStatementExecutor,
        units: Vec<ExecutionUnit>,
        analysis: &StatementContext,
        plan: &RoutePlan,
        merger: &dyn ResultMerger,
    ) -> Result<Vec<sea_orm::QueryResult>> {
        query_all_measured(raw, units, analysis, plan, merger).await
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, sync::Arc};

    use async_trait::async_trait;
    use parking_lot::Mutex;
    use sea_orm::{
        DbBackend, DbErr, ExecResult, ProxyExecResult, ProxyRow, QueryResult, Statement, Value,
    };

    use crate::{
        connector::analyze_statement,
        execute::{ExecutionUnit, Executor, RawStatementExecutor, ScatterGatherExecutor},
        merge::ResultMerger,
        router::{QualifiedTableName, RoutePlan, RouteTarget, SqlOperation, TableRewrite},
    };

    #[derive(Default)]
    struct RecordingRawExecutor {
        calls: Mutex<Vec<String>>,
        rows: Mutex<BTreeMap<String, Vec<BTreeMap<String, Value>>>>,
        exec_errors: Mutex<BTreeMap<String, String>>,
        query_errors: Mutex<BTreeMap<String, String>>,
    }

    #[async_trait]
    impl RawStatementExecutor for RecordingRawExecutor {
        async fn execute_for(
            &self,
            datasource: &str,
            _stmt: Statement,
        ) -> std::result::Result<ExecResult, DbErr> {
            self.calls.lock().push(format!("exec:{datasource}"));
            if let Some(message) = self.exec_errors.lock().get(datasource).cloned() {
                return Err(DbErr::Custom(message));
            }
            Ok(ProxyExecResult {
                last_insert_id: 1,
                rows_affected: 1,
            }
            .into())
        }

        async fn query_one_for(
            &self,
            datasource: &str,
            _stmt: Statement,
        ) -> std::result::Result<Option<QueryResult>, DbErr> {
            self.calls.lock().push(format!("one:{datasource}"));
            Ok(self
                .rows
                .lock()
                .get(datasource)
                .and_then(|rows| rows.first())
                .cloned()
                .map(ProxyRow::new)
                .map(QueryResult::from))
        }

        async fn query_all_for(
            &self,
            datasource: &str,
            _stmt: Statement,
        ) -> std::result::Result<Vec<QueryResult>, DbErr> {
            self.calls.lock().push(format!("all:{datasource}"));
            if let Some(message) = self.query_errors.lock().get(datasource).cloned() {
                return Err(DbErr::Custom(message));
            }
            Ok(self
                .rows
                .lock()
                .get(datasource)
                .into_iter()
                .flatten()
                .cloned()
                .map(ProxyRow::new)
                .map(QueryResult::from)
                .collect())
        }
    }

    struct CountingMerger {
        shard_count: Arc<Mutex<Vec<usize>>>,
    }

    impl ResultMerger for CountingMerger {
        fn merge(
            &self,
            shards: Vec<Vec<QueryResult>>,
            _analysis: &crate::connector::statement::StatementContext,
            _plan: &RoutePlan,
        ) -> crate::error::Result<Vec<QueryResult>> {
            self.shard_count.lock().push(shards.len());
            Ok(shards.into_iter().flatten().collect())
        }
    }

    fn analysis() -> crate::connector::statement::StatementContext {
        analyze_statement(&Statement::from_string(
            DbBackend::Postgres,
            "SELECT id FROM ai.log ORDER BY id",
        ))
        .expect("analysis")
    }

    fn plan() -> RoutePlan {
        RoutePlan {
            operation: SqlOperation::Select,
            logic_tables: vec![QualifiedTableName {
                schema: Some("ai".to_string()),
                table: "log".to_string(),
            }],
            targets: vec![RouteTarget {
                datasource: "ds_ai".to_string(),
                table_rewrites: vec![TableRewrite {
                    logic_table: QualifiedTableName {
                        schema: Some("ai".to_string()),
                        table: "log".to_string(),
                    },
                    actual_table: QualifiedTableName {
                        schema: Some("ai".to_string()),
                        table: "log".to_string(),
                    },
                }],
            }],
            order_by: Vec::new(),
            limit: None,
            offset: None,
            broadcast: false,
        }
    }

    #[tokio::test]
    async fn scatter_gather_executor_queries_all_shards() {
        let executor = ScatterGatherExecutor;
        let raw = RecordingRawExecutor {
            rows: Mutex::new(BTreeMap::from([
                (
                    "ds_a".to_string(),
                    vec![BTreeMap::from([("id".to_string(), Value::Int(Some(1)))])],
                ),
                (
                    "ds_b".to_string(),
                    vec![BTreeMap::from([("id".to_string(), Value::Int(Some(2)))])],
                ),
            ])),
            ..Default::default()
        };
        let shard_count = Arc::new(Mutex::new(Vec::new()));
        let merger = CountingMerger {
            shard_count: shard_count.clone(),
        };

        let rows = executor
            .query_all(
                &raw,
                vec![
                    ExecutionUnit {
                        datasource: "ds_a".to_string(),
                        statement: Statement::from_string(
                            DbBackend::Postgres,
                            "SELECT id FROM ai.log",
                        ),
                    },
                    ExecutionUnit {
                        datasource: "ds_b".to_string(),
                        statement: Statement::from_string(
                            DbBackend::Postgres,
                            "SELECT id FROM ai.log",
                        ),
                    },
                ],
                &analysis(),
                &plan(),
                &merger,
            )
            .await
            .expect("query");

        assert_eq!(rows.len(), 2);
        assert_eq!(shard_count.lock().as_slice(), [2]);
        assert_eq!(raw.calls.lock().len(), 2);
    }

    #[tokio::test]
    async fn scatter_gather_executor_delegates_single_shard_to_simple_executor() {
        let executor = ScatterGatherExecutor;
        let raw = RecordingRawExecutor {
            rows: Mutex::new(BTreeMap::from([(
                "ds_a".to_string(),
                vec![BTreeMap::from([("id".to_string(), Value::Int(Some(9)))])],
            )])),
            ..Default::default()
        };
        let shard_count = Arc::new(Mutex::new(Vec::new()));
        let merger = CountingMerger {
            shard_count: shard_count.clone(),
        };

        let rows = executor
            .query_all(
                &raw,
                vec![ExecutionUnit {
                    datasource: "ds_a".to_string(),
                    statement: Statement::from_string(DbBackend::Postgres, "SELECT id FROM ai.log"),
                }],
                &analysis(),
                &plan(),
                &merger,
            )
            .await
            .expect("query");

        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].try_get::<Option<i32>>("", "id").expect("id"),
            Some(9)
        );
        assert_eq!(shard_count.lock().as_slice(), [1]);
    }

    #[tokio::test]
    async fn scatter_gather_executor_executes_multi_shard_writes() {
        let executor = ScatterGatherExecutor;
        let raw = RecordingRawExecutor::default();

        let result = executor
            .execute(
                &raw,
                vec![
                    ExecutionUnit {
                        datasource: "ds_a".to_string(),
                        statement: Statement::from_string(
                            DbBackend::Postgres,
                            "UPDATE ai.log SET flag = true WHERE id = 1",
                        ),
                    },
                    ExecutionUnit {
                        datasource: "ds_b".to_string(),
                        statement: Statement::from_string(
                            DbBackend::Postgres,
                            "UPDATE ai.log SET flag = true WHERE id = 1",
                        ),
                    },
                ],
            )
            .await
            .expect("multi-shard execute");

        assert_eq!(result.rows_affected(), 2);
        assert_eq!(raw.calls.lock().as_slice(), ["exec:ds_a", "exec:ds_b"]);
    }

    #[tokio::test]
    async fn scatter_gather_executor_reports_all_query_errors() {
        let executor = ScatterGatherExecutor;
        let raw = RecordingRawExecutor {
            query_errors: Mutex::new(BTreeMap::from([
                ("ds_a".to_string(), "boom-a".to_string()),
                ("ds_b".to_string(), "boom-b".to_string()),
            ])),
            ..Default::default()
        };
        let shard_count = Arc::new(Mutex::new(Vec::new()));
        let merger = CountingMerger {
            shard_count: shard_count.clone(),
        };

        let error = executor
            .query_all(
                &raw,
                vec![
                    ExecutionUnit {
                        datasource: "ds_a".to_string(),
                        statement: Statement::from_string(
                            DbBackend::Postgres,
                            "SELECT id FROM ai.log",
                        ),
                    },
                    ExecutionUnit {
                        datasource: "ds_b".to_string(),
                        statement: Statement::from_string(
                            DbBackend::Postgres,
                            "SELECT id FROM ai.log",
                        ),
                    },
                ],
                &analysis(),
                &plan(),
                &merger,
            )
            .await
            .expect_err("query should fail");

        let message = error.to_string();
        assert!(message.contains("ds_a"));
        assert!(message.contains("boom-a"));
        assert!(message.contains("ds_b"));
        assert!(message.contains("boom-b"));
    }
}
