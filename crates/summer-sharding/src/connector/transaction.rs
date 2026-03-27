use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

use sea_orm::{
    AccessMode, ConnectionTrait, DatabaseTransaction, DbBackend, DbErr, ExecResult, IsolationLevel,
    QueryResult, TransactionError, TransactionOptions, TransactionSession, TransactionTrait,
};

use crate::{
    connector::connection::{ShardingConnection, ShardingConnectionInner},
    error::{Result, ShardingError},
    execute::RawStatementExecutor,
};

pub struct ShardingTransaction {
    pub(crate) inner: std::sync::Arc<ShardingConnectionInner>,
    pub(crate) options: TransactionOptions,
    pub(crate) transactions: tokio::sync::Mutex<BTreeMap<String, DatabaseTransaction>>,
}

pub struct TwoPhaseShardingTransaction {
    inner: Arc<ShardingConnectionInner>,
    transactions: BTreeMap<String, DatabaseTransaction>,
    branch_ids: BTreeMap<String, String>,
}

pub struct PreparedTwoPhaseTransaction {
    inner: Arc<ShardingConnectionInner>,
    branch_ids: BTreeMap<String, String>,
}

#[derive(Debug)]
pub enum TwoPhaseTransactionError<E> {
    Begin(DbErr),
    Transaction(E),
    Prepare(ShardingError),
    Commit(ShardingError),
    Rollback(ShardingError),
}

impl std::fmt::Debug for ShardingTransaction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let datasources = self
            .transactions
            .try_lock()
            .map(|guard| guard.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_else(|_| vec!["<locked>".to_string()]);
        f.debug_struct("ShardingTransaction")
            .field("datasources", &datasources)
            .finish()
    }
}

impl std::fmt::Debug for TwoPhaseShardingTransaction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TwoPhaseShardingTransaction")
            .field("datasources", &self.transactions.keys().collect::<Vec<_>>())
            .field("branch_ids", &self.branch_ids)
            .finish()
    }
}

impl std::fmt::Debug for PreparedTwoPhaseTransaction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedTwoPhaseTransaction")
            .field("branch_ids", &self.branch_ids)
            .finish()
    }
}

impl ShardingTransaction {
    async fn begin_from_connection(
        connection: &ShardingConnection,
        options: TransactionOptions,
    ) -> Result<Self> {
        Ok(Self {
            inner: connection.inner.clone(),
            options,
            transactions: tokio::sync::Mutex::new(BTreeMap::new()),
        })
    }

    async fn begin_nested(&self, options: TransactionOptions) -> Result<Self> {
        let mut transactions = BTreeMap::new();
        let guard = self.transactions.lock().await;
        for (datasource, transaction) in guard.iter() {
            transactions.insert(
                datasource.clone(),
                transaction.begin_with_options(options).await?,
            );
        }
        Ok(Self {
            inner: self.inner.clone(),
            options,
            transactions: tokio::sync::Mutex::new(transactions),
        })
    }

    async fn transaction_for(
        &self,
        datasource: &str,
    ) -> std::result::Result<tokio::sync::MutexGuard<'_, BTreeMap<String, DatabaseTransaction>>, DbErr>
    {
        let mut guard = self.transactions.lock().await;
        if guard.contains_key(datasource) {
            return Ok(guard);
        }
        if !guard.is_empty() {
            return Err(DbErr::Custom(format!(
                "standard sharding transaction already enlisted {:?}; touching `{datasource}` would span multiple datasources, use two_phase_transaction or saga",
                guard.keys().cloned().collect::<Vec<_>>()
            )));
        }
        let transaction = self
            .inner
            .pool
            .connection(datasource)?
            .begin_with_options(self.options)
            .await?;
        guard.insert(datasource.to_string(), transaction);
        Ok(guard)
    }
}

impl ShardingConnection {
    pub async fn two_phase_transaction<F, T, E>(
        &self,
        callback: F,
    ) -> std::result::Result<T, TwoPhaseTransactionError<E>>
    where
        F: for<'c> FnOnce(
                &'c TwoPhaseShardingTransaction,
            )
                -> Pin<Box<dyn Future<Output = std::result::Result<T, E>> + Send + 'c>>
            + Send,
        T: Send,
        E: std::fmt::Display + std::fmt::Debug + Send,
    {
        let transaction = TwoPhaseShardingTransaction::begin_from_connection(
            self,
            TransactionOptions::default(),
        )
        .await
        .map_err(|error| TwoPhaseTransactionError::Begin(error.into()))?;
        let result = callback(&transaction).await;
        match result {
            Ok(value) => {
                let prepared = transaction
                    .prepare()
                    .await
                    .map_err(TwoPhaseTransactionError::Prepare)?;
                prepared
                    .commit()
                    .await
                    .map_err(TwoPhaseTransactionError::Commit)?;
                Ok(value)
            }
            Err(error) => {
                transaction
                    .rollback_open()
                    .await
                    .map_err(TwoPhaseTransactionError::Rollback)?;
                Err(TwoPhaseTransactionError::Transaction(error))
            }
        }
    }
}

