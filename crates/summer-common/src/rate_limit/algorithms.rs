//! 各算法的具体实现（memory + redis）+ Lua 脚本绑定 + key 处理 helper。

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;

use crate::rate_limit::config::{RateLimitAlgorithm, RateLimitConfig, cache_key_for};
use crate::rate_limit::decision::{RateLimitDecision, RateLimitMetadata};
use crate::rate_limit::engine::{FixedWindowState, RateLimitEngine};

pub(crate) const REDIS_GCRA_SCRIPT: &str = include_str!("lua/rate_limit_gcra.lua");
pub(crate) const REDIS_GCRA_REFUND_SCRIPT: &str = include_str!("lua/rate_limit_gcra_refund.lua");
pub(crate) const REDIS_FIXED_WINDOW_SCRIPT: &str = include_str!("lua/rate_limit_fixed_window.lua");
pub(crate) const REDIS_SLIDING_WINDOW_SCRIPT: &str =
    include_str!("lua/rate_limit_sliding_window.lua");
pub(crate) const REDIS_SCHEDULED_SLOT_SCRIPT: &str =
    include_str!("lua/rate_limit_scheduled_slot.lua");

/// user-supplied key 段最多保留多少字节原文（超过则改用 md5 hash 形式）。
///
/// 作用是同时防御两类问题：
/// - 攻击者塞超长 header / user_id 把 Redis key / cache key 撑大
/// - 海量唯一长 key 把 moka cache 灌满（cache thrashing）
pub(crate) const USER_KEY_MAX_LEN: usize = 200;

