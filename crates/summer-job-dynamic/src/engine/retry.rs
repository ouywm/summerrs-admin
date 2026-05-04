//! 重试退避策略 —— Sidekiq 风格指数退避。
//!
//! 失败的 trigger 在 worker 内部按 `retry_backoff` 计算下一次延迟，spawn delay
//! task 重新调用 `Worker::execute`（trigger_type = Retry，retry_count + 1）。
//!
//! 几种策略对比（base 1 次失败的延迟）：
//!
//! | 策略 | retry_count=0 | =1 | =2 | =3 | =4 |
//! |---|---|---|---|---|---|
//! | EXPONENTIAL | ~15s | ~31s | ~76s | ~206s | ~511s |
//! | LINEAR | 60s | 120s | 180s | 240s | 300s |
//! | FIXED | 60s | 60s | 60s | 60s | 60s |

use std::time::Duration;

use rand::RngExt;

use crate::enums::RetryBackoff;

/// 计算下次重试延迟（输入是已失败次数 `retry_count`，从 0 开始）。
///
/// EXPONENTIAL 公式参考 Sidekiq：`(retry_count^4) + 15 + jitter(0..30 * (retry_count+1))`。
pub fn next_retry_delay(retry_count: u32, strategy: RetryBackoff) -> Duration {
    match strategy {
        RetryBackoff::Exponential => {
            let base = (retry_count as u64).pow(4);
            let jitter = rand::rng().random_range(0..(30 * (retry_count as u64 + 1)));
            Duration::from_secs(base + 15 + jitter)
        }
        RetryBackoff::Linear => Duration::from_secs((retry_count as u64 + 1) * 60),
        RetryBackoff::Fixed => Duration::from_secs(60),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_returns_constant() {
        assert_eq!(
            next_retry_delay(0, RetryBackoff::Fixed),
            Duration::from_secs(60)
        );
        assert_eq!(
            next_retry_delay(5, RetryBackoff::Fixed),
            Duration::from_secs(60)
        );
    }

    #[test]
    fn linear_grows_linearly() {
        assert_eq!(
            next_retry_delay(0, RetryBackoff::Linear),
            Duration::from_secs(60)
        );
        assert_eq!(
            next_retry_delay(1, RetryBackoff::Linear),
            Duration::from_secs(120)
        );
        assert_eq!(
            next_retry_delay(4, RetryBackoff::Linear),
            Duration::from_secs(300)
        );
    }

    #[test]
    fn exponential_in_reasonable_range() {
        let d0 = next_retry_delay(0, RetryBackoff::Exponential);
        assert!(d0 >= Duration::from_secs(15) && d0 <= Duration::from_secs(45));

        let d3 = next_retry_delay(3, RetryBackoff::Exponential);
        // base=81, +15=96, jitter 最多 120 → 上限 216
        assert!(d3 >= Duration::from_secs(96) && d3 <= Duration::from_secs(216));
    }
}