impl TwoPhaseShardingTransaction {
    async fn begin_from_connection(
        connection: &ShardingConnection,
        options: TransactionOptions,
    ) -> Result<Self> {
        let global_id = build_two_phase_global_id();
        let mut transactions = BTreeMap::new();
        let mut branch_ids = BTreeMap::new();
        for datasource in connection.inner.pool.datasource_names() {
            let transaction = connection
                .inner
                .pool
                .connection(datasource.as_str())?
                .begin_with_options(options)
                .await?;
            branch_ids.insert(
                datasource.clone(),
                format!("{}::{}", global_id, sanitize_branch_name(datasource.as_str())),
            );
            transactions.insert(datasource, transaction);
        }
        Ok(Self {
            inner: connection.inner.clone(),
            transactions,
            branch_ids,
        })
    }

    pub async fn execute_on(
        &self,
        datasource: &str,
        sql: &str,
    ) -> std::result::Result<ExecResult, DbErr> {
        self.transactions
            .get(datasource)
            .ok_or_else(|| {
                DbErr::Custom(format!("two-phase transaction datasource `{datasource}` not found"))
            })?
            .execute_unprepared(sql)
            .await
    }

    async fn rollback_open(self) -> Result<()> {
        for (_, transaction) in self.transactions {
            transaction.rollback().await?;
        }
        Ok(())
    }

    async fn prepare(self) -> Result<PreparedTwoPhaseTransaction> {
        let mut prepared = BTreeMap::<String, String>::new();
        let mut iter = self.transactions.into_iter();

        while let Some((datasource, transaction)) = iter.next() {
            let branch_id = self
                .branch_ids
                .get(datasource.as_str())
                .cloned()
                .ok_or_else(|| {
                    ShardingError::Route(format!(
                        "two-phase branch id missing for datasource `{datasource}`"
                    ))
                })?;
            let prepare_sql = format!(
                "PREPARE TRANSACTION '{}'",
                escape_literal(branch_id.as_str())
            );
            if let Err(error) = transaction.execute_unprepared(prepare_sql.as_str()).await {
                let _ = transaction.rollback().await;
                for (_, remaining) in iter {
                    let _ = remaining.rollback().await;
                }
                for (prepared_datasource, prepared_branch_id) in &prepared {
                    if let Ok(connection) =
                        self.inner.pool.connection(prepared_datasource.as_str())
                    {
                        let _ = connection
                            .execute_unprepared(
                                format!(
                                    "ROLLBACK PREPARED '{}'",
                                    escape_literal(prepared_branch_id.as_str())
                                )
                                .as_str(),
                            )
                            .await;
                    }
                }
                return Err(ShardingError::Db(error));
            }
            prepared.insert(datasource, branch_id);
        }

        Ok(PreparedTwoPhaseTransaction {
            inner: self.inner,
            branch_ids: prepared,
        })
    }
}

impl PreparedTwoPhaseTransaction {
    async fn commit(self) -> Result<()> {
        let mut errors = Vec::new();
        for (datasource, branch_id) in &self.branch_ids {
            let connection = self.inner.pool.connection(datasource.as_str())?;
            if let Err(error) = connection
                .execute_unprepared(
                    format!(
                        "COMMIT PREPARED '{}'",
                        escape_literal(branch_id.as_str())
                    )
                    .as_str(),
                )
                .await
            {
                errors.push(format!("{datasource}: {error}"));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(ShardingError::Route(format!(
                "two-phase commit left unresolved branches: {}",
                errors.join(", ")
            )))
        }
    }
}

#[async_trait::async_trait]
impl RawStatementExecutor for ShardingTransaction {
    async fn execute_for(
        &self,
        datasource: &str,
        stmt: sea_orm::Statement,
    ) -> std::result::Result<ExecResult, DbErr> {
        let transactions = self.transaction_for(datasource).await?;
        transactions
            .get(datasource)
            .ok_or_else(|| {
                DbErr::Custom(format!("transaction datasource `{datasource}` not found"))
            })?
            .execute_raw(stmt)
            .await
    }

    async fn query_one_for(
        &self,
        datasource: &str,
        stmt: sea_orm::Statement,
    ) -> std::result::Result<Option<QueryResult>, DbErr> {
        let transactions = self.transaction_for(datasource).await?;
        transactions
            .get(datasource)
            .ok_or_else(|| {
                DbErr::Custom(format!("transaction datasource `{datasource}` not found"))
            })?
            .query_one_raw(stmt)
            .await
    }

    async fn query_all_for(
        &self,
        datasource: &str,
        stmt: sea_orm::Statement,
    ) -> std::result::Result<Vec<QueryResult>, DbErr> {
        let transactions = self.transaction_for(datasource).await?;
        transactions
            .get(datasource)
            .ok_or_else(|| {
                DbErr::Custom(format!("transaction datasource `{datasource}` not found"))
            })?
            .query_all_raw(stmt)
            .await
    }
}

#[async_trait::async_trait]
impl ConnectionTrait for ShardingTransaction {
    fn get_database_backend(&self) -> DbBackend {
        self.transactions
            .try_lock()
            .ok()
            .and_then(|guard| {
                guard
                    .values()
                    .next()
                    .map(ConnectionTrait::get_database_backend)
            })
            .unwrap_or(DbBackend::Postgres)
    }

