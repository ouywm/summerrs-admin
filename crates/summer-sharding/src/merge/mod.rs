mod limit;
mod order_by;

use std::sync::Arc;

use sea_orm::QueryResult;

use crate::{
    config::ShardingConfig, connector::statement::StatementContext, error::Result,
    router::RoutePlan,
};

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
    #[allow(dead_code)]
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
        _analysis: &StatementContext,
        plan: &RoutePlan,
    ) -> Result<Vec<QueryResult>> {
        if !plan.order_by.is_empty() {
            let mut all_rows: Vec<QueryResult> = shards.into_iter().flatten().collect();
            all_rows = order_by::merge(all_rows, &plan.order_by);
            let offset = plan.offset.unwrap_or(0) as usize;
            let limit = plan.limit.map(|v| v as usize);
            let mut rows = Vec::new();
            for row in all_rows.into_iter().skip(offset) {
                rows.push(row);
                if limit.is_some_and(|l| rows.len() >= l) {
                    break;
                }
            }
            Ok(rows)
        } else {
            let rows: Vec<QueryResult> = shards.into_iter().flatten().collect();
            Ok(limit::apply(rows, plan.limit, plan.offset))
        }
    }
}
