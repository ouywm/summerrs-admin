use std::sync::Arc;

use sea_orm::{
    AccessMode, ConnectionTrait, DatabaseConnection, DbBackend, DbErr, ExecResult, IsolationLevel,
    QueryResult, Statement, StreamTrait, TransactionError, TransactionOptions, TransactionTrait,
};

use crate::{
    extensions::Extensions, pipeline, registry::PluginRegistry, transaction::RewriteTransaction,
};

#[derive(Debug, Clone)]
pub struct RewriteConnection {
    inner: DatabaseConnection,
    registry: Arc<PluginRegistry>,
    extensions: Extensions,
}

impl RewriteConnection {
    pub fn new(
        inner: DatabaseConnection,
        registry: impl Into<Arc<PluginRegistry>>,
        extensions: Extensions,
    ) -> Self {
        Self {
            inner,
            registry: registry.into(),
            extensions,
        }
    }

    pub fn inner(&self) -> &DatabaseConnection {
        &self.inner
    }

    pub fn registry(&self) -> &PluginRegistry {
        self.registry.as_ref()
    }

    pub fn extensions(&self) -> &Extensions {
        &self.extensions
    }

    pub fn with_extensions(&self, extensions: Extensions) -> Self {
        Self {
            inner: self.inner.clone(),
            registry: self.registry.clone(),
            extensions,
        }
    }
}

#[async_trait::async_trait]
impl ConnectionTrait for RewriteConnection {
    fn get_database_backend(&self) -> DbBackend {
        self.inner.get_database_backend()
    }

    async fn execute_raw(&self, stmt: Statement) -> Result<ExecResult, DbErr> {
        let rewritten =
            pipeline::rewrite_statement(stmt, self.registry.as_ref(), &self.extensions)?;
        self.inner.execute_raw(rewritten).await
    }

    async fn execute_unprepared(&self, sql: &str) -> Result<ExecResult, DbErr> {
        let rewritten = pipeline::rewrite_unprepared_sql(
            sql,
            ConnectionTrait::get_database_backend(self),
            self.registry.as_ref(),
            &self.extensions,
        )?;
        self.inner.execute_unprepared(&rewritten).await
    }

    async fn query_one_raw(&self, stmt: Statement) -> Result<Option<QueryResult>, DbErr> {
        let rewritten =
            pipeline::rewrite_statement(stmt, self.registry.as_ref(), &self.extensions)?;
        self.inner.query_one_raw(rewritten).await
    }

    async fn query_all_raw(&self, stmt: Statement) -> Result<Vec<QueryResult>, DbErr> {
        let rewritten =
            pipeline::rewrite_statement(stmt, self.registry.as_ref(), &self.extensions)?;
        self.inner.query_all_raw(rewritten).await
    }
}

impl StreamTrait for RewriteConnection {
    type Stream<'a>
        = <DatabaseConnection as StreamTrait>::Stream<'a>
    where
        Self: 'a;

    fn get_database_backend(&self) -> DbBackend {
        StreamTrait::get_database_backend(&self.inner)
    }

    fn stream_raw<'a>(
        &'a self,
        stmt: Statement,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self::Stream<'a>, DbErr>> + 'a + Send>,
    > {
        Box::pin(async move {
            let rewritten =
                pipeline::rewrite_statement(stmt, self.registry.as_ref(), &self.extensions)?;
            self.inner.stream_raw(rewritten).await
        })
    }
}

#[async_trait::async_trait]
impl TransactionTrait for RewriteConnection {
    type Transaction = RewriteTransaction;

    async fn begin(&self) -> Result<Self::Transaction, DbErr> {
        Ok(RewriteTransaction::new(
            self.inner.begin().await?,
            self.registry.clone(),
            self.extensions.clone(),
        ))
    }

    async fn begin_with_config(
        &self,
        isolation_level: Option<IsolationLevel>,
        access_mode: Option<AccessMode>,
    ) -> Result<Self::Transaction, DbErr> {
        Ok(RewriteTransaction::new(
            self.inner
                .begin_with_config(isolation_level, access_mode)
                .await?,
            self.registry.clone(),
            self.extensions.clone(),
        ))
    }

    async fn begin_with_options(
        &self,
        options: TransactionOptions,
    ) -> Result<Self::Transaction, DbErr> {
        Ok(RewriteTransaction::new(
            self.inner.begin_with_options(options).await?,
            self.registry.clone(),
            self.extensions.clone(),
        ))
    }

    async fn transaction<F, T, E>(&self, callback: F) -> Result<T, TransactionError<E>>
    where
        F: for<'c> FnOnce(
                &'c Self::Transaction,
            ) -> std::pin::Pin<
                Box<dyn std::future::Future<Output = Result<T, E>> + Send + 'c>,
            > + Send,
        T: Send,
        E: std::fmt::Display + std::fmt::Debug + Send,
    {
        let transaction = self.begin().await.map_err(TransactionError::Connection)?;
        transaction.run(callback).await
    }

    async fn transaction_with_config<F, T, E>(
        &self,
        callback: F,
        isolation_level: Option<IsolationLevel>,
        access_mode: Option<AccessMode>,
    ) -> Result<T, TransactionError<E>>
    where
        F: for<'c> FnOnce(
                &'c Self::Transaction,
            ) -> std::pin::Pin<
                Box<dyn std::future::Future<Output = Result<T, E>> + Send + 'c>,
            > + Send,
        T: Send,
        E: std::fmt::Display + std::fmt::Debug + Send,
    {
        let transaction = self
            .begin_with_config(isolation_level, access_mode)
            .await
            .map_err(TransactionError::Connection)?;
        transaction.run(callback).await
    }
}

