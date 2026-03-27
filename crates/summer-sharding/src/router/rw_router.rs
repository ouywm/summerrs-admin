use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use crate::{
    config::{LoadBalanceKind, ShardingConfig},
    datasource::{FanoutMetric, ShardHitMetric, record_fanout, record_shard_hit, route_state},
    router::SqlOperation,
};

#[derive(Debug)]
pub struct ReadWriteRouter {
    config: Arc<ShardingConfig>,
    counters: Arc<BTreeMap<String, AtomicUsize>>,
}

impl ReadWriteRouter {
    pub fn new(config: Arc<ShardingConfig>) -> Self {
        let counters = config
            .read_write_splitting
            .rules
            .iter()
            .map(|rule| (rule.name.clone(), AtomicUsize::new(0)))
            .collect();
        Self {
            config,
            counters: Arc::new(counters),
        }
    }

    pub fn route(&self, datasource: &str, operation: SqlOperation, force_primary: bool) -> String {
        let Some(rule) = self
            .config
            .read_write_splitting
            .rules
            .iter()
            .find(|rule| rule.primary == datasource)
        else {
            return datasource.to_string();
        };

        let runtime_state = route_state(rule.primary.as_str());
        let effective_primary = runtime_state
            .as_ref()
            .and_then(|state| state.effective_write_target())
            .unwrap_or_else(|| datasource.to_string());

        if force_primary
            || operation != SqlOperation::Select
            || !self.config.read_write_splitting.enabled
        {
            self.record_runtime_metrics(
                Some(rule.name.clone()),
                operation,
                effective_primary.clone(),
                vec![effective_primary.clone()],
                runtime_state
                    .as_ref()
                    .is_some_and(|state| state.failover_active),
            );
            return effective_primary;
        }

        let replicas = runtime_state
            .as_ref()
            .map(|state| state.healthy_replicas.clone())
            .unwrap_or_else(|| rule.replicas.clone());
        if replicas.is_empty() {
            self.record_runtime_metrics(
                Some(rule.name.clone()),
                operation,
                effective_primary.clone(),
                vec![effective_primary.clone()],
                runtime_state
                    .as_ref()
                    .is_some_and(|state| state.failover_active),
            );
            return effective_primary;
        }

        let selected = match rule.load_balance {
            LoadBalanceKind::RoundRobin => {
                let index = self
                    .counters
                    .get(rule.name.as_str())
                    .map(|counter| counter.fetch_add(1, Ordering::Relaxed))
                    .unwrap_or(0);
                replicas[index % replicas.len()].clone()
            }
            LoadBalanceKind::Random => {
                let index = rand::random_range(0..replicas.len());
                replicas[index].clone()
            }
            LoadBalanceKind::Weight => {
                let mut weighted = Vec::new();
                for replica in &replicas {
                    let weight = self
                        .config
                        .datasources
                        .get(replica.as_str())
                        .map(|config| config.weight.max(1))
                        .unwrap_or(1);
                    for _ in 0..weight {
                        weighted.push(replica.clone());
                    }
                }
                if weighted.is_empty() {
                    effective_primary.clone()
                } else {
                    let index = rand::random_range(0..weighted.len());
                    weighted[index].clone()
                }
            }
        };

        self.record_runtime_metrics(
            Some(rule.name.clone()),
            operation,
            selected.clone(),
            replicas.clone(),
            runtime_state
                .as_ref()
                .is_some_and(|state| state.failover_active),
        );
        selected
    }

    fn record_runtime_metrics(
        &self,
        rule_name: Option<String>,
        operation: SqlOperation,
        selected: String,
        candidates: Vec<String>,
        failover_active: bool,
    ) {
        record_shard_hit(ShardHitMetric {
            rule_name: rule_name.clone(),
            operation,
            selected,
            candidates: candidates.clone(),
            failover_active,
        });
        record_fanout(FanoutMetric {
            rule_name,
            operation,
            fanout: candidates.len(),
            targets: candidates,
        });
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        sync::{Arc, Mutex, OnceLock},
    };

