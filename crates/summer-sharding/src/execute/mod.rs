mod scatter_gather;
mod simple;

use async_trait::async_trait;
use sea_orm::{DbErr, ExecResult, QueryResult, Statement};

use crate::{
    connector::statement::StatementContext,
    error::{Result, ShardingError},
    merge::ResultMerger,
    router::RoutePlan,
};

pub use scatter_gather::ScatterGatherExecutor;
pub use simple::SimpleExecutor;

#[derive(Debug, Clone)]
pub struct ExecutionUnit {
    pub datasource: String,
    pub statement: Statement,
}

#[async_trait]
pub trait RawStatementExecutor: Send + Sync {
    async fn execute_for(
        &self,
        datasource: &str,
        stmt: Statement,
    ) -> std::result::Result<ExecResult, DbErr>;
    async fn query_one_for(
        &self,
        datasource: &str,
        stmt: Statement,
    ) -> std::result::Result<Option<QueryResult>, DbErr>;
    async fn query_all_for(
        &self,
        datasource: &str,
        stmt: Statement,
    ) -> std::result::Result<Vec<QueryResult>, DbErr>;
}

#[async_trait]
pub trait Executor: Send + Sync + 'static {
    async fn execute(
        &self,
        raw: &dyn RawStatementExecutor,
        units: Vec<ExecutionUnit>,
    ) -> Result<ExecResult>;

    async fn query_one(
        &self,
        raw: &dyn RawStatementExecutor,
        units: Vec<ExecutionUnit>,
        analysis: &StatementContext,
        plan: &RoutePlan,
        merger: &dyn ResultMerger,
    ) -> Result<Option<QueryResult>>;

    async fn query_all(
        &self,
        raw: &dyn RawStatementExecutor,
        units: Vec<ExecutionUnit>,
        analysis: &StatementContext,
        plan: &RoutePlan,
        merger: &dyn ResultMerger,
    ) -> Result<Vec<QueryResult>>;
}

pub(crate) fn ensure_units(units: &[ExecutionUnit]) -> Result<()> {
    if units.is_empty() {
        return Err(ShardingError::Route(
            "no execution units were produced for the routed statement".to_string(),
        ));
    }
    Ok(())
}
