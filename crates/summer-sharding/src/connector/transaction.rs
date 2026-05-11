use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

use sea_orm::{
    AccessMode, ConnectionTrait, DatabaseTransaction, DbBackend, DbErr, ExecResult, IsolationLevel,
    QueryResult, TransactionError, TransactionOptions, TransactionSession, TransactionTrait,
};

use crate::{
    connector::connection::{ExecutionOverrides, ShardingConnection, ShardingConnectionInner},
    error::{Result, ShardingError},
    execute::RawStatementExecutor,
};

pub struct ShardingTransaction {
    pub(crate) inner: std::sync::Arc<ShardingConnectionInner>,
    pub(crate) options: TransactionOptions,
    pub(crate) transactions: Arc<tokio::sync::Mutex<BTreeMap<String, DatabaseTransaction>>>,
    pub(crate) access_context_override: Option<crate::connector::ShardingAccessContext>,
    pub(crate) tenant_override: Option<crate::tenant::TenantContext>,
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
    fn execution_overrides(&self) -> ExecutionOverrides {
        ExecutionOverrides {
            hint: None,
            access_context: self.access_context_override.clone(),
            tenant: self.tenant_override.clone(),
        }
    }

    async fn begin_from_connection(
        connection: &ShardingConnection,
        options: TransactionOptions,
    ) -> Result<Self> {
        Ok(Self {
            inner: connection.inner.clone(),
            options,
            transactions: Arc::new(tokio::sync::Mutex::new(BTreeMap::new())),
            access_context_override: connection.access_context_override.clone(),
            tenant_override: connection.tenant_override.clone(),
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
            transactions: Arc::new(tokio::sync::Mutex::new(transactions)),
            access_context_override: self.access_context_override.clone(),
            tenant_override: self.tenant_override.clone(),
        })
    }

    async fn transaction_for(
        &self,
        datasource: &str,
    ) -> std::result::Result<
        tokio::sync::MutexGuard<'_, BTreeMap<String, DatabaseTransaction>>,
        DbErr,
    > {
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

impl ShardingTransaction {
    pub fn with_tenant_context(&self, tenant: crate::tenant::TenantContext) -> Self {
        Self {
            inner: self.inner.clone(),
            options: self.options,
            transactions: self.transactions.clone(),
            access_context_override: self.access_context_override.clone(),
            tenant_override: Some(self.inner.tenant_router.resolve_context(tenant)),
        }
    }

    pub fn with_access_context(&self, context: crate::connector::ShardingAccessContext) -> Self {
        Self {
            inner: self.inner.clone(),
            options: self.options,
            transactions: self.transactions.clone(),
            access_context_override: Some(context),
            tenant_override: self.tenant_override.clone(),
        }
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
        let transaction =
            TwoPhaseShardingTransaction::begin_from_connection(self, TransactionOptions::default())
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
                format!(
                    "{}::{}",
                    global_id,
                    sanitize_branch_name(datasource.as_str())
                ),
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
                DbErr::Custom(format!(
                    "two-phase transaction datasource `{datasource}` not found"
                ))
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
                let mut rollback_errors = Vec::new();
                for (prepared_datasource, prepared_branch_id) in &prepared {
                    if let Ok(connection) = self.inner.pool.connection(prepared_datasource.as_str())
                        && let Err(rollback_err) = connection
                            .execute_unprepared(
                                format!(
                                    "ROLLBACK PREPARED '{}'",
                                    escape_literal(prepared_branch_id.as_str())
                                )
                                .as_str(),
                            )
                            .await
                    {
                        rollback_errors.push(format!(
                            "ROLLBACK PREPARED '{}' on `{}` failed: {}",
                            prepared_branch_id, prepared_datasource, rollback_err
                        ));
                    }
                }
                if !rollback_errors.is_empty() {
                    return Err(ShardingError::Route(format!(
                        "PREPARE TRANSACTION failed ({error}) and rollback of prepared branches also failed, orphaned transactions may exist: {}",
                        rollback_errors.join("; ")
                    )));
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
        let mut committed = Vec::new();
        for (datasource, branch_id) in &self.branch_ids {
            let connection = self.inner.pool.connection(datasource.as_str())?;
            if let Err(error) = connection
                .execute_unprepared(
                    format!("COMMIT PREPARED '{}'", escape_literal(branch_id.as_str())).as_str(),
                )
                .await
            {
                // A partial commit has occurred: some branches committed, others
                // did not.  We must NOT continue committing remaining branches
                // blindly.  Instead, report the failure with details of what was
                // committed and what remains so an operator can resolve manually.
                let remaining: Vec<String> = self
                    .branch_ids
                    .iter()
                    .filter(|(ds, _)| !committed.contains(ds))
                    .map(|(ds, bid)| format!("{ds}:{bid}"))
                    .collect();
                return Err(ShardingError::Route(format!(
                    "two-phase COMMIT PREPARED failed on `{datasource}` ({error}); \
                     committed branches: [{}]; \
                     uncommitted branches requiring manual resolution: [{}]",
                    committed
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                    remaining.join(", ")
                )));
            }
            committed.push(datasource);
        }
        Ok(())
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
        self.inner
            .execute_with_raw(self, stmt, true, self.execution_overrides())
            .await
    }

    async fn execute_unprepared(&self, sql: &str) -> std::result::Result<ExecResult, DbErr> {
        let stmt = sea_orm::Statement::from_string(self.get_database_backend(), sql);
        self.execute_raw(stmt).await
    }

    async fn query_one_raw(
        &self,
        stmt: sea_orm::Statement,
    ) -> std::result::Result<Option<QueryResult>, DbErr> {
        self.inner
            .query_one_with_raw(self, stmt, true, self.execution_overrides())
            .await
    }

    async fn query_all_raw(
        &self,
        stmt: sea_orm::Statement,
    ) -> std::result::Result<Vec<QueryResult>, DbErr> {
        self.inner
            .query_all_with_raw(self, stmt, true, self.execution_overrides())
            .await
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
        let mut guard = self.transactions.lock().await;
        let transactions = std::mem::take(&mut *guard);
        drop(guard);
        for (_, transaction) in transactions {
            transaction.commit().await?;
        }
        Ok(())
    }

    async fn rollback(self) -> std::result::Result<(), DbErr> {
        let mut guard = self.transactions.lock().await;
        let transactions = std::mem::take(&mut *guard);
        drop(guard);
        for (_, transaction) in transactions {
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
    value
        .chars()
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
