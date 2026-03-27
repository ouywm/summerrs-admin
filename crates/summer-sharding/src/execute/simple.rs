use async_trait::async_trait;

use crate::{
    error::{Result, ShardingError},
    execute::{ExecutionUnit, Executor, RawStatementExecutor, ensure_units},
    merge::ResultMerger,
    router::RoutePlan,
};

#[derive(Debug, Clone, Default)]
pub struct SimpleExecutor;

#[async_trait]
impl Executor for SimpleExecutor {
    async fn execute(
        &self,
        raw: &dyn RawStatementExecutor,
        units: Vec<ExecutionUnit>,
    ) -> Result<sea_orm::ExecResult> {
        ensure_units(&units)?;
        if units.len() != 1 {
            return Err(ShardingError::Unsupported(
                "multi-shard writes are not supported by the simple executor".to_string(),
            ));
        }
        let unit = units.into_iter().next().expect("validated execution unit");
        Ok(raw
            .execute_for(unit.datasource.as_str(), unit.statement)
            .await?)
    }

    async fn query_one(
        &self,
        raw: &dyn RawStatementExecutor,
        units: Vec<ExecutionUnit>,
        analysis: &crate::connector::statement::StatementContext,
        plan: &RoutePlan,
        merger: &dyn ResultMerger,
    ) -> Result<Option<sea_orm::QueryResult>> {
        let rows = self.query_all(raw, units, analysis, plan, merger).await?;
        Ok(rows.into_iter().next())
    }

    async fn query_all(
        &self,
        raw: &dyn RawStatementExecutor,
        units: Vec<ExecutionUnit>,
        analysis: &crate::connector::statement::StatementContext,
        plan: &RoutePlan,
        merger: &dyn ResultMerger,
    ) -> Result<Vec<sea_orm::QueryResult>> {
        ensure_units(&units)?;
        if units.len() != 1 {
            return Err(ShardingError::Unsupported(
                "multi-shard queries require scatter-gather execution".to_string(),
            ));
        }
        let unit = units.into_iter().next().expect("validated execution unit");
        let rows = raw
            .query_all_for(unit.datasource.as_str(), unit.statement)
            .await?;
        merger.merge(vec![rows], analysis, plan)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use async_trait::async_trait;
    use parking_lot::Mutex;
    use sea_orm::{
        DbBackend, DbErr, ExecResult, ProxyExecResult, ProxyRow, QueryResult, Statement, Value,
    };

    use crate::{
        connector::analyze_statement,
        execute::{ExecutionUnit, Executor, RawStatementExecutor, SimpleExecutor},
        merge::ResultMerger,
        router::{QualifiedTableName, RoutePlan, RouteTarget, SqlOperation, TableRewrite},
    };

    #[derive(Default)]
    struct RecordingRawExecutor {
        calls: Mutex<Vec<String>>,
        rows: Mutex<BTreeMap<String, Vec<BTreeMap<String, Value>>>>,
    }

    #[async_trait]
    impl RawStatementExecutor for RecordingRawExecutor {
        async fn execute_for(
            &self,
            datasource: &str,
            _stmt: Statement,
        ) -> std::result::Result<ExecResult, DbErr> {
            self.calls.lock().push(format!("exec:{datasource}"));
            Ok(ProxyExecResult {
                last_insert_id: 9,
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

    struct PassthroughMerger;

    impl ResultMerger for PassthroughMerger {
        fn merge(
            &self,
            shards: Vec<Vec<QueryResult>>,
            _analysis: &crate::connector::statement::StatementContext,
            _plan: &RoutePlan,
        ) -> crate::error::Result<Vec<QueryResult>> {
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
    async fn simple_executor_rejects_multi_shard_query() {
        let executor = SimpleExecutor;
        let raw = RecordingRawExecutor::default();
        let stmt = Statement::from_string(DbBackend::Postgres, "SELECT 1");

        let error = executor
            .query_all(
                &raw,
                vec![
                    ExecutionUnit {
                        datasource: "ds_a".to_string(),
                        statement: stmt.clone(),
                    },
                    ExecutionUnit {
                        datasource: "ds_b".to_string(),
                        statement: stmt,
                    },
                ],
                &analysis(),
                &plan(),
                &PassthroughMerger,
            )
            .await
            .expect_err("unsupported");

        assert!(error.to_string().contains("multi-shard queries"));
    }

    #[tokio::test]
    async fn simple_executor_executes_single_unit() {
        let executor = SimpleExecutor;
        let raw = RecordingRawExecutor::default();

        let result = executor
            .execute(
                &raw,
                vec![ExecutionUnit {
                    datasource: "ds_ai".to_string(),
                    statement: Statement::from_string(DbBackend::Postgres, "UPDATE ai.log SET ok = 1"),
                }],
            )
            .await
            .expect("execute");

        assert_eq!(result.rows_affected(), 1);
        assert_eq!(raw.calls.lock().as_slice(), ["exec:ds_ai"]);
    }

    #[tokio::test]
    async fn simple_executor_queries_single_unit_and_merges() {
        let executor = SimpleExecutor;
        let raw = RecordingRawExecutor {
            rows: Mutex::new(BTreeMap::from([(
                "ds_ai".to_string(),
                vec![BTreeMap::from([("id".to_string(), Value::Int(Some(7)))])],
            )])),
            ..Default::default()
        };

        let rows = executor
            .query_all(
                &raw,
                vec![ExecutionUnit {
                    datasource: "ds_ai".to_string(),
                    statement: Statement::from_string(DbBackend::Postgres, "SELECT id FROM ai.log"),
                }],
                &analysis(),
                &plan(),
                &PassthroughMerger,
            )
            .await
            .expect("query");

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].try_get::<Option<i32>>("", "id").expect("id"), Some(7));
        assert_eq!(raw.calls.lock().as_slice(), ["all:ds_ai"]);
    }
}
