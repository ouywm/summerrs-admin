//! In-memory metrics counters for AI relay operations.
//!
//! All counters use atomic operations — no locking, no Redis overhead.
//! Exposed via the `/ai/runtime/metrics` dashboard endpoint.

use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

use serde::Serialize;

/// Global relay metrics singleton.
static METRICS: RelayMetrics = RelayMetrics::new();

/// Get the global metrics instance.
pub fn relay_metrics() -> &'static RelayMetrics {
    &METRICS
}

pub struct RelayMetrics {
    // ── Request counters ───────────────────────────────────────────
    /// Total relay requests received.
    pub requests_total: AtomicU64,
    /// Successful relay responses (2xx from upstream).
    pub requests_success: AtomicU64,
    /// Failed relay responses (upstream error or no channel).
    pub requests_failed: AtomicU64,
    /// Requests currently in-flight.
    pub requests_in_flight: AtomicI64,

    // ── Token counters ─────────────────────────────────────────────
    /// Total input (prompt) tokens consumed.
    pub tokens_input: AtomicU64,
    /// Total output (completion) tokens consumed.
    pub tokens_output: AtomicU64,
    /// Total cached tokens.
    pub tokens_cached: AtomicU64,

    // ── Latency tracking (cumulative for computing averages) ──────
    /// Sum of all response latencies in milliseconds.
    pub latency_sum_ms: AtomicU64,
    /// Sum of all first-token latencies in milliseconds.
    pub first_token_latency_sum_ms: AtomicU64,
    /// Count of latency samples (for computing average).
    pub latency_count: AtomicU64,

    // ── Billing counters ───────────────────────────────────────────
    /// Total quota consumed (in internal units).
    pub quota_consumed: AtomicI64,
    /// Total quota refunded.
    pub quota_refunded: AtomicU64,

    // ── Rate limiting ──────────────────────────────────────────────
    /// Requests rejected by rate limiter.
    pub rate_limited_total: AtomicU64,

    // ── Channel health ─────────────────────────────────────────────
    /// Upstream 429 responses received.
    pub upstream_rate_limited: AtomicU64,
    /// Upstream 5xx responses received.
    pub upstream_server_errors: AtomicU64,
    /// Fallback attempts (retried on different channel).
    pub fallback_total: AtomicU64,
    /// Circuit breaker rejections.
    pub circuit_breaker_rejections: AtomicU64,

    // ── Stream ─────────────────────────────────────────────────────
    /// Active SSE streams.
    pub streams_active: AtomicI64,
    /// Total SSE streams completed.
    pub streams_completed: AtomicU64,
}

impl RelayMetrics {
    const fn new() -> Self {
        Self {
            requests_total: AtomicU64::new(0),
            requests_success: AtomicU64::new(0),
            requests_failed: AtomicU64::new(0),
            requests_in_flight: AtomicI64::new(0),
            tokens_input: AtomicU64::new(0),
            tokens_output: AtomicU64::new(0),
            tokens_cached: AtomicU64::new(0),
            latency_sum_ms: AtomicU64::new(0),
            first_token_latency_sum_ms: AtomicU64::new(0),
            latency_count: AtomicU64::new(0),
            quota_consumed: AtomicI64::new(0),
            quota_refunded: AtomicU64::new(0),
            rate_limited_total: AtomicU64::new(0),
            upstream_rate_limited: AtomicU64::new(0),
            upstream_server_errors: AtomicU64::new(0),
            fallback_total: AtomicU64::new(0),
            circuit_breaker_rejections: AtomicU64::new(0),
            streams_active: AtomicI64::new(0),
            streams_completed: AtomicU64::new(0),
        }
    }

    // ── Recording helpers ──────────────────────────────────────────

