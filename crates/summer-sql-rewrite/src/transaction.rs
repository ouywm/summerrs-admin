use std::{future::Future, pin::Pin, sync::Arc};

use sea_orm::{
    AccessMode, ConnectionTrait, DatabaseTransaction, DbBackend, DbErr, ExecResult, IsolationLevel,
    QueryResult, Statement, StreamTrait, TransactionError, TransactionOptions, TransactionSession,
    TransactionTrait,
};

use crate::{extensions::Extensions, pipeline, registry::PluginRegistry};

#[derive(Debug)]
pub struct RewriteTransaction {
    inner: DatabaseTransaction,
    registry: Arc<PluginRegistry>,
    extensions: Extensions,
}

impl RewriteTransaction {
    pub fn new(
        inner: DatabaseTransaction,
        registry: Arc<PluginRegistry>,
        extensions: Extensions,
    ) -> Self {
        Self {
            inner,
            registry,
            extensions,
        }
    }

    pub async fn commit(self) -> Result<(), DbErr> {
        self.inner.commit().await
    }

    pub async fn rollback(self) -> Result<(), DbErr> {
        self.inner.rollback().await
    }

    pub(crate) async fn run<F, T, E>(self, callback: F) -> Result<T, TransactionError<E>>
    where
        F: for<'b> FnOnce(
                &'b RewriteTransaction,
            ) -> Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'b>>
            + Send,
        T: Send,
        E: std::fmt::Display + std::fmt::Debug + Send,
    {
        let result = callback(&self).await.map_err(TransactionError::Transaction);
        if result.is_ok() {
            self.commit().await.map_err(TransactionError::Connection)?;
        } else {
            self.rollback()
                .await
                .map_err(TransactionError::Connection)?;
        }
        result
    }
}

#[async_trait::async_trait]
impl ConnectionTrait for RewriteTransaction {
    fn get_database_backend(&self) -> DbBackend {
        ConnectionTrait::get_database_backend(&self.inner)
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

impl StreamTrait for RewriteTransaction {
    type Stream<'a>
        = <DatabaseTransaction as StreamTrait>::Stream<'a>
    where
        Self: 'a;

    fn get_database_backend(&self) -> DbBackend {
        StreamTrait::get_database_backend(&self.inner)
    }

    fn stream_raw<'a>(
        &'a self,
        stmt: Statement,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Stream<'a>, DbErr>> + 'a + Send>> {
        Box::pin(async move {
            let rewritten =
                pipeline::rewrite_statement(stmt, self.registry.as_ref(), &self.extensions)?;
            self.inner.stream_raw(rewritten).await
        })
    }
}

#[async_trait::async_trait]
impl TransactionTrait for RewriteTransaction {
    type Transaction = RewriteTransaction;

    async fn begin(&self) -> Result<Self::Transaction, DbErr> {
        Ok(Self::new(
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
        Ok(Self::new(
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
        Ok(Self::new(
            self.inner.begin_with_options(options).await?,
            self.registry.clone(),
            self.extensions.clone(),
        ))
    }

    async fn transaction<F, T, E>(&self, callback: F) -> Result<T, TransactionError<E>>
    where
        F: for<'c> FnOnce(
                &'c Self::Transaction,
            ) -> Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'c>>
            + Send,
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
            ) -> Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'c>>
            + Send,
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

#[async_trait::async_trait]
impl TransactionSession for RewriteTransaction {
    async fn commit(self) -> Result<(), DbErr> {
        self.commit().await
    }

    async fn rollback(self) -> Result<(), DbErr> {
        self.rollback().await
    }
}
