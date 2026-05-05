//! Retry 退避配置 + 计算函数。
//!
//! 跨候选 retry 时(`pipeline::execute_non_stream_with_retry`),候选间需要短暂
//! sleep 避免上游瞬时压力雪崩。同时通过 `max_attempts` 给候选数封顶,
//! 避免某 model 命中 50 个 channel 时把 retry 拉到 50 次。
//!
//! # 退避公式
//!
//! `min(base_ms * 2^attempt, max_ms) ± jitter%`
//!
//! 默认参数(LLM 中转用户对延迟敏感,选短间隔变体):
//!
//! | attempt | 延迟范围(±20%) |
//! |---------|----------------|
//! | 0       | 40–60 ms       |
//! | 1       | 80–120 ms      |
//! | 2       | 160–240 ms     |
//! | 3       | 320–480 ms     |
//! | 4       | 640–960 ms     |
//! | 5+      | ~1000 ms (上限)|
//!
//! # 配置
//!
//! 从 `[relay-resilience]` toml 段加载,缺省走 `Default`。

use std::time::Duration;

use rand::RngExt;
use serde::Deserialize;
use summer::config::Configurable;

/// 跨候选 retry 的退避 / 上限配置。
///
/// ```toml
/// [relay-resilience]
/// max_attempts = 5
/// backoff_base_ms = 50
/// backoff_max_ms = 1000
/// backoff_jitter_pct = 20
/// ```
#[derive(Debug, Clone, Deserialize, Configurable)]
#[config_prefix = "relay-resilience"]
pub struct RetryConfig {
    /// 最多尝试多少个候选(候选超出此数被截断)。默认 5。
    #[serde(default = "default_max_attempts")]
    pub max_attempts: u32,
    /// 退避基础时长(毫秒)。第一个候选不 sleep,第二起按 base*2^attempt 计算。默认 50ms。
    #[serde(default = "default_backoff_base_ms")]
    pub backoff_base_ms: u64,
    /// 退避上限(毫秒),指数增长被钳制在此值。默认 1000ms。
    #[serde(default = "default_backoff_max_ms")]
    pub backoff_max_ms: u64,
    /// 抖动百分比(0-100),实际延迟在 base±jitter% 范围。默认 20%。
    #[serde(default = "default_backoff_jitter_pct")]
    pub backoff_jitter_pct: u32,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: default_max_attempts(),
            backoff_base_ms: default_backoff_base_ms(),
            backoff_max_ms: default_backoff_max_ms(),
            backoff_jitter_pct: default_backoff_jitter_pct(),
        }
    }
}

fn default_max_attempts() -> u32 {
    5
}
fn default_backoff_base_ms() -> u64 {
    50
}
fn default_backoff_max_ms() -> u64 {
    1000
}
fn default_backoff_jitter_pct() -> u32 {
    20
}

/// 计算第 `attempt` 次重试的退避时长。
///
/// `attempt` 从 0 开始,即"切到第二个候选前"的等待是 `backoff_delay(0, ...)`。
///
/// 公式:
/// 1. base = `backoff_base_ms * 2^attempt`(用 `saturating_*` 防溢出)
/// 2. capped = `min(base, backoff_max_ms)`
/// 3. jitter = `±capped * jitter_pct / 100`(均匀分布在 `[-jitter_range, +jitter_range]`)
/// 4. delay = `(capped + jitter).max(0)`
pub fn backoff_delay(attempt: u32, cfg: &RetryConfig) -> Duration {
    let base = cfg
        .backoff_base_ms
        .saturating_mul(2u64.saturating_pow(attempt));
    let capped = base.min(cfg.backoff_max_ms);

    if cfg.backoff_jitter_pct == 0 || capped == 0 {
        return Duration::from_millis(capped);
    }

    let jitter_range = capped * cfg.backoff_jitter_pct as u64 / 100;
    if jitter_range == 0 {
        return Duration::from_millis(capped);
    }

    let mut rng = rand::rng();
    let span = jitter_range.saturating_mul(2);
    let offset = rng.random_range(0..=span) as i64 - jitter_range as i64;
    let delay_ms = (capped as i64 + offset).max(0) as u64;
    Duration::from_millis(delay_ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(base: u64, max: u64, jitter: u32) -> RetryConfig {
        RetryConfig {
            max_attempts: 5,
            backoff_base_ms: base,
            backoff_max_ms: max,
            backoff_jitter_pct: jitter,
        }
    }

    #[test]
    fn no_jitter_returns_exact_base_for_attempt_zero() {
        let c = cfg(50, 1000, 0);
        assert_eq!(backoff_delay(0, &c), Duration::from_millis(50));
    }

    #[test]
    fn no_jitter_doubles_per_attempt() {
        let c = cfg(50, 10_000, 0);
        assert_eq!(backoff_delay(0, &c), Duration::from_millis(50));
        assert_eq!(backoff_delay(1, &c), Duration::from_millis(100));
        assert_eq!(backoff_delay(2, &c), Duration::from_millis(200));
        assert_eq!(backoff_delay(3, &c), Duration::from_millis(400));
        assert_eq!(backoff_delay(4, &c), Duration::from_millis(800));
    }

    #[test]
    fn no_jitter_clamps_to_max() {
        let c = cfg(50, 200, 0);
        assert_eq!(backoff_delay(0, &c), Duration::from_millis(50));
        assert_eq!(backoff_delay(1, &c), Duration::from_millis(100));
        assert_eq!(backoff_delay(2, &c), Duration::from_millis(200));
        assert_eq!(backoff_delay(3, &c), Duration::from_millis(200));
        assert_eq!(backoff_delay(10, &c), Duration::from_millis(200));
    }

    #[test]
    fn very_large_attempt_does_not_overflow() {
        let c = cfg(50, 1000, 0);
        // 100 这种极端 attempt 不会 panic;最终被钳到 max
        assert_eq!(backoff_delay(100, &c), Duration::from_millis(1000));
    }

    #[test]
    fn jitter_keeps_delay_within_range() {
        let c = cfg(100, 1000, 20);
        // 100 ± 20% = [80, 120]
        for _ in 0..200 {
            let d = backoff_delay(0, &c).as_millis() as u64;
            assert!(
                (80..=120).contains(&d),
                "delay {d} out of [80, 120] for attempt=0"
            );
        }
    }

    #[test]
    fn jitter_distribution_actually_varies() {
        let c = cfg(100, 1000, 20);
        let mut seen: std::collections::HashSet<u64> = std::collections::HashSet::new();
        for _ in 0..200 {
            seen.insert(backoff_delay(0, &c).as_millis() as u64);
        }
        // 200 次抽样里至少应该看到 5 个不同延迟值(否则 RNG 没工作)
        assert!(seen.len() > 5, "expected variation, got {seen:?}");
    }

    #[test]
    fn zero_base_is_zero() {
        let c = cfg(0, 1000, 20);
        assert_eq!(backoff_delay(0, &c), Duration::from_millis(0));
        assert_eq!(backoff_delay(5, &c), Duration::from_millis(0));
    }

    #[test]
    fn default_values_are_sane() {
        let c = RetryConfig::default();
        assert_eq!(c.max_attempts, 5);
        assert_eq!(c.backoff_base_ms, 50);
        assert_eq!(c.backoff_max_ms, 1000);
        assert_eq!(c.backoff_jitter_pct, 20);
    }
}