    async fn execute_raw(
        &self,
        stmt: sea_orm::Statement,
    ) -> std::result::Result<ExecResult, DbErr> {
        self.inner.execute_with_raw(self, stmt, true, None).await
    }

    async fn execute_unprepared(&self, sql: &str) -> std::result::Result<ExecResult, DbErr> {
        let stmt = sea_orm::Statement::from_string(self.get_database_backend(), sql);
        self.execute_raw(stmt).await
    }

    async fn query_one_raw(
        &self,
        stmt: sea_orm::Statement,
    ) -> std::result::Result<Option<QueryResult>, DbErr> {
        self.inner.query_one_with_raw(self, stmt, true, None).await
    }

    async fn query_all_raw(
        &self,
        stmt: sea_orm::Statement,
    ) -> std::result::Result<Vec<QueryResult>, DbErr> {
        self.inner.query_all_with_raw(self, stmt, true, None).await
    }
}

#[async_trait::async_trait]
impl TransactionTrait for ShardingConnection {
    type Transaction = ShardingTransaction;

    async fn begin(&self) -> std::result::Result<Self::Transaction, DbErr> {
        ShardingTransaction::begin_from_connection(self, TransactionOptions::default())
            .await
            .map_err(DbErr::from)
    }

    async fn begin_with_config(
        &self,
        isolation_level: Option<IsolationLevel>,
        access_mode: Option<AccessMode>,
    ) -> std::result::Result<Self::Transaction, DbErr> {
        self.begin_with_options(TransactionOptions {
            isolation_level,
            access_mode,
            sqlite_transaction_mode: None,
        })
        .await
    }

    async fn begin_with_options(
        &self,
        options: TransactionOptions,
    ) -> std::result::Result<Self::Transaction, DbErr> {
        ShardingTransaction::begin_from_connection(self, options)
            .await
            .map_err(DbErr::from)
    }

    async fn transaction<F, T, E>(&self, callback: F) -> std::result::Result<T, TransactionError<E>>
    where
        F: for<'c> FnOnce(
                &'c Self::Transaction,
            )
                -> Pin<Box<dyn Future<Output = std::result::Result<T, E>> + Send + 'c>>
            + Send,
        T: Send,
        E: std::fmt::Display + std::fmt::Debug + Send,
    {
        self.transaction_with_config(callback, None, None).await
    }

    async fn transaction_with_config<F, T, E>(
        &self,
        callback: F,
        isolation_level: Option<IsolationLevel>,
        access_mode: Option<AccessMode>,
    ) -> std::result::Result<T, TransactionError<E>>
    where
        F: for<'c> FnOnce(
                &'c Self::Transaction,
            )
                -> Pin<Box<dyn Future<Output = std::result::Result<T, E>> + Send + 'c>>
            + Send,
        T: Send,
        E: std::fmt::Display + std::fmt::Debug + Send,
    {
        let transaction = self
            .begin_with_config(isolation_level, access_mode)
            .await
            .map_err(TransactionError::Connection)?;
        let result = callback(&transaction).await;
        match result {
            Ok(value) => {
                transaction
                    .commit()
                    .await
                    .map_err(TransactionError::Connection)?;
                Ok(value)
            }
            Err(err) => {
                transaction
                    .rollback()
                    .await
                    .map_err(TransactionError::Connection)?;
                Err(TransactionError::Transaction(err))
            }
        }
    }
}

#[async_trait::async_trait]
impl TransactionTrait for ShardingTransaction {
    type Transaction = ShardingTransaction;

    async fn begin(&self) -> std::result::Result<Self::Transaction, DbErr> {
        self.begin_nested(TransactionOptions::default())
            .await
            .map_err(DbErr::from)
    }

    async fn begin_with_config(
        &self,
        isolation_level: Option<IsolationLevel>,
        access_mode: Option<AccessMode>,
    ) -> std::result::Result<Self::Transaction, DbErr> {
        self.begin_with_options(TransactionOptions {
            isolation_level,
            access_mode,
            sqlite_transaction_mode: None,
        })
        .await
    }

    async fn begin_with_options(
        &self,
        options: TransactionOptions,
    ) -> std::result::Result<Self::Transaction, DbErr> {
        self.begin_nested(options).await.map_err(DbErr::from)
    }