#[cfg(test)]
mod tests {
    use futures::TryStreamExt;
    use sea_orm::{
        ConnectionTrait, DbBackend, DbErr, MockDatabase, MockExecResult, Statement, StreamTrait,
        TransactionError, TransactionTrait,
    };

    use crate::{
        Extensions, PluginRegistry, RewriteConnection, SqlRewriteContext, SqlRewritePlugin,
    };

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct AuditTag(&'static str);

    struct ExtensionCommentPlugin;

    impl SqlRewritePlugin for ExtensionCommentPlugin {
        fn name(&self) -> &str {
            "extension_comment"
        }

        fn matches(&self, _ctx: &SqlRewriteContext) -> bool {
            true
        }

        fn rewrite(&self, ctx: &mut SqlRewriteContext) -> crate::Result<()> {
            if let Some(tag) = ctx.extension::<AuditTag>() {
                ctx.append_comment(tag.0);
            }
            Ok(())
        }
    }

    fn rewrite_connection_with_tag(
        sql_tag: &'static str,
    ) -> (sea_orm::DatabaseConnection, RewriteConnection) {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_exec_results([MockExecResult::default()])
            .into_connection();
        let mut registry = PluginRegistry::new();
        registry.register(ExtensionCommentPlugin);
        let mut extensions = Extensions::new();
        extensions.insert(AuditTag(sql_tag));
        let conn = RewriteConnection::new(db.clone(), registry, extensions);
        (db, conn)
    }

    #[tokio::test]
    async fn transaction_commit_rewrites_sql_inside_transaction() {
        let (db, conn) = rewrite_connection_with_tag("trace=commit");

        conn.transaction::<_, (), DbErr>(|txn| {
            Box::pin(async move {
                txn.execute_raw(Statement::from_string(
                    DbBackend::Postgres,
                    "UPDATE users SET active = true",
                ))
                .await?;
                Ok(())
            })
        })
        .await
        .expect("transaction commit");

        let logs = db.into_transaction_log();
        assert_eq!(logs.len(), 1);
        let statements = logs[0].statements();
        assert_eq!(statements[0].sql, "BEGIN");
        assert!(
            statements[1]
                .sql
                .contains("UPDATE users SET active = true /* trace=commit */")
        );
        assert_eq!(statements[2].sql, "COMMIT");
    }

    #[tokio::test]
    async fn transaction_rollback_rewrites_sql_inside_transaction() {
        let (db, conn) = rewrite_connection_with_tag("trace=rollback");

        let result = conn
            .transaction::<_, (), DbErr>(|txn| {
                Box::pin(async move {
                    txn.execute_raw(Statement::from_string(
                        DbBackend::Postgres,
                        "DELETE FROM users WHERE id = 42",
                    ))
                    .await?;
                    Err(DbErr::Custom("boom".to_string()))
                })
            })
            .await;

        match result {
            Err(TransactionError::Transaction(DbErr::Custom(message))) => {
                assert_eq!(message, "boom");
            }
            other => panic!("unexpected transaction result: {other:?}"),
        }

        let logs = db.into_transaction_log();
        assert_eq!(logs.len(), 1);
        let statements = logs[0].statements();
        assert_eq!(statements[0].sql, "BEGIN");
        assert!(
            statements[1]
                .sql
                .contains("DELETE FROM users WHERE id = 42 /* trace=rollback */")
        );
        assert_eq!(statements[2].sql, "ROLLBACK");
    }

    #[test]
    fn rewrite_connection_and_transaction_implement_stream_trait() {
        fn assert_stream<T: StreamTrait>() {}

        assert_stream::<RewriteConnection>();
        assert_stream::<crate::RewriteTransaction>();
    }

    #[tokio::test]
    async fn stream_raw_rewrites_sql_before_streaming() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([[std::collections::BTreeMap::from([(
                "id".to_string(),
                1_i64.into(),
            )])]])
            .into_connection();
        let log_db = db.clone();
        let mut registry = PluginRegistry::new();
        registry.register(ExtensionCommentPlugin);
        let mut extensions = Extensions::new();
        extensions.insert(AuditTag("trace=stream"));
        let conn = RewriteConnection::new(db, registry, extensions);

        let mut stream = conn
            .stream_raw(Statement::from_string(
                DbBackend::Postgres,
                "SELECT id FROM users",
            ))
            .await
            .expect("stream");

        let first = stream.try_next().await.expect("next row");
        assert!(first.is_some());

        let logs = log_db.into_transaction_log();
        assert_eq!(logs.len(), 1);
        assert!(
            logs[0].statements()[0]
                .sql
                .contains("SELECT id FROM users /* trace=stream */")
        );
    }
}
