//! 限流配置类型：算法 / key 类型 / 后端 / 失败策略 / 执行模式 / 完整配置。

use crate::rate_limit::algorithms::sanitize_user_key;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateLimitPer {
    Second,
    Minute,
    Hour,
    Day,
}

impl RateLimitPer {
    pub fn window_seconds(self) -> u64 {
        match self {
            Self::Second => 1,
            Self::Minute => 60,
            Self::Hour => 3600,
            Self::Day => 86400,
        }
    }

    pub fn window_millis(self) -> i64 {
        (self.window_seconds() * 1000) as i64
    }
}

#[derive(Debug, Clone)]
pub enum RateLimitKeyType {
    Global,
    Ip,
    User,
    Header(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateLimitBackend {
    Memory,
    Redis,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateLimitAlgorithm {
    /// 兼容选项；内核同 GCRA，burst 默认 = rate。
    TokenBucket,
    /// 显式 GCRA。
    Gcra,
    /// 内核 ScheduledSlot(max_wait=0)。
    LeakyBucket,
    /// 内核 ScheduledSlot(max_wait>0)。
    ThrottleQueue,
    /// 计数器按自然窗口边界滚动。
    FixedWindow,
    /// 时间戳日志。
    SlidingWindow,
}

impl RateLimitAlgorithm {
    pub fn as_key_segment(self) -> &'static str {
        match self {
            Self::TokenBucket => "token_bucket",
            Self::Gcra => "gcra",
            Self::LeakyBucket => "leaky_bucket",
            Self::ThrottleQueue => "throttle_queue",
            Self::FixedWindow => "fixed_window",
            Self::SlidingWindow => "sliding_window",
        }
    }

    pub(crate) fn uses_gcra(self) -> bool {
        matches!(self, Self::TokenBucket | Self::Gcra)
    }

    pub(crate) fn uses_scheduled_slot(self) -> bool {
        matches!(self, Self::LeakyBucket | Self::ThrottleQueue)
    }

    /// 该算法是否支持 cost-based 限流。
    pub fn supports_cost(self) -> bool {
        self.uses_gcra()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateLimitFailurePolicy {
    /// Redis 失败时直接放行（仅记录 stats / log）。
    FailOpen,
    /// Redis 失败时返回 503。
    FailClosed,
    /// Redis 失败时跌到内存桶（多实例下语义降级，仅当前进程隔离）。
    FallbackMemory,
}

impl RateLimitFailurePolicy {
    pub(crate) fn as_key_segment(self) -> &'static str {
        match self {
            Self::FailOpen => "fail_open",
            Self::FailClosed => "fail_closed",
            Self::FallbackMemory => "fallback_memory",
        }
    }
}

/// 限流执行模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RateLimitMode {
    /// 命中即拒绝（标准模式）
    #[default]
    Enforce,
    /// 命中只记日志，不真的拒绝（灰度评估、上线安全网）。
    ///
    /// **重要：Shadow 与 Enforce 共享同一个桶状态**（mode 不参与 cache key 生成）。
    /// 这是有意设计 —— 让 shadow 在真实流量里观察"如果开启限流会拒多少"，否则
    /// shadow 桶独立计数会让灰度评估失真。但带来的副作用是：
    ///
    /// - 长期跑 shadow 推进过的 GCRA TAT，切换 enforce 后会立刻拒一段时间，建议
    ///   切换前调一次 [`crate::rate_limit::RateLimitEngine::reset_key`]。
    /// - 不能在 shadow 和 enforce 之间无缝灰度（如果是同一 key + 同一桶配置）。
    Shadow,
}

#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    pub rate: u32,
    pub per: RateLimitPer,
    pub burst: u32,
    pub backend: RateLimitBackend,
    pub algorithm: RateLimitAlgorithm,
    pub failure_policy: RateLimitFailurePolicy,
    pub max_wait_ms: u64,
    pub mode: RateLimitMode,
}

impl RateLimitConfig {
    /// GCRA / ScheduledSlot 的"每单位理论间隔"，整数毫秒，向上取整防 0。
    pub fn emission_interval_millis(&self) -> i64 {
        let window_ms = self.per.window_millis().max(1);
        let rate = self.rate.max(1) as i64;
        // (window_ms + rate - 1) / rate，加法用 saturating 防 overflow。
        window_ms.saturating_add(rate - 1) / rate
    }

    pub fn window_seconds(&self) -> u64 {
        self.per.window_seconds()
    }

    pub fn window_millis(&self) -> i64 {
        self.per.window_millis()
    }

    pub fn window_limit(&self) -> u32 {
        self.rate.max(1)
    }

    /// LeakyBucket 强制 burst=1；其它走 GCRA 的算法用配置 burst（默认 = rate）。
    pub fn effective_burst(&self) -> u32 {
        match self.algorithm {
            RateLimitAlgorithm::LeakyBucket => 1,
            _ => self.burst.max(1),
        }
    }

    /// Redis key 的 TTL（秒）。
    pub fn redis_expire_seconds(&self) -> u64 {
        match self.algorithm {
            RateLimitAlgorithm::TokenBucket | RateLimitAlgorithm::Gcra => {
                let burst = self.effective_burst().max(1) as u64;
                let interval = self.emission_interval_millis().max(1) as u64;
                (burst.saturating_mul(interval).div_ceil(1000)).max(2) * 2
            }
            RateLimitAlgorithm::FixedWindow | RateLimitAlgorithm::SlidingWindow => {
                self.window_seconds().max(1) * 2
            }
            RateLimitAlgorithm::LeakyBucket => {
                let interval = self.emission_interval_millis().max(1) as u64;
                interval.div_ceil(1000).max(1) * 2
            }
            RateLimitAlgorithm::ThrottleQueue => {
                let interval = self.emission_interval_millis().max(1) as u64;
                let max_wait = self.max_wait_ms.max(1);
                (interval.saturating_add(max_wait)).div_ceil(1000).max(1) * 2
            }
        }
    }

    /// `algorithm:rate:window_seconds:burst:max_wait_ms`，参与 cache / Redis key
    /// 拼接，让不同参数的桶天然隔离。
    pub fn signature(&self) -> String {
        format!(
            "{}:{}:{}:{}:{}",
            self.algorithm.as_key_segment(),
            self.rate,
            self.window_seconds(),
            self.effective_burst(),
            self.max_wait_ms,
        )
    }
}

/// `{config.signature()}:{sanitized_key}` —— memory cache 的 key 生成 helper。
pub(crate) fn cache_key_for(config: &RateLimitConfig, key: &str) -> String {
    format!("{}:{}", config.signature(), sanitize_user_key(key))
}