    async fn transaction<F, T, E>(&self, callback: F) -> std::result::Result<T, TransactionError<E>>
    where
        F: for<'c> FnOnce(
                &'c Self::Transaction,
            )
                -> Pin<Box<dyn Future<Output = std::result::Result<T, E>> + Send + 'c>>
            + Send,
        T: Send,
        E: std::fmt::Display + std::fmt::Debug + Send,
    {
        self.transaction_with_config(callback, None, None).await
    }

    async fn transaction_with_config<F, T, E>(
        &self,
        callback: F,
        isolation_level: Option<IsolationLevel>,
        access_mode: Option<AccessMode>,
    ) -> std::result::Result<T, TransactionError<E>>
    where
        F: for<'c> FnOnce(
                &'c Self::Transaction,
            )
                -> Pin<Box<dyn Future<Output = std::result::Result<T, E>> + Send + 'c>>
            + Send,
        T: Send,
        E: std::fmt::Display + std::fmt::Debug + Send,
    {
        let transaction = self
            .begin_with_config(isolation_level, access_mode)
            .await
            .map_err(TransactionError::Connection)?;
        let result = callback(&transaction).await;
        match result {
            Ok(value) => {
                transaction
                    .commit()
                    .await
                    .map_err(TransactionError::Connection)?;
                Ok(value)
            }
            Err(err) => {
                transaction
                    .rollback()
                    .await
                    .map_err(TransactionError::Connection)?;
                Err(TransactionError::Transaction(err))
            }
        }
    }
}

#[async_trait::async_trait]
impl TransactionSession for ShardingTransaction {
    async fn commit(self) -> std::result::Result<(), DbErr> {
        for (_, transaction) in self.transactions.into_inner() {
            transaction.commit().await?;
        }
        Ok(())
    }

    async fn rollback(self) -> std::result::Result<(), DbErr> {
        for (_, transaction) in self.transactions.into_inner() {
            transaction.rollback().await?;
        }
        Ok(())
    }
}

fn build_two_phase_global_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let micros = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_micros())
        .unwrap_or_default();
    format!("summer_sharding_2pc_{micros}")
}

