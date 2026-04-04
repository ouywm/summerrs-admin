use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Circuit breaker states following the standard pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// Normal operation — requests are allowed.
    Closed,
    /// Too many failures — requests are blocked until recovery timeout.
    Open,
    /// Recovery timeout expired — one probe request is allowed.
    HalfOpen,
}

#[derive(Debug)]
struct CircuitEntry {
    state: CircuitState,
    failure_count: u32,
    last_failure_at: Option<Instant>,
    last_transition_at: Instant,
}

impl CircuitEntry {
    fn new() -> Self {
        Self {
            state: CircuitState::Closed,
            failure_count: 0,
            last_failure_at: None,
            last_transition_at: Instant::now(),
        }
    }
}

#[derive(Clone)]
pub struct CircuitBreakerRegistry {
    inner: Arc<RwLock<HashMap<i64, CircuitEntry>>>,
    failure_threshold: u32,
    recovery_timeout: Duration,
}

impl CircuitBreakerRegistry {
    pub fn new(failure_threshold: u32, recovery_timeout: Duration) -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
            failure_threshold,
            recovery_timeout,
        }
    }

    /// Returns `true` if the request should be allowed through.
    pub async fn allow_request(&self, channel_id: i64) -> bool {
        let mut map = self.inner.write().await;
        let entry = map.entry(channel_id).or_insert_with(CircuitEntry::new);

        match entry.state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                let elapsed = entry
                    .last_failure_at
                    .map(|t| t.elapsed())
                    .unwrap_or(Duration::ZERO);
                if elapsed >= self.recovery_timeout {
                    entry.state = CircuitState::HalfOpen;
                    entry.last_transition_at = Instant::now();
                    tracing::info!(
                        channel_id,
                        "circuit breaker: Open -> HalfOpen, allowing probe"
                    );
                    true
                } else {
                    false
                }
            }
            CircuitState::HalfOpen => {
                // Only one probe at a time — block additional requests while probing.
                false
            }
        }
    }

    /// Record a successful request — reset the circuit to Closed.
    pub async fn record_success(&self, channel_id: i64) {
        let mut map = self.inner.write().await;
        if let Some(entry) = map.get_mut(&channel_id) {
            if entry.state != CircuitState::Closed {
                tracing::info!(
                    channel_id,
                    prev_state = ?entry.state,
                    "circuit breaker: -> Closed after success"
                );
            }
            entry.state = CircuitState::Closed;
            entry.failure_count = 0;
            entry.last_failure_at = None;
            entry.last_transition_at = Instant::now();
        }
    }

    /// Record a failed request — increment failure count, possibly open the circuit.
    pub async fn record_failure(&self, channel_id: i64) {
        let mut map = self.inner.write().await;
        let entry = map.entry(channel_id).or_insert_with(CircuitEntry::new);

        entry.failure_count += 1;
        entry.last_failure_at = Some(Instant::now());

        if entry.state == CircuitState::HalfOpen {
            // Probe failed — go back to Open.
            entry.state = CircuitState::Open;
            entry.last_transition_at = Instant::now();
            tracing::warn!(
                channel_id,
                "circuit breaker: HalfOpen -> Open (probe failed)"
            );
        } else if entry.failure_count >= self.failure_threshold {
            if entry.state != CircuitState::Open {
                tracing::warn!(
                    channel_id,
                    failures = entry.failure_count,
                    threshold = self.failure_threshold,
                    "circuit breaker: Closed -> Open"
                );
            }
            entry.state = CircuitState::Open;
            entry.last_transition_at = Instant::now();
        }
    }

    /// Get the current state of a channel's circuit breaker.
    pub async fn state(&self, channel_id: i64) -> CircuitState {
        let map = self.inner.read().await;
        map.get(&channel_id)
            .map(|e| e.state)
            .unwrap_or(CircuitState::Closed)
    }

    /// Remove stale entries that have been Closed for a long time.
    pub async fn gc(&self) {
        let mut map = self.inner.write().await;
        map.retain(|_, entry| {
            !(entry.state == CircuitState::Closed
                && entry.last_transition_at.elapsed() > Duration::from_secs(3600))
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn circuit_opens_after_threshold_failures() {
        let cb = CircuitBreakerRegistry::new(3, Duration::from_millis(100));

        assert!(cb.allow_request(1).await);
        cb.record_failure(1).await;
        cb.record_failure(1).await;
        assert!(cb.allow_request(1).await); // still Closed at 2 failures

        cb.record_failure(1).await; // 3rd failure → Open
        assert_eq!(cb.state(1).await, CircuitState::Open);
        assert!(!cb.allow_request(1).await); // blocked
    }

    #[tokio::test]
    async fn circuit_transitions_to_half_open_after_timeout() {
        let cb = CircuitBreakerRegistry::new(1, Duration::from_millis(50));

        cb.record_failure(1).await;
        assert_eq!(cb.state(1).await, CircuitState::Open);

        tokio::time::sleep(Duration::from_millis(60)).await;
        assert!(cb.allow_request(1).await); // -> HalfOpen, probe allowed
        assert_eq!(cb.state(1).await, CircuitState::HalfOpen);
    }

    #[tokio::test]
    async fn successful_probe_closes_circuit() {
        let cb = CircuitBreakerRegistry::new(1, Duration::from_millis(50));

        cb.record_failure(1).await;
        tokio::time::sleep(Duration::from_millis(60)).await;
        cb.allow_request(1).await; // -> HalfOpen

        cb.record_success(1).await;
        assert_eq!(cb.state(1).await, CircuitState::Closed);
        assert!(cb.allow_request(1).await);
    }

    #[tokio::test]
    async fn failed_probe_reopens_circuit() {
        let cb = CircuitBreakerRegistry::new(1, Duration::from_millis(50));

        cb.record_failure(1).await;
        tokio::time::sleep(Duration::from_millis(60)).await;
        cb.allow_request(1).await; // -> HalfOpen

        cb.record_failure(1).await; // probe failed
        assert_eq!(cb.state(1).await, CircuitState::Open);
    }
}