    pub fn record_request_start(&self) {
        self.requests_total.fetch_add(1, Ordering::Relaxed);
        self.requests_in_flight.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_request_success(&self, latency_ms: u64) {
        self.requests_success.fetch_add(1, Ordering::Relaxed);
        self.requests_in_flight.fetch_sub(1, Ordering::Relaxed);
        self.latency_sum_ms.fetch_add(latency_ms, Ordering::Relaxed);
        self.latency_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_request_failure(&self) {
        self.requests_failed.fetch_add(1, Ordering::Relaxed);
        self.requests_in_flight.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn record_tokens(&self, input: i32, output: i32, cached: i32) {
        self.tokens_input
            .fetch_add(input.max(0) as u64, Ordering::Relaxed);
        self.tokens_output
            .fetch_add(output.max(0) as u64, Ordering::Relaxed);
        self.tokens_cached
            .fetch_add(cached.max(0) as u64, Ordering::Relaxed);
    }

    pub fn record_stream_start(&self) {
        self.streams_active.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_stream_end(&self) {
        self.streams_active.fetch_sub(1, Ordering::Relaxed);
        self.streams_completed.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_first_token_latency(&self, ms: u64) {
        self.first_token_latency_sum_ms
            .fetch_add(ms, Ordering::Relaxed);
    }

    // ── Snapshot ───────────────────────────────────────────────────

    pub fn snapshot(&self) -> RelayMetricsSnapshot {
        let latency_count = self.latency_count.load(Ordering::Relaxed);
        RelayMetricsSnapshot {
            requests_total: self.requests_total.load(Ordering::Relaxed),
            requests_success: self.requests_success.load(Ordering::Relaxed),
            requests_failed: self.requests_failed.load(Ordering::Relaxed),
            requests_in_flight: self.requests_in_flight.load(Ordering::Relaxed),
            tokens_input: self.tokens_input.load(Ordering::Relaxed),
            tokens_output: self.tokens_output.load(Ordering::Relaxed),
            tokens_cached: self.tokens_cached.load(Ordering::Relaxed),
            avg_latency_ms: if latency_count > 0 {
                self.latency_sum_ms.load(Ordering::Relaxed) / latency_count
            } else {
                0
            },
            avg_first_token_ms: if latency_count > 0 {
                self.first_token_latency_sum_ms.load(Ordering::Relaxed) / latency_count
            } else {
                0
            },
            quota_consumed: self.quota_consumed.load(Ordering::Relaxed),
            quota_refunded: self.quota_refunded.load(Ordering::Relaxed),
            rate_limited_total: self.rate_limited_total.load(Ordering::Relaxed),
            upstream_rate_limited: self.upstream_rate_limited.load(Ordering::Relaxed),
            upstream_server_errors: self.upstream_server_errors.load(Ordering::Relaxed),
            fallback_total: self.fallback_total.load(Ordering::Relaxed),
            circuit_breaker_rejections: self.circuit_breaker_rejections.load(Ordering::Relaxed),
            streams_active: self.streams_active.load(Ordering::Relaxed),
            streams_completed: self.streams_completed.load(Ordering::Relaxed),
        }
    }
}

/// Serializable metrics snapshot for the dashboard API.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct RelayMetricsSnapshot {
    pub requests_total: u64,
    pub requests_success: u64,
    pub requests_failed: u64,
    pub requests_in_flight: i64,
    pub tokens_input: u64,
    pub tokens_output: u64,
    pub tokens_cached: u64,
    pub avg_latency_ms: u64,
    pub avg_first_token_ms: u64,
    pub quota_consumed: i64,
    pub quota_refunded: u64,
    pub rate_limited_total: u64,
    pub upstream_rate_limited: u64,
    pub upstream_server_errors: u64,
    pub fallback_total: u64,
    pub circuit_breaker_rejections: u64,
    pub streams_active: i64,
    pub streams_completed: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_request_lifecycle() {
        let m = RelayMetrics::new();
        m.record_request_start();
        assert_eq!(m.requests_in_flight.load(Ordering::Relaxed), 1);

        m.record_request_success(150);
        assert_eq!(m.requests_in_flight.load(Ordering::Relaxed), 0);
        assert_eq!(m.requests_total.load(Ordering::Relaxed), 1);
        assert_eq!(m.requests_success.load(Ordering::Relaxed), 1);

        let snap = m.snapshot();
        assert_eq!(snap.avg_latency_ms, 150);
    }

    #[test]
    fn record_tokens() {
        let m = RelayMetrics::new();
        m.record_tokens(100, 50, 20);
        m.record_tokens(200, 100, 0);

        let snap = m.snapshot();
        assert_eq!(snap.tokens_input, 300);
        assert_eq!(snap.tokens_output, 150);
        assert_eq!(snap.tokens_cached, 20);
    }

    #[test]
    fn stream_tracking() {
        let m = RelayMetrics::new();
        m.record_stream_start();
        m.record_stream_start();
        assert_eq!(m.streams_active.load(Ordering::Relaxed), 2);

        m.record_stream_end();
        assert_eq!(m.streams_active.load(Ordering::Relaxed), 1);
        assert_eq!(m.streams_completed.load(Ordering::Relaxed), 1);
    }
}