/// 把 user-supplied 的 key 段处理成稳定 + 有界长度的形式。
///
/// 短 key（含中文 / 特殊字符）原样返回，方便运维 `redis-cli` 直接看；超长 key
/// 改用 `h:` 前缀 + md5 hex（32 字符）表示。md5 选用是因为本仓库已有依赖，
/// 在 cache-key 场景里它不是密码学用途，只要稳定且分布均匀即可。
pub(crate) fn sanitize_user_key(key: &str) -> String {
    if key.len() <= USER_KEY_MAX_LEN {
        return key.to_string();
    }
    use md5::{Digest, Md5};
    let mut hasher = Md5::new();
    hasher.update(key.as_bytes());
    let digest = hasher.finalize();
    let mut out = String::with_capacity(34);
    out.push_str("h:");
    for byte in digest.iter() {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

pub(crate) fn current_time_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

// =============================================================================
// 算法实现：在 RateLimitEngine 上做 inherent impl 拓展
// =============================================================================

impl RateLimitEngine {
    pub(crate) fn check_memory(
        &self,
        key: &str,
        config: &RateLimitConfig,
        cost: u32,
    ) -> RateLimitDecision {
        if config.algorithm.uses_gcra() {
            self.check_memory_gcra(key, config, cost)
        } else if config.algorithm.uses_scheduled_slot() {
            let max_wait_ms = if config.algorithm == RateLimitAlgorithm::LeakyBucket {
                0
            } else {
                config.max_wait_ms
            };
            self.check_memory_scheduled_slot(key, config, max_wait_ms)
        } else {
            match config.algorithm {
                RateLimitAlgorithm::FixedWindow => self.check_memory_fixed_window(key, config),
                RateLimitAlgorithm::SlidingWindow => self.check_memory_sliding_window(key, config),
                _ => unreachable!("algorithm dispatch must be exhaustive"),
            }
        }
    }

    /// GCRA / TokenBucket 共享路径，CAS 无锁，支持 cost > 1。
    pub(crate) fn check_memory_gcra(
        &self,
        key: &str,
        config: &RateLimitConfig,
        cost: u32,
    ) -> RateLimitDecision {
        let now_ms = current_time_millis();
        let emission = config.emission_interval_millis();
        let burst = config.effective_burst() as i64;
        let cost = cost.max(1) as i64;
        // burst * emission / cost * emission：极端值（cost=u32::MAX, emission=86_400_000）
        // 乘积约 3.7e17，离 i64::MAX (9.22e18) 仍有 25 倍余量；改 saturating 是为了
        // 即使未来数据类型 / 范围调整也不会 panic。
        let capacity = burst.saturating_mul(emission);
        let cost_emission = cost.saturating_mul(emission);
        let limit = burst as u32;

        let cache_key = cache_key_for(config, key);
        let state = self
            .gcra_states
            .get_with(cache_key, || Arc::new(AtomicI64::new(now_ms)));

        loop {
            let tat = state.load(Ordering::Acquire);
            let arrival = tat.max(now_ms);
            let diff = arrival.saturating_sub(now_ms);

            // cost-based GCRA: 推进后桶超容 → 拒绝
            if diff.saturating_add(cost_emission) > capacity {
                let retry_after_ms = diff
                    .saturating_add(cost_emission)
                    .saturating_sub(capacity)
                    .max(0) as u64;
                let retry_after = Duration::from_millis(retry_after_ms);
                return RateLimitDecision::Rejected(RateLimitMetadata::rejected(
                    limit,
                    retry_after,
                ));
            }

            let new_tat = arrival.saturating_add(cost_emission);
            if state
                .compare_exchange(tat, new_tat, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                let remaining = if emission > 0 {
                    (capacity.saturating_sub(diff).saturating_sub(cost_emission) / emission).max(0)
                        as u32
                } else {
                    0
                };
                let reset_after =
                    Duration::from_millis(new_tat.saturating_sub(now_ms).max(0) as u64);
                return RateLimitDecision::Allowed(RateLimitMetadata {
                    limit,
                    remaining,
                    reset_after,
                    retry_after: None,
                });
            }
        }
    }

    pub(crate) fn check_memory_fixed_window(
        &self,
        key: &str,
        config: &RateLimitConfig,
    ) -> RateLimitDecision {
        let now_ms = current_time_millis();
        let window_ms = config.window_millis().max(1);
        let window_id = now_ms.div_euclid(window_ms);
        let limit = config.window_limit();

        let cache_key = cache_key_for(config, key);
        let state = self.fixed_window_states.get_with(cache_key, || {
            Arc::new(Mutex::new(FixedWindowState {
                window_id,
                count: 0,
            }))
        });

        let mut s = state.lock();
        if s.window_id != window_id {
            s.window_id = window_id;
            s.count = 0;
        }

        let window_end_ms = (window_id + 1) * window_ms;
        let reset_after = Duration::from_millis(window_end_ms.saturating_sub(now_ms).max(0) as u64);

        if s.count >= limit {
            return RateLimitDecision::Rejected(RateLimitMetadata {
                limit,
                remaining: 0,
                reset_after,
                retry_after: Some(reset_after),
            });
        }
        s.count += 1;
        RateLimitDecision::Allowed(RateLimitMetadata {
            limit,
            remaining: limit.saturating_sub(s.count),
            reset_after,
            retry_after: None,
        })
    }

    pub(crate) fn check_memory_sliding_window(
        &self,
        key: &str,
        config: &RateLimitConfig,
    ) -> RateLimitDecision {
        let now_ms = current_time_millis();
        let window_ms = config.window_millis().max(1);
        let limit = config.window_limit();

        let cache_key = cache_key_for(config, key);
        let state = self
            .sliding_window_states
            .get_with(cache_key, || Arc::new(Mutex::new(VecDeque::new())));

        let mut entries = state.lock();
        let cutoff = now_ms - window_ms;
        while entries.front().is_some_and(|ts| *ts <= cutoff) {
            entries.pop_front();
        }

        // reset_after = 最老一条出窗的时间（front + window - now）。
        // 之前版本恒返回 window_ms，违反 IETF `RateLimit-Reset` 语义。
        let reset_after_from_front = |entries: &VecDeque<i64>| -> Duration {
            entries
                .front()
                .map(|oldest| Duration::from_millis((*oldest + window_ms - now_ms).max(0) as u64))
                .unwrap_or(Duration::ZERO)
        };

        if entries.len() as u32 >= limit {
            let retry_after = reset_after_from_front(&entries);
            return RateLimitDecision::Rejected(RateLimitMetadata {
                limit,
                remaining: 0,
                reset_after: retry_after,
                retry_after: Some(retry_after),
            });
        }

        entries.push_back(now_ms);
        let reset_after = reset_after_from_front(&entries);
        RateLimitDecision::Allowed(RateLimitMetadata {
            limit,
            remaining: limit.saturating_sub(entries.len() as u32),
            reset_after,
            retry_after: None,
        })
    }

    pub(crate) fn check_memory_scheduled_slot(
        &self,
        key: &str,
        config: &RateLimitConfig,
        max_wait_ms: u64,
    ) -> RateLimitDecision {
        let now_ms = current_time_millis();
        let interval_ms = config.emission_interval_millis().max(1);
        let limit = 1u32;

        let cache_key = cache_key_for(config, key);
        let state = self
            .scheduled_states
            .get_with(cache_key, || Arc::new(AtomicI64::new(now_ms)));

        loop {
            let next_available = state.load(Ordering::Acquire);
            let scheduled = next_available.max(now_ms);
            let delay_ms = scheduled.saturating_sub(now_ms).max(0) as u64;

            if delay_ms > max_wait_ms {
                let retry_after = Duration::from_millis(delay_ms);
                return RateLimitDecision::Rejected(RateLimitMetadata::rejected(
                    limit,
                    retry_after,
                ));
            }

            let new_next = scheduled.saturating_add(interval_ms);
            if state
                .compare_exchange(
                    next_available,
                    new_next,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_ok()
            {
                let meta = RateLimitMetadata {
                    limit,
                    remaining: 0,
                    reset_after: Duration::from_millis(interval_ms as u64),
                    retry_after: None,
                };
                if delay_ms == 0 {
                    return RateLimitDecision::Allowed(meta);
                }
                return RateLimitDecision::Delayed {
                    delay: Duration::from_millis(delay_ms),
                    meta,
                };
            }
        }
    }

    pub(crate) async fn check_redis(
        &self,
        key: &str,
        config: &RateLimitConfig,
        cost: u32,
    ) -> RateLimitDecision {
        let Some(redis) = &self.redis else {
            return self.handle_backend_failure(key, config, cost);
        };

        let now_ms = current_time_millis();
        let expire_seconds = config.redis_expire_seconds() as i64;
        let redis_key = self.redis_key_for(key, config);
        let mut conn = redis.clone();

        let result = match config.algorithm {
            RateLimitAlgorithm::TokenBucket | RateLimitAlgorithm::Gcra => {
                let burst = config.effective_burst().max(1) as i64;
                let emission = config.emission_interval_millis();
                summer_redis::redis::Script::new(REDIS_GCRA_SCRIPT)
                    .key(redis_key)
                    .arg(now_ms)
                    .arg(emission)
                    .arg(burst)
                    .arg(expire_seconds)
                    .arg(cost as i64)
                    .invoke_async::<Vec<i64>>(&mut conn)
                    .await
                    .map(|values| {
                        let (allowed, value_ms, remaining) = unpack_triple(&values);
                        decision_from_lua_triple(
                            allowed,
                            value_ms,
                            remaining,
                            burst as u32,
                            Duration::from_millis(emission as u64),
                        )
                    })
            }
            RateLimitAlgorithm::FixedWindow => {
                let limit = config.window_limit();
                summer_redis::redis::Script::new(REDIS_FIXED_WINDOW_SCRIPT)
                    .key(redis_key)
                    .arg(now_ms)
                    .arg(config.window_millis())
                    .arg(limit as i64)
                    .invoke_async::<Vec<i64>>(&mut conn)
                    .await
                    .map(|values| {
                        let (allowed, value_ms, remaining) = unpack_triple(&values);
                        decision_from_lua_triple(
                            allowed,
                            value_ms,
                            remaining,
                            limit,
                            Duration::from_millis(config.window_millis() as u64),
                        )
                    })
            }
            RateLimitAlgorithm::SlidingWindow => {
                let limit = config.window_limit();
                summer_redis::redis::Script::new(REDIS_SLIDING_WINDOW_SCRIPT)
                    .key(redis_key)
                    .arg(now_ms)
                    .arg(config.window_millis())
                    .arg(limit as i64)
                    .arg(format!("{now_ms}:{}", uuid::Uuid::new_v4()))
                    .arg(expire_seconds)
                    .invoke_async::<Vec<i64>>(&mut conn)
                    .await
                    .map(|values| {
                        let (allowed, value_ms, remaining) = unpack_triple(&values);
                        decision_from_lua_triple(
                            allowed,
                            value_ms,
                            remaining,
                            limit,
                            Duration::from_millis(config.window_millis() as u64),
                        )
                    })
            }
            RateLimitAlgorithm::LeakyBucket => {
                let interval = config.emission_interval_millis();
                summer_redis::redis::Script::new(REDIS_SCHEDULED_SLOT_SCRIPT)
                    .key(redis_key)
                    .arg(now_ms)
                    .arg(interval)
                    .arg(0_i64)
                    .arg(expire_seconds)
                    .invoke_async::<Vec<i64>>(&mut conn)
                    .await
                    .map(|values| {
                        let (allowed, value_ms, _) = unpack_triple(&values);
                        scheduled_decision_from_lua(
                            allowed,
                            value_ms,
                            Duration::from_millis(interval as u64),
                        )
                    })
            }
            RateLimitAlgorithm::ThrottleQueue => {
                let interval = config.emission_interval_millis();
                summer_redis::redis::Script::new(REDIS_SCHEDULED_SLOT_SCRIPT)
                    .key(redis_key)
                    .arg(now_ms)
                    .arg(interval)
                    .arg(config.max_wait_ms as i64)
                    .arg(expire_seconds)
                    .invoke_async::<Vec<i64>>(&mut conn)
                    .await
                    .map(|values| {
                        let (allowed, value_ms, _) = unpack_triple(&values);
                        scheduled_decision_from_lua(
                            allowed,
                            value_ms,
                            Duration::from_millis(interval as u64),
                        )
                    })
            }
        };

        match result {
            Ok(decision) => decision,
            Err(error) => {
                self.stats()
                    .backend_failures
                    .fetch_add(1, Ordering::Relaxed);
                tracing::warn!(
                    error = %error,
                    key,
                    rate = config.rate,
                    burst = config.burst,
                    algorithm = %config.algorithm.as_key_segment(),
                    failure_policy = %config.failure_policy.as_key_segment(),
                    window_seconds = config.window_seconds(),
                    max_wait_ms = config.max_wait_ms,
                    "redis rate limit check failed; applying failure policy"
                );
                self.handle_backend_failure(key, config, cost)
            }
        }
    }
}

// =============================================================================
// Lua 解析辅助
// =============================================================================

fn unpack_triple(values: &[i64]) -> (i64, i64, i64) {
    (
        values.first().copied().unwrap_or(0),
        values.get(1).copied().unwrap_or(0),
        values.get(2).copied().unwrap_or(0),
    )
}

fn decision_from_lua_triple(
    allowed: i64,
    value_ms: i64,
    remaining: i64,
    limit: u32,
    reset_after: Duration,
) -> RateLimitDecision {
    let remaining = remaining.max(0) as u32;
    let value = value_ms.max(0) as u64;
    if allowed == 1 {
        RateLimitDecision::Allowed(RateLimitMetadata {
            limit,
            remaining,
            reset_after,
            retry_after: None,
        })
    } else {
        let retry_after = Duration::from_millis(value);
        RateLimitDecision::Rejected(RateLimitMetadata {
            limit,
            remaining: 0,
            reset_after,
            retry_after: Some(retry_after),
        })
    }
}

fn scheduled_decision_from_lua(
    allowed: i64,
    value_ms: i64,
    reset_after: Duration,
) -> RateLimitDecision {
    let value = value_ms.max(0) as u64;
    let limit = 1u32;
    if allowed == 1 {
        let meta = RateLimitMetadata {
            limit,
            remaining: 0,
            reset_after,
            retry_after: None,
        };
        if value == 0 {
            RateLimitDecision::Allowed(meta)
        } else {
            RateLimitDecision::Delayed {
                delay: Duration::from_millis(value),
                meta,
            }
        }
    } else {
        let retry_after = Duration::from_millis(value);
        RateLimitDecision::Rejected(RateLimitMetadata::rejected(limit, retry_after))
    }
}
