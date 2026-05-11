use super::ShardingConnectionInner;
use crate::{
    connector::statement::StatementContext,
    datasource::{FanoutMetric, SlowQueryMetric, record_fanout, record_slow_query},
    router::RoutePlan,
};

impl ShardingConnectionInner {
    pub(super) fn audit(
        &self,
        _sql: String,
        analysis: &StatementContext,
        plan: &RoutePlan,
        duration_ms: u128,
    ) {
        if !self.config.audit.enabled {
            return;
        }
        let target_datasources = plan
            .targets
            .iter()
            .map(|target| target.datasource.clone())
            .collect::<Vec<_>>();
        record_fanout(FanoutMetric {
            rule_name: None,
            operation: analysis.operation,
            fanout: target_datasources.len(),
            targets: target_datasources.clone(),
        });
        if duration_ms >= self.config.audit.slow_query_threshold_ms as u128 {
            for datasource in target_datasources {
                record_slow_query(SlowQueryMetric {
                    datasource,
                    elapsed_ms: duration_ms,
                    threshold_ms: self.config.audit.slow_query_threshold_ms as u128,
                    reason: "query_execution".to_string(),
                });
            }
        }
    }
}
