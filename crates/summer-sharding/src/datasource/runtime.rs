use std::{
    collections::BTreeMap,
    sync::{Arc, OnceLock},
};

use parking_lot::{Mutex, RwLock};

use crate::router::SqlOperation;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataSourceRouteState {
    pub rule_name: String,
    pub configured_primary: String,
    pub write_target: Option<String>,
    pub healthy_replicas: Vec<String>,
    pub unhealthy: Vec<String>,
    pub failover_active: bool,
}

impl DataSourceRouteState {
    pub fn effective_write_target(&self) -> Option<String> {
        self.write_target
            .clone()
            .or_else(|| Some(self.configured_primary.clone()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShardHitMetric {
    pub rule_name: Option<String>,
    pub operation: SqlOperation,
    pub selected: String,
    pub candidates: Vec<String>,
    pub failover_active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FanoutMetric {
    pub rule_name: Option<String>,
    pub operation: SqlOperation,
    pub fanout: usize,
    pub targets: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlowQueryMetric {
    pub datasource: String,
    pub elapsed_ms: u128,
    pub threshold_ms: u128,
    pub reason: String,
}

pub trait RuntimeRecorder: Send + Sync + 'static {
    fn record_shard_hit(&self, metric: ShardHitMetric);
    fn record_fanout(&self, metric: FanoutMetric);
    fn record_slow_query(&self, metric: SlowQueryMetric);
}

#[derive(Debug, Default)]
pub struct NoopRuntimeRecorder;

impl RuntimeRecorder for NoopRuntimeRecorder {
    fn record_shard_hit(&self, _metric: ShardHitMetric) {}

    fn record_fanout(&self, _metric: FanoutMetric) {}

    fn record_slow_query(&self, _metric: SlowQueryMetric) {}
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RuntimeMetricsSnapshot {
    pub shard_hits: Vec<ShardHitMetric>,
    pub fanouts: Vec<FanoutMetric>,
    pub slow_queries: Vec<SlowQueryMetric>,
}

#[derive(Debug, Default)]
pub struct InMemoryRuntimeRecorder {
    shard_hits: Mutex<Vec<ShardHitMetric>>,
    fanouts: Mutex<Vec<FanoutMetric>>,
    slow_queries: Mutex<Vec<SlowQueryMetric>>,
}

impl InMemoryRuntimeRecorder {
    pub fn snapshot(&self) -> RuntimeMetricsSnapshot {
        RuntimeMetricsSnapshot {
            shard_hits: self.shard_hits.lock().clone(),
            fanouts: self.fanouts.lock().clone(),
            slow_queries: self.slow_queries.lock().clone(),
        }
    }
}

impl RuntimeRecorder for InMemoryRuntimeRecorder {
    fn record_shard_hit(&self, metric: ShardHitMetric) {
        self.shard_hits.lock().push(metric);
    }

    fn record_fanout(&self, metric: FanoutMetric) {
        self.fanouts.lock().push(metric);
    }

    fn record_slow_query(&self, metric: SlowQueryMetric) {
        self.slow_queries.lock().push(metric);
    }
}

fn recorder_cell() -> &'static RwLock<Arc<dyn RuntimeRecorder>> {
    static RECORDER: OnceLock<RwLock<Arc<dyn RuntimeRecorder>>> = OnceLock::new();
    RECORDER.get_or_init(|| RwLock::new(Arc::new(NoopRuntimeRecorder)))
}

fn route_state_cell() -> &'static RwLock<BTreeMap<String, DataSourceRouteState>> {
    static ROUTE_STATE: OnceLock<RwLock<BTreeMap<String, DataSourceRouteState>>> = OnceLock::new();
    ROUTE_STATE.get_or_init(|| RwLock::new(BTreeMap::new()))
}

pub fn runtime_recorder() -> Arc<dyn RuntimeRecorder> {
    recorder_cell().read().clone()
}

pub fn set_runtime_recorder(recorder: Arc<dyn RuntimeRecorder>) {
    *recorder_cell().write() = recorder;
}

pub fn reset_runtime_recorder() {
    *recorder_cell().write() = Arc::new(NoopRuntimeRecorder);
}

pub fn set_route_state(primary: &str, state: DataSourceRouteState) {
    route_state_cell()
        .write()
        .insert(primary.to_string(), state);
}

pub fn route_state(primary: &str) -> Option<DataSourceRouteState> {
    route_state_cell().read().get(primary).cloned()
}

pub fn clear_route_states() {
    route_state_cell().write().clear();
}

pub fn record_shard_hit(metric: ShardHitMetric) {
    runtime_recorder().record_shard_hit(metric);
}

pub fn record_fanout(metric: FanoutMetric) {
    runtime_recorder().record_fanout(metric);
}

pub fn record_slow_query(metric: SlowQueryMetric) {
    runtime_recorder().record_slow_query(metric);
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::{
        datasource::{
            FanoutMetric, InMemoryRuntimeRecorder, RuntimeRecorder, ShardHitMetric,
            SlowQueryMetric, reset_runtime_recorder, set_runtime_recorder,
        },
        router::SqlOperation,
    };

    #[test]
    fn in_memory_runtime_recorder_records_all_metric_types() {
        let recorder = Arc::new(InMemoryRuntimeRecorder::default());
        set_runtime_recorder(recorder.clone());

        recorder.record_shard_hit(ShardHitMetric {
            rule_name: Some("rw".to_string()),
            operation: SqlOperation::Select,
            selected: "ds_r0".to_string(),
            candidates: vec!["ds_r0".to_string(), "ds_r1".to_string()],
            failover_active: false,
        });
        recorder.record_fanout(FanoutMetric {
            rule_name: Some("rw".to_string()),
            operation: SqlOperation::Select,
            fanout: 2,
            targets: vec!["ds_r0".to_string(), "ds_r1".to_string()],
        });
        recorder.record_slow_query(SlowQueryMetric {
            datasource: "ds_p".to_string(),
            elapsed_ms: 1200,
            threshold_ms: 1000,
            reason: "health_check".to_string(),
        });

        let snapshot = recorder.snapshot();
        assert_eq!(snapshot.shard_hits.len(), 1);
        assert_eq!(snapshot.fanouts.len(), 1);
        assert_eq!(snapshot.slow_queries.len(), 1);

        reset_runtime_recorder();
    }
}
