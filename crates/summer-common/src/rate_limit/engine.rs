//! 限流引擎：管理内存桶 + Redis 后端，对外暴露 check / refund / reset_key 等高层 API。

use std::collections::VecDeque;
use std::net::IpAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use ipnetwork::IpNetwork;
use moka::sync::Cache;
use parking_lot::Mutex;

use crate::rate_limit::algorithms::{
    REDIS_GCRA_REFUND_SCRIPT, current_time_millis, sanitize_user_key,
};
use crate::rate_limit::config::{RateLimitAlgorithm, RateLimitBackend, RateLimitConfig};
use crate::rate_limit::decision::{
    RateLimitDecision, RateLimitMetadata, RateLimitStats, RateLimitStatsSnapshot,
};

/// 内存 cache 默认容量（防恶意 IP 注入爆内存）
pub const DEFAULT_MEMORY_CAPACITY: u64 = 100_000;
/// 内存 cache 默认空闲过期时间
pub const DEFAULT_MEMORY_IDLE_SECS: u64 = 3600;

/// FixedWindow 内存状态（窗口 id + 计数）。
pub(crate) struct FixedWindowState {
    pub(crate) window_id: i64,
    pub(crate) count: u32,
}

/// [`RateLimitEngine`] 构造参数。
#[derive(Debug, Clone)]
pub struct RateLimitEngineConfig {
    /// 内存 cache 单表最大条目数（防恶意 IP 注入）。默认 100_000。
    pub memory_capacity: u64,
    /// 内存 cache 闲置回收时间。默认 1 小时。
    pub memory_idle: Duration,
    /// 白名单：CIDR 命中直接放行，不消耗任何配额。
    pub allowlist: Vec<IpNetwork>,
    /// 黑名单：CIDR 命中直接拒绝（429）。
    pub blocklist: Vec<IpNetwork>,
}

impl Default for RateLimitEngineConfig {
    fn default() -> Self {
        Self {
            memory_capacity: DEFAULT_MEMORY_CAPACITY,
            memory_idle: Duration::from_secs(DEFAULT_MEMORY_IDLE_SECS),
            allowlist: Vec::new(),
            blocklist: Vec::new(),
        }
    }
}

/// 限流引擎。`Clone` 廉价（内部全 `Arc`）。
#[derive(Clone)]
pub struct RateLimitEngine {
    /// GCRA / TokenBucket / Gcra 共享：value 是 TAT (theoretical arrival time, ms)。
    pub(crate) gcra_states: Cache<String, Arc<AtomicI64>>,
    /// FixedWindow 状态（窗口 id + 计数）。
    pub(crate) fixed_window_states: Cache<String, Arc<Mutex<FixedWindowState>>>,
    /// SlidingWindow 时间戳日志。
    pub(crate) sliding_window_states: Cache<String, Arc<Mutex<VecDeque<i64>>>>,
    /// LeakyBucket / ThrottleQueue 共享：value 是 next_available_ms。
    pub(crate) scheduled_states: Cache<String, Arc<AtomicI64>>,
    pub(crate) redis: Option<summer_redis::Redis>,
    pub(crate) stats_inner: Arc<RateLimitStats>,
    pub(crate) allowlist: Arc<Vec<IpNetwork>>,
    pub(crate) blocklist: Arc<Vec<IpNetwork>>,
}

impl RateLimitEngine {
    pub fn new(redis: Option<summer_redis::Redis>) -> Self {
        Self::with_config(redis, RateLimitEngineConfig::default())
    }

    pub fn with_config(redis: Option<summer_redis::Redis>, cfg: RateLimitEngineConfig) -> Self {
        let build_atomic = |capacity: u64, idle: Duration| -> Cache<String, Arc<AtomicI64>> {
            Cache::builder()
                .max_capacity(capacity)
                .time_to_idle(idle)
                .build()
        };
        let build_fixed =
            |capacity: u64, idle: Duration| -> Cache<String, Arc<Mutex<FixedWindowState>>> {
                Cache::builder()
                    .max_capacity(capacity)
                    .time_to_idle(idle)
                    .build()
            };
        let build_sliding =
            |capacity: u64, idle: Duration| -> Cache<String, Arc<Mutex<VecDeque<i64>>>> {
                Cache::builder()
                    .max_capacity(capacity)
                    .time_to_idle(idle)
                    .build()
            };

        Self {
            gcra_states: build_atomic(cfg.memory_capacity, cfg.memory_idle),
            fixed_window_states: build_fixed(cfg.memory_capacity, cfg.memory_idle),
            sliding_window_states: build_sliding(cfg.memory_capacity, cfg.memory_idle),
            scheduled_states: build_atomic(cfg.memory_capacity, cfg.memory_idle),
            redis,
            stats_inner: Arc::new(RateLimitStats::default()),
            allowlist: Arc::new(cfg.allowlist),
            blocklist: Arc::new(cfg.blocklist),
        }
    }

    /// 暴露给业务层做监控接入。
    pub fn stats(&self) -> &RateLimitStats {
        &self.stats_inner
    }

    /// stats 的快照，方便业务层调 `as_metrics()` 接 Prometheus 等。
    pub fn stats_snapshot(&self) -> RateLimitStatsSnapshot {
        self.stats_inner.snapshot()
    }

    /// 名单短路：allowlist → 直接 Allowed；blocklist → 直接 Rejected。
    pub fn check_lists(&self, client_ip: IpAddr) -> Option<RateLimitDecision> {
        if !self.allowlist.is_empty() && self.allowlist.iter().any(|net| net.contains(client_ip)) {
            self.stats_inner
                .allowlist_passes
                .fetch_add(1, Ordering::Relaxed);
            return Some(RateLimitDecision::Allowed(RateLimitMetadata::unlimited()));
        }
        if !self.blocklist.is_empty() && self.blocklist.iter().any(|net| net.contains(client_ip)) {
            self.stats_inner
                .blocklist_blocks
                .fetch_add(1, Ordering::Relaxed);
            return Some(RateLimitDecision::Rejected(RateLimitMetadata::blocklisted()));
        }
        None
    }

