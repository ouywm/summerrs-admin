//! 调度器运行时 metrics —— 进程内计数器，admin API 拉取展示。
//!
//! 当前实现是简单的原子计数：
//! - 按状态分桶累计 worker 处理过的 run（succeeded / failed / timeout / canceled / discarded）
//! - 按 trigger_type 分桶累计触发次数
//! - 当前正在跑的 run 数（acquire 时 +1，drop guard 时 -1）
//!
//! 未来可以接入 prometheus exporter（保留 metric 名跟 prometheus 风格一致）。

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::Serialize;

use crate::enums::{RunState, TriggerType};

#[derive(Default)]
pub struct SchedulerMetrics {
    pub triggers_cron: AtomicU64,
    pub triggers_manual: AtomicU64,
    pub triggers_retry: AtomicU64,
    pub triggers_misfire: AtomicU64,
    pub triggers_workflow: AtomicU64,
    pub triggers_api: AtomicU64,

    pub runs_succeeded: AtomicU64,
    pub runs_failed: AtomicU64,
    pub runs_timeout: AtomicU64,
    pub runs_canceled: AtomicU64,
    pub runs_discarded: AtomicU64,
    pub runs_enqueued_or_running: AtomicU64,

    pub runs_in_flight: AtomicU64,
}

impl SchedulerMetrics {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn record_trigger(&self, trigger_type: TriggerType) {
        let counter = match trigger_type {
            TriggerType::Cron => &self.triggers_cron,
            TriggerType::Manual => &self.triggers_manual,
            TriggerType::Retry => &self.triggers_retry,
            TriggerType::Misfire => &self.triggers_misfire,
            TriggerType::Workflow => &self.triggers_workflow,
            TriggerType::Api => &self.triggers_api,
        };
        counter.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_terminal(&self, state: RunState) {
        let counter = match state {
            RunState::Succeeded => &self.runs_succeeded,
            RunState::Failed => &self.runs_failed,
            RunState::Timeout => &self.runs_timeout,
            RunState::Canceled => &self.runs_canceled,
            RunState::Discarded => &self.runs_discarded,
            RunState::Enqueued | RunState::Running => &self.runs_enqueued_or_running,
        };
        counter.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_in_flight(&self) {
        self.runs_in_flight.fetch_add(1, Ordering::AcqRel);
    }

    pub fn dec_in_flight(&self) {
        self.runs_in_flight.fetch_sub(1, Ordering::AcqRel);
    }

    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            triggers_cron: self.triggers_cron.load(Ordering::Relaxed),
            triggers_manual: self.triggers_manual.load(Ordering::Relaxed),
            triggers_retry: self.triggers_retry.load(Ordering::Relaxed),
            triggers_misfire: self.triggers_misfire.load(Ordering::Relaxed),
            triggers_workflow: self.triggers_workflow.load(Ordering::Relaxed),
            triggers_api: self.triggers_api.load(Ordering::Relaxed),
            runs_succeeded: self.runs_succeeded.load(Ordering::Relaxed),
            runs_failed: self.runs_failed.load(Ordering::Relaxed),
            runs_timeout: self.runs_timeout.load(Ordering::Relaxed),
            runs_canceled: self.runs_canceled.load(Ordering::Relaxed),
            runs_discarded: self.runs_discarded.load(Ordering::Relaxed),
            runs_enqueued_or_running: self.runs_enqueued_or_running.load(Ordering::Relaxed),
            runs_in_flight: self.runs_in_flight.load(Ordering::Acquire),
        }
    }
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MetricsSnapshot {
    pub triggers_cron: u64,
    pub triggers_manual: u64,
    pub triggers_retry: u64,
    pub triggers_misfire: u64,
    pub triggers_workflow: u64,
    pub triggers_api: u64,
    pub runs_succeeded: u64,
    pub runs_failed: u64,
    pub runs_timeout: u64,
    pub runs_canceled: u64,
    pub runs_discarded: u64,
    pub runs_enqueued_or_running: u64,
    pub runs_in_flight: u64,
}