    use crate::{
        config::{
            DataSourceConfig, DataSourceRole, LoadBalanceKind, ReadWriteRuleConfig,
            ReadWriteSplittingConfig, ShardingConfig,
        },
        datasource::{
            DataSourceRouteState, InMemoryRuntimeRecorder, clear_route_states,
            reset_runtime_recorder, set_route_state, set_runtime_recorder,
        },
        router::{ReadWriteRouter, SqlOperation},
    };

    fn build_config() -> Arc<ShardingConfig> {
        let mut datasources = BTreeMap::new();
        datasources.insert(
            "ds_ai_primary".to_string(),
            DataSourceConfig {
                uri: "mock://primary".to_string(),
                schema: None,
                role: DataSourceRole::Primary,
                weight: 1,
            },
        );
        datasources.insert(
            "ds_ai_replica_a".to_string(),
            DataSourceConfig {
                uri: "mock://replica-a".to_string(),
                schema: None,
                role: DataSourceRole::Replica,
                weight: 1,
            },
        );
        datasources.insert(
            "ds_ai_replica_b".to_string(),
            DataSourceConfig {
                uri: "mock://replica-b".to_string(),
                schema: None,
                role: DataSourceRole::Replica,
                weight: 1,
            },
        );

        Arc::new(ShardingConfig {
            datasources,
            read_write_splitting: ReadWriteSplittingConfig {
                enabled: true,
                rules: vec![ReadWriteRuleConfig {
                    name: "ai-rw".to_string(),
                    primary: "ds_ai_primary".to_string(),
                    replicas: vec!["ds_ai_replica_a".to_string(), "ds_ai_replica_b".to_string()],
                    load_balance: LoadBalanceKind::RoundRobin,
                }],
            },
            ..Default::default()
        })
    }

    fn runtime_state_test_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn route_uses_discovery_failover_for_reads_and_writes() {
        let _guard = runtime_state_test_lock().lock().unwrap();
        clear_route_states();
        let router = ReadWriteRouter::new(build_config());
        set_route_state(
            "ds_ai_primary",
            DataSourceRouteState {
                rule_name: "ai-rw".to_string(),
                configured_primary: "ds_ai_primary".to_string(),
                write_target: Some("ds_ai_replica_b".to_string()),
                healthy_replicas: vec!["ds_ai_replica_b".to_string()],
                unhealthy: vec!["ds_ai_primary".to_string(), "ds_ai_replica_a".to_string()],
                failover_active: true,
            },
        );

        let read_target = router.route("ds_ai_primary", SqlOperation::Select, false);
        let write_target = router.route("ds_ai_primary", SqlOperation::Update, false);

        assert_eq!(read_target, "ds_ai_replica_b");
        assert_eq!(write_target, "ds_ai_replica_b");
        clear_route_states();
    }

    #[test]
    fn route_records_shard_hit_and_fanout_metrics() {
        let _guard = runtime_state_test_lock().lock().unwrap();
        clear_route_states();
        let recorder = Arc::new(InMemoryRuntimeRecorder::default());
        set_runtime_recorder(recorder.clone());

        let router = ReadWriteRouter::new(build_config());
        let _ = router.route("ds_ai_primary", SqlOperation::Select, false);

        let snapshot = recorder.snapshot();
        assert_eq!(snapshot.shard_hits.len(), 1);
        assert_eq!(snapshot.fanouts.len(), 1);
        assert_eq!(snapshot.shard_hits[0].operation, SqlOperation::Select);
        assert_eq!(snapshot.shard_hits[0].selected, "ds_ai_replica_a");
        assert_eq!(snapshot.fanouts[0].fanout, 2);

        reset_runtime_recorder();
        clear_route_states();
    }
}