    pub async fn check(&self, key: &str, config: &RateLimitConfig) -> RateLimitDecision {
        self.check_with_cost(key, config, 1).await
    }

    pub async fn check_with_cost(
        &self,
        key: &str,
        config: &RateLimitConfig,
        cost: u32,
    ) -> RateLimitDecision {
        let cost = cost.max(1);
        let decision = match config.backend {
            RateLimitBackend::Memory => self.check_memory(key, config, cost),
            RateLimitBackend::Redis => self.check_redis(key, config, cost).await,
        };
        self.stats_inner.record(&decision);
        decision
    }

    /// 重置某个 key 的限流状态（运维干预）。
    pub fn reset_key(&self, key: &str, config: &RateLimitConfig) {
        let cache_key = crate::rate_limit::config::cache_key_for(config, key);
        match config.algorithm {
            RateLimitAlgorithm::TokenBucket | RateLimitAlgorithm::Gcra => {
                self.gcra_states.invalidate(&cache_key);
            }
            RateLimitAlgorithm::FixedWindow => {
                self.fixed_window_states.invalidate(&cache_key);
            }
            RateLimitAlgorithm::SlidingWindow => {
                self.sliding_window_states.invalidate(&cache_key);
            }
            RateLimitAlgorithm::LeakyBucket | RateLimitAlgorithm::ThrottleQueue => {
                self.scheduled_states.invalidate(&cache_key);
            }
        }
        // Redis 端：尝试删除（best effort）
        if matches!(config.backend, RateLimitBackend::Redis)
            && let Some(redis) = self.redis.clone()
        {
            let redis_key = self.redis_key_for(key, config);
            tokio::spawn(async move {
                let mut conn = redis;
                let _: Result<i64, _> = summer_redis::redis::cmd("DEL")
                    .arg(redis_key)
                    .query_async(&mut conn)
                    .await;
            });
        }
    }

    /// 退还 `cost` 个单位的配额（仅 GCRA 内核生效）。
    pub async fn refund(&self, key: &str, config: &RateLimitConfig, cost: u32) {
        if !config.algorithm.supports_cost() || cost == 0 {
            return;
        }
        let emission = config.emission_interval_millis();
        let refund_ms = (cost as i64).saturating_mul(emission);

        match config.backend {
            RateLimitBackend::Memory => {
                let cache_key = crate::rate_limit::config::cache_key_for(config, key);
                if let Some(state) = self.gcra_states.get(&cache_key) {
                    // fetch_sub atomic；TAT 可能临时低于 now，下次 check 用 max(tat, now) 兜底
                    state.fetch_sub(refund_ms, Ordering::AcqRel);
                }
            }
            RateLimitBackend::Redis => {
                let Some(redis) = &self.redis else {
                    return;
                };
                let redis_key = self.redis_key_for(key, config);
                let mut conn = redis.clone();
                let now_ms = current_time_millis();
                let result = summer_redis::redis::Script::new(REDIS_GCRA_REFUND_SCRIPT)
                    .key(redis_key)
                    .arg(now_ms)
                    .arg(emission)
                    .arg(cost as i64)
                    .invoke_async::<i64>(&mut conn)
                    .await;
                if let Err(error) = result {
                    tracing::warn!(error = %error, key, cost, "rate-limit refund failed");
                }
            }
        }
        self.stats_inner
            .cost_refunded
            .fetch_add(cost as u64, Ordering::Relaxed);
    }

    /// 生成 Redis key。
    ///
    /// 把 user key 段用 `{...}` 包裹（Redis 的 hash tag 语法），保证 Cluster
    /// 模式下相同 user key 的所有派生 key（如 fixed_window 内动态拼的
    /// `KEYS[1]:window_id`）落在同一 hash slot —— 否则 Lua 脚本会触发
    /// `CROSSSLOT` 错误。单实例 Redis 对 hash tag 透明，无副作用。
    pub(crate) fn redis_key_for(&self, key: &str, config: &RateLimitConfig) -> String {
        // user key 可能很长（header value、长 user_id 等），这里做 sanitize +
        // 截断 + 必要时 hash，避免 Redis key 过大撑爆内存。
        let safe_key = sanitize_user_key(key);
        format!(
            "rate-limit:{}:{}:{}:{}:{}:{{{}}}",
            config.algorithm.as_key_segment(),
            config.rate,
            config.window_seconds(),
            config.effective_burst(),
            config.max_wait_ms,
            safe_key,
        )
    }

    pub(crate) fn handle_backend_failure(
        &self,
        key: &str,
        config: &RateLimitConfig,
        cost: u32,
    ) -> RateLimitDecision {
        use crate::rate_limit::config::RateLimitFailurePolicy;
        match config.failure_policy {
            RateLimitFailurePolicy::FailOpen => {
                self.stats_inner
                    .fail_open_passes
                    .fetch_add(1, Ordering::Relaxed);
                let limit = config.effective_burst().max(1);
                RateLimitDecision::Allowed(RateLimitMetadata {
                    limit,
                    remaining: limit,
                    reset_after: Duration::ZERO,
                    retry_after: None,
                })
            }
            RateLimitFailurePolicy::FailClosed => RateLimitDecision::BackendUnavailable,
            RateLimitFailurePolicy::FallbackMemory => {
                self.stats_inner
                    .fallback_to_memory
                    .fetch_add(1, Ordering::Relaxed);
                self.check_memory(key, config, cost)
            }
        }
    }
}
