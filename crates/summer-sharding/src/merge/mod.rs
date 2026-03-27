mod group_by;
mod limit;
mod order_by;
mod post_process;
mod row;
mod stream;

use std::sync::Arc;

use sea_orm::QueryResult;

use crate::{
    config::ShardingConfig, connector::statement::StatementContext, error::Result,
    router::RoutePlan,
};

pub use stream::MergedRowStream;

pub trait ResultMerger: Send + Sync + 'static {
    fn merge(
        &self,
        shards: Vec<Vec<QueryResult>>,
        analysis: &StatementContext,
        plan: &RoutePlan,
    ) -> Result<Vec<QueryResult>>;
}

#[derive(Debug, Clone)]
pub struct DefaultResultMerger {
    config: Arc<ShardingConfig>,
}

impl DefaultResultMerger {
    pub fn new(config: Arc<ShardingConfig>) -> Self {
        Self { config }
    }
}

impl ResultMerger for DefaultResultMerger {
    fn merge(
        &self,
        shards: Vec<Vec<QueryResult>>,
        analysis: &StatementContext,
        plan: &RoutePlan,
    ) -> Result<Vec<QueryResult>> {
        let rows = if analysis.has_aggregate_projection() || analysis.is_grouped_query() {
            group_by::merge(shards, analysis)?
        } else {
            shards.into_iter().flatten().collect()
        };
        let rows = order_by::merge(rows, plan.order_by.as_slice());
        let rows = limit::apply(rows, plan.limit, plan.offset);
        post_process::apply(rows, analysis, self.config.as_ref())
    }
}