fn sanitize_branch_name(value: &str) -> String {
    value.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn escape_literal(value: &str) -> String {
    value.replace('\'', "''")
}

#[cfg(test)]
mod saga_tests {
    use std::{collections::BTreeMap, sync::Arc};

    use chrono::Utc;
    use parking_lot::Mutex;
    use rand::random;
    use sea_orm::{
        ConnectionTrait, Database, DbBackend, MockDatabase, Statement, TransactionError,
        TransactionTrait,
    };
    use tokio::sync::Barrier;

    use super::{SagaContext, SagaCoordinator, SagaStep};
    use crate::{
        cdc::test_support::PreparedTransactionTestDatabases,
        config::{DataSourceConfig, DataSourceRole, ShardingConfig, TenantIsolationLevel},
        connector::ShardingConnection,
        datasource::DataSourcePool,
        error::ShardingError,
        tenant::{TenantContext, with_tenant},
    };

    struct DummyContext {
        log: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait::async_trait]
    impl SagaContext for DummyContext {
        async fn execute(&self, sql: &str) -> crate::error::Result<()> {
            self.log.lock().push(sql.to_string());
            Ok(())
        }
    }

    #[derive(Debug)]
    struct RecordingStep {
        name: String,
        fail: bool,
    }

    impl RecordingStep {
        fn new(name: &str, fail: bool) -> Self {
            Self {
                name: name.to_string(),
                fail,
            }
        }
    }

    #[async_trait::async_trait]
    impl SagaStep for RecordingStep {
        fn name(&self) -> &str {
            &self.name
        }

        async fn execute(&self, ctx: &dyn SagaContext) -> crate::error::Result<()> {
            ctx.execute(&format!("execute:{}", self.name)).await?;
            if self.fail {
                Err(ShardingError::Unsupported(format!("{} failed", self.name)))
            } else {
                Ok(())
            }
        }

        async fn compensate(&self, ctx: &dyn SagaContext) -> crate::error::Result<()> {
            ctx.execute(&format!("compensate:{}", self.name)).await
        }
    }

    #[tokio::test]
    async fn saga_runs_compensation_on_failure() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let context = DummyContext { log: log.clone() };
        let steps = vec![
            Arc::new(RecordingStep::new("step1", false)) as Arc<dyn SagaStep>,
            Arc::new(RecordingStep::new("step2", true)) as Arc<dyn SagaStep>,
        ];
        let coordinator = SagaCoordinator::new(steps);
        let err = coordinator
            .execute(&context)
            .await
            .expect_err("should fail");
        assert!(matches!(err, ShardingError::Unsupported(_)));
        let history = log.lock().clone();
        assert_eq!(
            history,
            vec![
                "execute:step1".to_string(),
                "execute:step2".to_string(),
                "compensate:step1".to_string()
            ]
        );
    }

    #[tokio::test]
    async fn begin_is_lazy_and_does_not_eagerly_enlist_pool_datasources() {
        let mut datasources = BTreeMap::new();
        datasources.insert(
            "ds_primary".to_string(),
            DataSourceConfig {
                uri: "mock://primary".to_string(),
                schema: Some("test".to_string()),
                role: DataSourceRole::Primary,
                weight: 1,
            },
        );

        let config = Arc::new(ShardingConfig {
            datasources,
            ..Default::default()
        });

        let primary = MockDatabase::new(DbBackend::Postgres).into_connection();
        let tenant_db = MockDatabase::new(DbBackend::Postgres).into_connection();
        let pool = DataSourcePool::from_connections(
            config.clone(),
            BTreeMap::from([
                ("ds_primary".to_string(), primary),
                ("tenant_tseeddb".to_string(), tenant_db),
            ]),
        )
        .expect("pool");
        let sharding = ShardingConnection::with_pool(config, pool).expect("sharding");

        let transaction = sharding.begin().await.expect("begin");
        let enlisted = transaction.transactions.lock().await;
        assert!(enlisted.is_empty());
    }

    #[tokio::test]
    async fn standard_transaction_rejects_touching_multiple_datasources() {
        let mut datasources = BTreeMap::new();
        datasources.insert(
            "ds_primary".to_string(),
            DataSourceConfig {
                uri: "mock://primary".to_string(),
                schema: Some("test".to_string()),
                role: DataSourceRole::Primary,
                weight: 1,
            },
        );

        let config = Arc::new(ShardingConfig {
            datasources,
            ..Default::default()
        });

        let primary = MockDatabase::new(DbBackend::Postgres)
            .append_exec_results([sea_orm::MockExecResult {
                last_insert_id: 0,
                rows_affected: 1,
            }])
            .into_connection();
        let tenant_db = MockDatabase::new(DbBackend::Postgres).into_connection();
        let pool = DataSourcePool::from_connections(
            config.clone(),
            BTreeMap::from([
                ("ds_primary".to_string(), primary),
                ("tenant_tseeddb".to_string(), tenant_db),
            ]),
        )
        .expect("pool");
        let sharding = ShardingConnection::with_pool(config, pool).expect("sharding");

        let transaction = sharding.begin().await.expect("begin");
        crate::execute::RawStatementExecutor::execute_for(
            &transaction,
            "ds_primary",
            Statement::from_string(DbBackend::Postgres, "SELECT 1"),
        )
        .await
        .expect("execute on primary");

        let error = crate::execute::RawStatementExecutor::execute_for(
            &transaction,
            "tenant_tseeddb",
            Statement::from_string(DbBackend::Postgres, "SELECT 1"),
        )
        .await
        .expect_err("second datasource should be rejected");

        assert!(error
            .to_string()
            .contains("would span multiple datasources"));
    }

    #[tokio::test]
    async fn standard_transaction_rejects_concurrent_first_touch_on_different_datasources() {
        let mut datasources = BTreeMap::new();
        datasources.insert(
            "ds_primary".to_string(),
            DataSourceConfig {
                uri: "mock://primary".to_string(),
                schema: Some("test".to_string()),
                role: DataSourceRole::Primary,
                weight: 1,
            },
        );

        let config = Arc::new(ShardingConfig {
            datasources,
            ..Default::default()
        });

        let primary = MockDatabase::new(DbBackend::Postgres)
            .append_exec_results([sea_orm::MockExecResult {
                last_insert_id: 0,
                rows_affected: 1,
            }])
            .into_connection();
        let tenant_db = MockDatabase::new(DbBackend::Postgres)
            .append_exec_results([sea_orm::MockExecResult {
                last_insert_id: 0,
                rows_affected: 1,
            }])
            .into_connection();
        let pool = DataSourcePool::from_connections(
            config.clone(),
            BTreeMap::from([
                ("ds_primary".to_string(), primary),
                ("tenant_tseeddb".to_string(), tenant_db),
            ]),
        )
        .expect("pool");
        let sharding = ShardingConnection::with_pool(config, pool).expect("sharding");

        let transaction = Arc::new(sharding.begin().await.expect("begin"));
        let barrier = Arc::new(Barrier::new(2));

        let primary_task = {
            let transaction = transaction.clone();
            let barrier = barrier.clone();
            async move {
                barrier.wait().await;
                crate::execute::RawStatementExecutor::execute_for(
                    transaction.as_ref(),
                    "ds_primary",
                    Statement::from_string(DbBackend::Postgres, "SELECT 1"),
                )
                .await
            }
        };
        let secondary_task = {
            let transaction = transaction.clone();
            let barrier = barrier.clone();
            async move {
                barrier.wait().await;
                crate::execute::RawStatementExecutor::execute_for(
                    transaction.as_ref(),
                    "tenant_tseeddb",
                    Statement::from_string(DbBackend::Postgres, "SELECT 1"),
                )
                .await
            }
        };

        let (first, second) = tokio::join!(primary_task, secondary_task);
        let (errors, oks) = match (first, second) {
            (Ok(left), Err(right)) | (Err(right), Ok(left)) => (vec![right], vec![left]),
            (left, right) => panic!("expected one success and one failure, got {left:?} and {right:?}"),
        };

        assert_eq!(oks.len(), 1);
        assert_eq!(errors.len(), 1);
        assert!(errors[0]
            .to_string()
            .contains("would span multiple datasources"));
        assert_eq!(transaction.transactions.lock().await.len(), 1);
    }

    fn e2e_database_url() -> String {
        std::env::var("SUMMER_SHARDING_E2E_DATABASE_URL")
            .or_else(|_| std::env::var("DATABASE_URL"))
            .unwrap_or_else(|_| {
                "postgres://admin:123456@localhost/summerrs-admin?options=-c%20TimeZone%3DAsia%2FShanghai"
                    .to_string()
            })
    }

    fn e2e_replica_database_url() -> String {
        std::env::var("SUMMER_SHARDING_E2E_REPLICA_DATABASE_URL").unwrap_or_else(|_| {
            "postgres://admin:123456@localhost/summerrs_admin_sharding_e2e?options=-c%20TimeZone%3DAsia%2FShanghai"
                .to_string()
        })
    }

    async fn prepare_real_transaction_probe_tables(
        primary_url: &str,
        tenant_url: &str,
        table: &str,
    ) {
        let primary = Database::connect(primary_url).await.expect("connect primary");
        let tenant = Database::connect(tenant_url).await.expect("connect tenant");
        primary
            .execute_unprepared("CREATE SCHEMA IF NOT EXISTS test;")
            .await
            .expect("create primary test schema");
        tenant
            .execute_unprepared("CREATE SCHEMA IF NOT EXISTS test;")
            .await
            .expect("create tenant test schema");
        let ddl = format!(
            "CREATE TABLE IF NOT EXISTS test.{table} (
                id BIGINT PRIMARY KEY,
                payload VARCHAR(255) NOT NULL,
                create_time TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
            );"
        );
        primary
            .execute_unprepared(ddl.as_str())
            .await
            .expect("create primary probe table");
        tenant
            .execute_unprepared(ddl.as_str())
            .await
            .expect("create tenant probe table");
    }

    async fn cleanup_real_transaction_probe_rows(
        primary_url: &str,
        tenant_url: &str,
        table: &str,
        primary_id: i64,
        tenant_id: i64,
    ) {
        let primary = Database::connect(primary_url).await.expect("connect primary");
        let tenant = Database::connect(tenant_url).await.expect("connect tenant");
        primary
            .execute_unprepared(
                format!("DELETE FROM test.{table} WHERE id IN ({primary_id}, {tenant_id});")
                    .as_str(),
            )
            .await
            .expect("cleanup primary rows");
        tenant
            .execute_unprepared(
                format!("DELETE FROM test.{table} WHERE id IN ({primary_id}, {tenant_id});")
                    .as_str(),
            )
            .await
            .expect("cleanup tenant rows");
    }

    async fn count_probe_rows(database_url: &str, table: &str, row_id: i64) -> i64 {
        let connection = Database::connect(database_url).await.expect("connect count db");
        let row = connection
            .query_one_raw(Statement::from_string(
                DbBackend::Postgres,
                format!("SELECT COUNT(*) AS count FROM test.{table} WHERE id = {row_id}"),
            ))
            .await
            .expect("count query")
            .expect("count row");
        row.try_get("", "count").expect("count")
    }

    #[tokio::test]
    #[ignore = "requires local PostgreSQL separate-database tenant metadata data"]
    async fn sharding_transaction_commits_across_primary_and_tenant_database() {
        let primary_url = e2e_database_url();
        let tenant_url = e2e_replica_database_url();
        let suffix = Utc::now().timestamp_micros().unsigned_abs() * 1000 + u64::from(random::<u16>());
        let table = format!("dist_tx_probe_{suffix}");
        let primary_id = suffix as i64;
        let tenant_id = primary_id + 1;

        prepare_real_transaction_probe_tables(&primary_url, &tenant_url, table.as_str()).await;
        cleanup_real_transaction_probe_rows(
            &primary_url,
            &tenant_url,
            table.as_str(),
            primary_id,
            tenant_id,
        )
        .await;

        let config = ShardingConfig::from_test_str(
            format!(
                r#"
                [datasources.ds_test]
                uri = "{primary_url}"
                schema = "test"
                role = "primary"

                [tenant]
                enabled = true
                default_isolation = "shared_row"

                [tenant.row_level]
                column_name = "tenant_id"
                strategy = "sql_rewrite"
                "#
            )
            .as_str(),
        )
        .expect("config");

        let sharding = ShardingConnection::build(config)
            .await
            .expect("build sharding connection");
        let metadata_connection = Database::connect(&primary_url)
            .await
            .expect("connect metadata db");
        sharding
            .reload_tenant_metadata(&metadata_connection)
            .await
            .expect("reload tenant metadata");

        let tx = sharding.begin().await.expect("begin");
        tx.execute_unprepared(
            format!(
                "INSERT INTO test.{table}(id, payload) VALUES ({primary_id}, 'primary-commit')"
            )
            .as_str(),
        )
        .await
        .expect("insert primary");
        with_tenant(
            TenantContext::new("T-SEED-DB", TenantIsolationLevel::SharedRow),
            tx.execute_unprepared(
                format!(
                    "INSERT INTO test.{table}(id, payload) VALUES ({tenant_id}, 'tenant-commit')"
                )
                .as_str(),
            ),
        )
        .await
        .expect("insert tenant");
        sea_orm::TransactionSession::commit(tx)
            .await
            .expect("commit");

        assert_eq!(count_probe_rows(&primary_url, table.as_str(), primary_id).await, 1);
        assert_eq!(count_probe_rows(&tenant_url, table.as_str(), tenant_id).await, 1);

        cleanup_real_transaction_probe_rows(
            &primary_url,
            &tenant_url,
            table.as_str(),
            primary_id,
            tenant_id,
        )
        .await;
    }

    #[tokio::test]
    #[ignore = "requires local PostgreSQL separate-database tenant metadata data"]
    async fn sharding_transaction_rolls_back_across_primary_and_tenant_database() {
        let primary_url = e2e_database_url();
        let tenant_url = e2e_replica_database_url();
        let suffix = Utc::now().timestamp_micros().unsigned_abs() * 1000 + u64::from(random::<u16>());
        let table = format!("dist_tx_probe_{suffix}");
        let primary_id = suffix as i64;
        let tenant_id = primary_id + 1;

        prepare_real_transaction_probe_tables(&primary_url, &tenant_url, table.as_str()).await;
        cleanup_real_transaction_probe_rows(
            &primary_url,
            &tenant_url,
            table.as_str(),
            primary_id,
            tenant_id,
        )
        .await;

        let config = ShardingConfig::from_test_str(
            format!(
                r#"
                [datasources.ds_test]
                uri = "{primary_url}"
                schema = "test"
                role = "primary"

                [tenant]
                enabled = true
                default_isolation = "shared_row"

                [tenant.row_level]
                column_name = "tenant_id"
                strategy = "sql_rewrite"
                "#
            )
            .as_str(),
        )
        .expect("config");

        let sharding = ShardingConnection::build(config)
            .await
            .expect("build sharding connection");
        let metadata_connection = Database::connect(&primary_url)
            .await
            .expect("connect metadata db");
        sharding
            .reload_tenant_metadata(&metadata_connection)
            .await
            .expect("reload tenant metadata");

        let primary_insert_sql =
            format!("INSERT INTO test.{table}(id, payload) VALUES ({primary_id}, 'primary-rollback')");
        let tenant_insert_sql =
            format!("INSERT INTO test.{table}(id, payload) VALUES ({tenant_id}, 'tenant-rollback')");
        let err = sharding
            .transaction::<_, (), String>(|tx| {
                let primary_insert_sql = primary_insert_sql.clone();
                let tenant_insert_sql = tenant_insert_sql.clone();
                Box::pin(async move {
                    tx.execute_unprepared(primary_insert_sql.as_str())
                    .await
                    .map_err(|error| error.to_string())?;
                    with_tenant(
                        TenantContext::new("T-SEED-DB", TenantIsolationLevel::SharedRow),
                        tx.execute_unprepared(tenant_insert_sql.as_str()),
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                    Err("force rollback".to_string())
                })
            })
            .await
            .expect_err("transaction must roll back");

        assert!(matches!(err, TransactionError::Transaction(message) if message == "force rollback"));
        assert_eq!(count_probe_rows(&primary_url, table.as_str(), primary_id).await, 0);
        assert_eq!(count_probe_rows(&tenant_url, table.as_str(), tenant_id).await, 0);
    }

    #[tokio::test]
    #[ignore = "requires docker or SUMMER_SHARDING_2PC_E2E_DATABASE_URL_A/B"]
    async fn sharding_connection_two_phase_transaction_commits_across_real_databases() {
        let test_dbs = PreparedTransactionTestDatabases::start()
            .await
            .expect("start prepared transaction test databases");
        let primary_url = test_dbs.primary_database_url().to_string();
        let secondary_url = test_dbs.secondary_database_url().to_string();
        let suffix = Utc::now().timestamp_micros().unsigned_abs() * 1000 + u64::from(random::<u16>());
        let table = format!("two_phase_probe_{suffix}");
        let primary_id = suffix as i64;
        let secondary_id = primary_id + 1;

        prepare_real_transaction_probe_tables(&primary_url, &secondary_url, table.as_str()).await;
        cleanup_real_transaction_probe_rows(
            &primary_url,
            &secondary_url,
            table.as_str(),
            primary_id,
            secondary_id,
        )
        .await;

        let config = ShardingConfig::from_test_str(
            format!(
                r#"
                [datasources.ds_primary]
                uri = "{primary_url}"
                schema = "test"
                role = "primary"

                [datasources.ds_secondary]
                uri = "{secondary_url}"
                schema = "test"
                role = "primary"
                "#
            )
            .as_str(),
        )
        .expect("config");
        let sharding = ShardingConnection::build(config)
            .await
            .expect("build sharding connection");

        sharding
            .two_phase_transaction::<_, (), String>(|tx| {
                let primary_sql = format!(
                    "INSERT INTO test.{table}(id, payload) VALUES ({primary_id}, 'two-phase-primary')"
                );
                let secondary_sql = format!(
                    "INSERT INTO test.{table}(id, payload) VALUES ({secondary_id}, 'two-phase-secondary')"
                );
                Box::pin(async move {
                    tx.execute_on("ds_primary", primary_sql.as_str())
                        .await
                        .map_err(|error| error.to_string())?;
                    tx.execute_on("ds_secondary", secondary_sql.as_str())
                        .await
                        .map_err(|error| error.to_string())?;
                    Ok(())
                })
            })
            .await
            .expect("two phase commit");

        assert_eq!(count_probe_rows(&primary_url, table.as_str(), primary_id).await, 1);
        assert_eq!(count_probe_rows(&secondary_url, table.as_str(), secondary_id).await, 1);
    }

    #[tokio::test]
    #[ignore = "requires docker or SUMMER_SHARDING_2PC_E2E_DATABASE_URL_A/B"]
    async fn sharding_connection_two_phase_transaction_rolls_back_on_callback_error() {
        let test_dbs = PreparedTransactionTestDatabases::start()
            .await
            .expect("start prepared transaction test databases");
        let primary_url = test_dbs.primary_database_url().to_string();
        let secondary_url = test_dbs.secondary_database_url().to_string();
        let suffix = Utc::now().timestamp_micros().unsigned_abs() * 1000 + u64::from(random::<u16>());
        let table = format!("two_phase_probe_{suffix}");
        let primary_id = suffix as i64;
        let secondary_id = primary_id + 1;

        prepare_real_transaction_probe_tables(&primary_url, &secondary_url, table.as_str()).await;
        cleanup_real_transaction_probe_rows(
            &primary_url,
            &secondary_url,
            table.as_str(),
            primary_id,
            secondary_id,
        )
        .await;

        let config = ShardingConfig::from_test_str(
            format!(
                r#"
                [datasources.ds_primary]
                uri = "{primary_url}"
                schema = "test"
                role = "primary"

                [datasources.ds_secondary]
                uri = "{secondary_url}"
                schema = "test"
                role = "primary"
                "#
            )
            .as_str(),
        )
        .expect("config");
        let sharding = ShardingConnection::build(config)
            .await
            .expect("build sharding connection");

        let err = sharding
            .two_phase_transaction::<_, (), String>(|tx| {
                let primary_sql = format!(
                    "INSERT INTO test.{table}(id, payload) VALUES ({primary_id}, 'two-phase-primary')"
                );
                let secondary_sql = format!(
                    "INSERT INTO test.{table}(id, payload) VALUES ({secondary_id}, 'two-phase-secondary')"
                );
                Box::pin(async move {
                    tx.execute_on("ds_primary", primary_sql.as_str())
                        .await
                        .map_err(|error| error.to_string())?;
                    tx.execute_on("ds_secondary", secondary_sql.as_str())
                        .await
                        .map_err(|error| error.to_string())?;
                    Err("force two phase rollback".to_string())
                })
            })
            .await
            .expect_err("two phase transaction must roll back");

        assert!(matches!(err, crate::connector::TwoPhaseTransactionError::Transaction(message) if message == "force two phase rollback"));
        assert_eq!(count_probe_rows(&primary_url, table.as_str(), primary_id).await, 0);
        assert_eq!(count_probe_rows(&secondary_url, table.as_str(), secondary_id).await, 0);
    }
}

