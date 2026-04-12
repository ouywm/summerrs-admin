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
    config: ShardingConfig,
}

impl DefaultResultMerger {
    pub fn new(config: Arc<ShardingConfig>) -> Self {
        Self {
            config: config.as_ref().clone(),
        }
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
        } else if !plan.order_by.is_empty() {
            let mut stream = stream::MergedRowStream::from_sorted_shards(shards, &plan.order_by);
            let mut rows = Vec::new();
            let offset = plan.offset.unwrap_or(0) as usize;
            for _ in 0..offset {
                if stream.next().is_none() {
                    return post_process::apply(rows, analysis, &self.config);
                }
            }
            let limit = plan.limit.map(|value| value as usize);
            for row in stream {
                rows.push(row);
                if limit.is_some_and(|limit| rows.len() >= limit) {
                    break;
                }
            }
            rows
        } else {
            shards.into_iter().flatten().collect()
        };
        let rows = if plan.order_by.is_empty() {
            limit::apply(rows, plan.limit, plan.offset)
        } else {
            rows
        };
        let rows = if plan.order_by.is_empty() {
            rows
        } else {
            order_by::merge(rows, &[])
        };
        post_process::apply(rows, analysis, &self.config)
    }
}
