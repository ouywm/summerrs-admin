use super::ShardingConnectionInner;
use crate::{
    audit::AuditEvent,
    connector::statement::StatementContext,
    datasource::{FanoutMetric, SlowQueryMetric, record_fanout, record_slow_query},
    router::RoutePlan,
};

impl ShardingConnectionInner {
    pub(super) fn audit(
        &self,
        sql: String,
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
        self.auditor.record(AuditEvent {
            sql,
            duration_ms,
            route: plan.clone(),
            is_slow_query: duration_ms >= self.config.audit.slow_query_threshold_ms as u128,
            full_scatter: self.config.audit.log_full_scatter && plan.targets.len() > 1,
            missing_sharding_key: self.config.audit.log_no_sharding_key
                && !analysis.has_sharding_key(),
        });
    }
}