#[async_trait::async_trait]
pub trait SagaContext: Send + Sync {
    async fn execute(&self, sql: &str) -> Result<()>;
}

#[async_trait::async_trait]
impl SagaContext for ShardingTransaction {
    async fn execute(&self, sql: &str) -> Result<()> {
        let transactions = self.transactions.lock().await;
        for transaction in transactions.values() {
            transaction.execute_unprepared(sql).await?;
        }
        Ok(())
    }
}

#[async_trait::async_trait]
pub trait SagaStep: Send + Sync {
    fn name(&self) -> &str;
    async fn execute(&self, ctx: &dyn SagaContext) -> Result<()>;
    async fn compensate(&self, ctx: &dyn SagaContext) -> Result<()>;
}

pub struct SagaCoordinator {
    steps: Vec<Arc<dyn SagaStep>>,
}

impl SagaCoordinator {
    pub fn new(steps: Vec<Arc<dyn SagaStep>>) -> Self {
        Self { steps }
    }

    pub async fn execute(&self, ctx: &dyn SagaContext) -> Result<()> {
        let mut completed: Vec<Arc<dyn SagaStep>> = Vec::new();
        for step in &self.steps {
            if let Err(err) = step.execute(ctx).await {
                for executed in completed.iter().rev() {
                    let _ = executed.compensate(ctx).await;
                }
                return Err(err);
            }
            completed.push(step.clone());
        }
        Ok(())
    }
}
