//! 限流决策、metadata、统计计数器。

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use parking_lot::Mutex;

/// 限流决策附带的元数据，可用于 HTTP `RateLimit-*` / `Retry-After` 头。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateLimitMetadata {
    pub limit: u32,
    pub remaining: u32,
    pub reset_after: Duration,
    pub retry_after: Option<Duration>,
}

impl RateLimitMetadata {
    pub fn rejected(limit: u32, retry_after: Duration) -> Self {
        Self {
            limit,
            remaining: 0,
            reset_after: retry_after,
            retry_after: Some(retry_after),
        }
    }

    /// allowlist / 不限速场景的 metadata。
    pub fn unlimited() -> Self {
        Self {
            limit: u32::MAX,
            remaining: u32::MAX,
            reset_after: Duration::ZERO,
            retry_after: None,
        }
    }

    /// blocklist 命中的 metadata：不带 `retry_after`（永久拉黑，重试无意义；
    /// `Retry-After: 0` 反而会让攻击者立刻重试放大压力）。
    pub fn blocklisted() -> Self {
        Self {
            limit: 0,
            remaining: 0,
            reset_after: Duration::ZERO,
            retry_after: None,
        }
    }

    /// limit 是否是"无限"的占位值（[`Self::unlimited`] 或仅做 fallback 用途）。
    pub fn is_unlimited(&self) -> bool {
        self.limit == u32::MAX
    }
}

/// 跨多次 [`super::RateLimitContext::check`] 的共享 metadata 持有器。
///
/// 由 axum extractor 在 `Parts::extensions` 中注入；响应阶段的 layer 取出
/// 写入 HTTP header。多键复合限流时取**最严格**（remaining 最少）的那一个。
#[derive(Debug, Default)]
pub struct RateLimitMetadataHolder {
    inner: Mutex<Option<RateLimitMetadata>>,
}

impl RateLimitMetadataHolder {
    pub fn record(&self, meta: RateLimitMetadata) {
        let mut slot = self.inner.lock();
        match slot.as_ref() {
            Some(existing) if existing.remaining <= meta.remaining => {}
            _ => *slot = Some(meta),
        }
    }

    pub fn snapshot(&self) -> Option<RateLimitMetadata> {
        *self.inner.lock()
    }
}

#[derive(Debug, Clone)]
pub enum RateLimitDecision {
    Allowed(RateLimitMetadata),
    Delayed {
        delay: Duration,
        meta: RateLimitMetadata,
    },
    Rejected(RateLimitMetadata),
    BackendUnavailable,
}

impl RateLimitDecision {
    pub fn metadata(&self) -> Option<&RateLimitMetadata> {
        match self {
            Self::Allowed(meta) | Self::Rejected(meta) => Some(meta),
            Self::Delayed { meta, .. } => Some(meta),
            Self::BackendUnavailable => None,
        }
    }
}

/// 引擎运行时统计（atomic counter，线程安全），可对接 Prometheus / Datadog。
#[derive(Debug, Default)]
pub struct RateLimitStats {
    pub allowed: AtomicU64,
    pub delayed: AtomicU64,
    pub rejected: AtomicU64,
    pub backend_failures: AtomicU64,
    pub fallback_to_memory: AtomicU64,
    pub fail_open_passes: AtomicU64,
    pub fail_closed_blocks: AtomicU64,
    pub allowlist_passes: AtomicU64,
    pub blocklist_blocks: AtomicU64,
    pub shadow_passes: AtomicU64,
    pub cost_consumed: AtomicU64,
    pub cost_refunded: AtomicU64,
}

#[derive(Debug, Clone, Copy, Default, serde::Serialize)]
pub struct RateLimitStatsSnapshot {
    pub allowed: u64,
    pub delayed: u64,
    pub rejected: u64,
    pub backend_failures: u64,
    pub fallback_to_memory: u64,
    pub fail_open_passes: u64,
    pub fail_closed_blocks: u64,
    pub allowlist_passes: u64,
    pub blocklist_blocks: u64,
    pub shadow_passes: u64,
    pub cost_consumed: u64,
    pub cost_refunded: u64,
}

impl RateLimitStats {
    pub fn snapshot(&self) -> RateLimitStatsSnapshot {
        RateLimitStatsSnapshot {
            allowed: self.allowed.load(Ordering::Relaxed),
            delayed: self.delayed.load(Ordering::Relaxed),
            rejected: self.rejected.load(Ordering::Relaxed),
            backend_failures: self.backend_failures.load(Ordering::Relaxed),
            fallback_to_memory: self.fallback_to_memory.load(Ordering::Relaxed),
            fail_open_passes: self.fail_open_passes.load(Ordering::Relaxed),
            fail_closed_blocks: self.fail_closed_blocks.load(Ordering::Relaxed),
            allowlist_passes: self.allowlist_passes.load(Ordering::Relaxed),
            blocklist_blocks: self.blocklist_blocks.load(Ordering::Relaxed),
            shadow_passes: self.shadow_passes.load(Ordering::Relaxed),
            cost_consumed: self.cost_consumed.load(Ordering::Relaxed),
            cost_refunded: self.cost_refunded.load(Ordering::Relaxed),
        }
    }

    pub(crate) fn record(&self, decision: &RateLimitDecision) {
        match decision {
            RateLimitDecision::Allowed(_) => {
                self.allowed.fetch_add(1, Ordering::Relaxed);
            }
            RateLimitDecision::Delayed { .. } => {
                self.delayed.fetch_add(1, Ordering::Relaxed);
            }
            RateLimitDecision::Rejected(_) => {
                self.rejected.fetch_add(1, Ordering::Relaxed);
            }
            RateLimitDecision::BackendUnavailable => {
                self.fail_closed_blocks.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

impl RateLimitStatsSnapshot {
    /// 以 (metric_name, value) 数组形式暴露所有 counter，方便接入任意 metrics 库
    /// （prometheus / opentelemetry / metrics crate 等）。指标名遵循
    /// Prometheus 命名约定（snake_case，全小写，单数）。
    ///
    /// ```ignore
    /// for (name, value) in engine.stats().snapshot().as_metrics() {
    ///     // metrics::counter!(name).absolute(value);  // metrics crate
    ///     // prometheus::IntCounter::with_name(name).inc_by(value);
    /// }
    /// ```
    pub fn as_metrics(&self) -> [(&'static str, u64); 12] {
        [
            ("rate_limit_allowed_total", self.allowed),
            ("rate_limit_delayed_total", self.delayed),
            ("rate_limit_rejected_total", self.rejected),
            ("rate_limit_backend_failures_total", self.backend_failures),
            (
                "rate_limit_fallback_to_memory_total",
                self.fallback_to_memory,
            ),
            ("rate_limit_fail_open_passes_total", self.fail_open_passes),
            (
                "rate_limit_fail_closed_blocks_total",
                self.fail_closed_blocks,
            ),
            ("rate_limit_allowlist_passes_total", self.allowlist_passes),
            ("rate_limit_blocklist_blocks_total", self.blocklist_blocks),
            ("rate_limit_shadow_passes_total", self.shadow_passes),
            ("rate_limit_cost_consumed_total", self.cost_consumed),
            ("rate_limit_cost_refunded_total", self.cost_refunded),
        ]
    }
}

/// 让 holder 可以构造为 Arc 共享。
pub type SharedMetadataHolder = Arc<RateLimitMetadataHolder>;
