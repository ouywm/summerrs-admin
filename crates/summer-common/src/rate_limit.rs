use std::collections::{HashMap, VecDeque};
use std::net::{IpAddr, Ipv4Addr};
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use governor::clock::DefaultClock;
use governor::state::keyed::DefaultKeyedStateStore;
use governor::{Quota, RateLimiter};
use parking_lot::RwLock;
use summer_web::axum::extract::FromRequestParts;
use summer_web::axum::http::HeaderMap;
use summer_web::axum::http::request::Parts;
use summer_web::extractor::RequestPartsExt;

use crate::error::{ApiErrors, ApiResult};

type KeyedRateLimiter = RateLimiter<String, DefaultKeyedStateStore<String>, DefaultClock>;

const REDIS_TOKEN_BUCKET_SCRIPT: &str = include_str!("lua/rate_limit_token_bucket.lua");
const REDIS_FIXED_WINDOW_SCRIPT: &str = include_str!("lua/rate_limit_fixed_window.lua");
const REDIS_SLIDING_WINDOW_SCRIPT: &str = include_str!("lua/rate_limit_sliding_window.lua");
const REDIS_SCHEDULED_SLOT_SCRIPT: &str = include_str!("lua/rate_limit_scheduled_slot.lua");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateLimitPer {
    Second,
    Minute,
    Hour,
    Day,
}

impl RateLimitPer {
    pub fn to_quota(self, rate: u32) -> Quota {
        let rate = NonZeroU32::new(rate).expect("rate must be > 0");
        match self {
            Self::Second => Quota::per_second(rate),
            Self::Minute => Quota::per_minute(rate),
            Self::Hour => Quota::with_period(Duration::from_secs(3600))
                .expect("valid hour quota")
                .allow_burst(rate),
            Self::Day => Quota::with_period(Duration::from_secs(86400))
                .expect("valid day quota")
                .allow_burst(rate),
        }
    }

    pub fn window_seconds(self) -> u64 {
        match self {
            Self::Second => 1,
            Self::Minute => 60,
            Self::Hour => 3600,
            Self::Day => 86400,
        }
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
    TokenBucket,
    FixedWindow,
    SlidingWindow,
    LeakyBucket,
    ThrottleQueue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateLimitFailurePolicy {
    FailOpen,
    FailClosed,
    FallbackMemory,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RateLimitDecision {
    Allowed,
    Delayed(Duration),
    Rejected,
    BackendUnavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FixedWindowCounter {
    window_id: i64,
    count: u32,
}

#[derive(Clone)]
pub struct RateLimitContext {
    pub client_ip: IpAddr,
    pub user_id: Option<i64>,
    pub headers: HeaderMap,
    pub engine: RateLimitEngine,
}

impl RateLimitContext {
    pub fn extract_key(&self, key_type: RateLimitKeyType) -> String {
        match key_type {
            RateLimitKeyType::Global => "global".to_string(),
            RateLimitKeyType::Ip => self.client_ip.to_string(),
            RateLimitKeyType::User => self
                .user_id
                .map(|user_id| format!("user:{user_id}"))
                .unwrap_or_else(|| self.client_ip.to_string()),
            RateLimitKeyType::Header(name) => self
                .headers
                .get(name)
                .and_then(|value| value.to_str().ok())
                .unwrap_or("unknown")
                .to_string(),
        }
    }

    pub async fn check(&self, key: &str, config: RateLimitConfig, message: &str) -> ApiResult<()> {
        match self.engine.check(key, &config).await {
            RateLimitDecision::Allowed => Ok(()),
            RateLimitDecision::Delayed(delay) => {
                tokio::time::sleep(delay).await;
                Ok(())
            }
            RateLimitDecision::Rejected => Err(ApiErrors::TooManyRequests(message.to_string())),
            RateLimitDecision::BackendUnavailable => Err(ApiErrors::ServiceUnavailable(
                "限流服务暂时不可用，请稍后再试".to_string(),
            )),
        }
    }
}

impl<S: Send + Sync> FromRequestParts<S> for RateLimitContext {
    type Rejection = summer_web::error::WebError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let client_ip = axum_client_ip::ClientIp::from_request_parts(parts, state)
            .await
            .map(|axum_client_ip::ClientIp(ip)| ip)
            .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));

        let user_id = parts
            .extensions
            .get::<summer_auth::UserSession>()
            .map(|session| session.login_id.user_id);

        let headers = parts.headers.clone();
        let engine = if let Some(engine) = parts.extensions.get::<RateLimitEngine>().cloned() {
            engine
        } else {
            parts.get_component::<RateLimitEngine>()?
        };

        Ok(Self {
            client_ip,
            user_id,
            headers,
            engine,
        })
    }
}

impl summer_web::aide::OperationInput for RateLimitContext {}

#[derive(Clone)]
pub struct RateLimitEngine {
    memory_limiters: Arc<RwLock<HashMap<String, Arc<KeyedRateLimiter>>>>,
    fixed_window_counters: Arc<RwLock<HashMap<String, FixedWindowCounter>>>,
    sliding_window_logs: Arc<RwLock<HashMap<String, VecDeque<i64>>>>,
    schedule_states: Arc<RwLock<HashMap<String, i64>>>,
    redis: Option<summer_redis::Redis>,
}

impl RateLimitEngine {
    pub fn new(redis: Option<summer_redis::Redis>) -> Self {
        Self {
            memory_limiters: Arc::new(RwLock::new(HashMap::new())),
            fixed_window_counters: Arc::new(RwLock::new(HashMap::new())),
            sliding_window_logs: Arc::new(RwLock::new(HashMap::new())),
            schedule_states: Arc::new(RwLock::new(HashMap::new())),
            redis,
        }
    }

    async fn check(&self, key: &str, config: &RateLimitConfig) -> RateLimitDecision {
        match config.backend {
            RateLimitBackend::Memory => self.check_memory(key, config),
            RateLimitBackend::Redis => self.check_redis(key, config).await,
        }
    }

    fn check_memory(&self, key: &str, config: &RateLimitConfig) -> RateLimitDecision {
        match config.algorithm {
            RateLimitAlgorithm::TokenBucket => {
                if self.check_memory_token_bucket(key, config) {
                    RateLimitDecision::Allowed
                } else {
                    RateLimitDecision::Rejected
                }
            }
            RateLimitAlgorithm::FixedWindow => {
                if self.check_memory_fixed_window(key, config) {
                    RateLimitDecision::Allowed
                } else {
                    RateLimitDecision::Rejected
                }
            }
            RateLimitAlgorithm::SlidingWindow => {
                if self.check_memory_sliding_window(key, config) {
                    RateLimitDecision::Allowed
                } else {
                    RateLimitDecision::Rejected
                }
            }
            RateLimitAlgorithm::LeakyBucket => self.check_memory_scheduled_slot(key, config, 0),
            RateLimitAlgorithm::ThrottleQueue => {
                self.check_memory_scheduled_slot(key, config, config.max_wait_ms)
            }
        }
    }

    fn check_memory_token_bucket(&self, key: &str, config: &RateLimitConfig) -> bool {
        let limiter_key = format!(
            "{}:{}:{}:{}",
            config.algorithm.as_key_segment(),
            config.rate,
            config.window_seconds(),
            config.burst.max(1)
        );
        let limiter = {
            let mut limiters = self.memory_limiters.write();
            limiters
                .entry(limiter_key)
                .or_insert_with(|| Arc::new(RateLimiter::keyed(config.to_quota())))
                .clone()
        };

        limiter.check_key(&key.to_string()).is_ok()
    }

    fn check_memory_fixed_window(&self, key: &str, config: &RateLimitConfig) -> bool {
        let window_ms = config.window_millis();
        let window_id = current_time_millis().div_euclid(window_ms.max(1));
        let counter_key = format!(
            "{}:{}:{}:{}",
            config.algorithm.as_key_segment(),
            config.rate,
            config.window_seconds(),
            key
        );

        let mut counters = self.fixed_window_counters.write();
        let counter = counters.entry(counter_key).or_insert(FixedWindowCounter {
            window_id,
            count: 0,
        });

        if counter.window_id != window_id {
            counter.window_id = window_id;
            counter.count = 0;
        }

        if counter.count >= config.window_limit() {
            return false;
        }

        counter.count += 1;
        true
    }

    fn check_memory_sliding_window(&self, key: &str, config: &RateLimitConfig) -> bool {
        let now_ms = current_time_millis();
        let window_ms = config.window_millis();
        let log_key = format!(
            "{}:{}:{}:{}",
            config.algorithm.as_key_segment(),
            config.rate,
            config.window_seconds(),
            key
        );
        let mut logs = self.sliding_window_logs.write();
        let entries = logs.entry(log_key).or_default();

        while entries
            .front()
            .is_some_and(|ts| *ts <= now_ms.saturating_sub(window_ms))
        {
            entries.pop_front();
        }

        if entries.len() as u32 >= config.window_limit() {
            return false;
        }

        entries.push_back(now_ms);
        true
    }

    fn check_memory_scheduled_slot(
        &self,
        key: &str,
        config: &RateLimitConfig,
        max_wait_ms: u64,
    ) -> RateLimitDecision {
        let now_ms = current_time_millis();
        let interval_ms = config.request_interval_millis();
        let schedule_key = format!(
            "{}:{}:{}:{}:{}",
            config.algorithm.as_key_segment(),
            config.rate,
            config.window_seconds(),
            max_wait_ms,
            key
        );

        let mut states = self.schedule_states.write();
        let next_available_ms = states.entry(schedule_key).or_insert(now_ms);
        let scheduled_at_ms = (*next_available_ms).max(now_ms);
        let delay_ms = scheduled_at_ms.saturating_sub(now_ms) as u64;

        if delay_ms > max_wait_ms {
            return RateLimitDecision::Rejected;
        }

        *next_available_ms = scheduled_at_ms.saturating_add(interval_ms);
        Self::scheduled_delay_decision(delay_ms)
    }

    async fn check_redis(&self, key: &str, config: &RateLimitConfig) -> RateLimitDecision {
        let Some(redis) = &self.redis else {
            return self.handle_backend_failure(key, config);
        };

        let now_ms = current_time_millis();
        let expire_seconds = config.redis_expire_seconds() as i64;
        let redis_key = format!(
            "rate-limit:{}:{}:{}:{}:{}",
            config.algorithm.as_key_segment(),
            config.rate,
            config.window_seconds(),
            config.burst,
            key
        );
        let mut conn = redis.clone();

        let result = match config.algorithm {
            RateLimitAlgorithm::TokenBucket => {
                summer_redis::redis::Script::new(REDIS_TOKEN_BUCKET_SCRIPT)
                    .key(redis_key)
                    .arg(now_ms)
                    .arg(config.window_millis())
                    .arg(config.rate as i64)
                    .arg(config.burst.max(1) as i64)
                    .arg(expire_seconds)
                    .invoke_async::<i32>(&mut conn)
                    .await
                    .map(|value| {
                        if value == 1 {
                            RateLimitDecision::Allowed
                        } else {
                            RateLimitDecision::Rejected
                        }
                    })
            }
            RateLimitAlgorithm::FixedWindow => {
                summer_redis::redis::Script::new(REDIS_FIXED_WINDOW_SCRIPT)
                    .key(redis_key)
                    .arg(expire_seconds)
                    .arg(config.window_limit() as i64)
                    .invoke_async::<i32>(&mut conn)
                    .await
                    .map(|value| {
                        if value == 1 {
                            RateLimitDecision::Allowed
                        } else {
                            RateLimitDecision::Rejected
                        }
                    })
            }
            RateLimitAlgorithm::SlidingWindow => {
                summer_redis::redis::Script::new(REDIS_SLIDING_WINDOW_SCRIPT)
                    .key(redis_key)
                    .arg(now_ms)
                    .arg(config.window_millis())
                    .arg(config.window_limit() as i64)
                    .arg(format!("{now_ms}:{}", uuid::Uuid::new_v4()))
                    .arg(expire_seconds)
                    .invoke_async::<i32>(&mut conn)
                    .await
                    .map(|value| {
                        if value == 1 {
                            RateLimitDecision::Allowed
                        } else {
                            RateLimitDecision::Rejected
                        }
                    })
            }
            RateLimitAlgorithm::LeakyBucket => {
                summer_redis::redis::Script::new(REDIS_SCHEDULED_SLOT_SCRIPT)
                    .key(redis_key)
                    .arg(now_ms)
                    .arg(config.request_interval_millis())
                    .arg(0_i64)
                    .arg(expire_seconds)
                    .invoke_async::<i64>(&mut conn)
                    .await
                    .map(Self::scheduled_redis_result)
            }
            RateLimitAlgorithm::ThrottleQueue => {
                summer_redis::redis::Script::new(REDIS_SCHEDULED_SLOT_SCRIPT)
                    .key(redis_key)
                    .arg(now_ms)
                    .arg(config.request_interval_millis())
                    .arg(config.max_wait_ms as i64)
                    .arg(expire_seconds)
                    .invoke_async::<i64>(&mut conn)
                    .await
                    .map(Self::scheduled_redis_result)
            }
        };

        match result {
            Ok(decision) => decision,
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    key,
                    rate = config.rate,
                    burst = config.burst,
                    algorithm = %config.algorithm.as_key_segment(),
                    failure_policy = %config.failure_policy.as_key_segment(),
                    window_seconds = config.window_seconds(),
                    max_wait_ms = config.max_wait_ms,
                    "redis rate limit check failed"
                );
                self.handle_backend_failure(key, config)
            }
        }
    }

    fn handle_backend_failure(&self, key: &str, config: &RateLimitConfig) -> RateLimitDecision {
        let fallback = self.check_memory(key, config);
        Self::apply_failure_policy(config.failure_policy, fallback)
    }

    fn apply_failure_policy(
        policy: RateLimitFailurePolicy,
        fallback: RateLimitDecision,
    ) -> RateLimitDecision {
        match policy {
            RateLimitFailurePolicy::FailOpen => RateLimitDecision::Allowed,
            RateLimitFailurePolicy::FailClosed => RateLimitDecision::BackendUnavailable,
            RateLimitFailurePolicy::FallbackMemory => fallback,
        }
    }

    fn scheduled_redis_result(value: i64) -> RateLimitDecision {
        if value < 0 {
            RateLimitDecision::Rejected
        } else {
            Self::scheduled_delay_decision(value as u64)
        }
    }

    fn scheduled_delay_decision(delay_ms: u64) -> RateLimitDecision {
        if delay_ms == 0 {
            RateLimitDecision::Allowed
        } else {
            RateLimitDecision::Delayed(Duration::from_millis(delay_ms))
        }
    }
}

impl RateLimitConfig {
    fn to_quota(&self) -> Quota {
        self.per
            .to_quota(self.rate.max(1))
            .allow_burst(NonZeroU32::new(self.burst.max(1)).expect("burst must be > 0"))
    }

    fn window_seconds(&self) -> u64 {
        self.per.window_seconds()
    }

    fn window_millis(&self) -> i64 {
        (self.window_seconds() * 1000) as i64
    }

    fn window_limit(&self) -> u32 {
        self.rate.max(1)
    }

    fn request_interval_millis(&self) -> i64 {
        let window_ms = self.window_millis().max(1);
        let rate = self.rate.max(1) as i64;
        (window_ms + rate - 1) / rate
    }

    fn redis_expire_seconds(&self) -> u64 {
        match self.algorithm {
            RateLimitAlgorithm::TokenBucket => {
                let rate = self.rate.max(1) as u64;
                let burst = self.burst.max(1) as u64;
                let refill_full_seconds =
                    self.window_seconds().saturating_mul(burst).div_ceil(rate);
                refill_full_seconds.max(1) * 2
            }
            RateLimitAlgorithm::FixedWindow | RateLimitAlgorithm::SlidingWindow => {
                self.window_seconds().max(1) * 2
            }
            RateLimitAlgorithm::LeakyBucket => {
                let hold_ms = self.request_interval_millis().max(1) as u64;
                hold_ms.div_ceil(1000).max(1) * 2
            }
            RateLimitAlgorithm::ThrottleQueue => {
                let hold_ms =
                    self.request_interval_millis().max(1) as u64 + self.max_wait_ms.max(1);
                hold_ms.div_ceil(1000).max(1) * 2
            }
        }
    }
}

impl RateLimitAlgorithm {
    fn as_key_segment(self) -> &'static str {
        match self {
            Self::TokenBucket => "token_bucket",
            Self::FixedWindow => "fixed_window",
            Self::SlidingWindow => "sliding_window",
            Self::LeakyBucket => "leaky_bucket",
            Self::ThrottleQueue => "throttle_queue",
        }
    }
}

impl RateLimitFailurePolicy {
    fn as_key_segment(self) -> &'static str {
        match self {
            Self::FailOpen => "fail_open",
            Self::FailClosed => "fail_closed",
            Self::FallbackMemory => "fallback_memory",
        }
    }
}

fn current_time_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_allowed(decision: RateLimitDecision) -> bool {
        matches!(
            decision,
            RateLimitDecision::Allowed | RateLimitDecision::Delayed(_)
        )
    }

    fn delayed_ms(decision: RateLimitDecision) -> Option<u64> {
        match decision {
            RateLimitDecision::Delayed(delay) => Some(delay.as_millis() as u64),
            _ => None,
        }
    }

    fn test_context(client_ip: IpAddr, user_id: Option<i64>) -> RateLimitContext {
        RateLimitContext {
            client_ip,
            user_id,
            headers: HeaderMap::new(),
            engine: RateLimitEngine::new(None),
        }
    }

    #[tokio::test]
    async fn extract_user_key_falls_back_to_ip_when_session_missing() {
        let ctx = test_context("127.0.0.1".parse().unwrap(), None);
        assert_eq!(ctx.extract_key(RateLimitKeyType::User), "127.0.0.1");
    }

    #[tokio::test]
    async fn extract_user_key_uses_user_id_when_present() {
        let ctx = test_context("127.0.0.1".parse().unwrap(), Some(42));
        assert_eq!(ctx.extract_key(RateLimitKeyType::User), "user:42");
    }

    #[tokio::test]
    async fn memory_token_bucket_rejects_after_burst() {
        let engine = RateLimitEngine::new(None);
        let config = RateLimitConfig {
            rate: 2,
            per: RateLimitPer::Second,
            burst: 2,
            backend: RateLimitBackend::Memory,
            algorithm: RateLimitAlgorithm::TokenBucket,
            failure_policy: RateLimitFailurePolicy::FailOpen,
            max_wait_ms: 0,
        };

        assert!(is_allowed(engine.check("memory:token", &config).await));
        assert!(is_allowed(engine.check("memory:token", &config).await));
        assert!(!is_allowed(engine.check("memory:token", &config).await));
    }

    #[tokio::test]
    async fn memory_sliding_window_rejects_third_request_within_window() {
        let engine = RateLimitEngine::new(None);
        let config = RateLimitConfig {
            rate: 2,
            per: RateLimitPer::Second,
            burst: 2,
            backend: RateLimitBackend::Memory,
            algorithm: RateLimitAlgorithm::SlidingWindow,
            failure_policy: RateLimitFailurePolicy::FailOpen,
            max_wait_ms: 0,
        };

        assert!(is_allowed(engine.check("memory:sliding", &config).await));
        assert!(is_allowed(engine.check("memory:sliding", &config).await));
        assert!(!is_allowed(engine.check("memory:sliding", &config).await));

        tokio::time::sleep(Duration::from_millis(1100)).await;
        assert!(is_allowed(engine.check("memory:sliding", &config).await));
    }

    #[tokio::test]
    async fn memory_fixed_window_rejects_third_request_within_window() {
        let engine = RateLimitEngine::new(None);
        let config = RateLimitConfig {
            rate: 2,
            per: RateLimitPer::Second,
            burst: 2,
            backend: RateLimitBackend::Memory,
            algorithm: RateLimitAlgorithm::FixedWindow,
            failure_policy: RateLimitFailurePolicy::FailOpen,
            max_wait_ms: 0,
        };

        assert!(is_allowed(engine.check("memory:fixed", &config).await));
        assert!(is_allowed(engine.check("memory:fixed", &config).await));
        assert!(!is_allowed(engine.check("memory:fixed", &config).await));
    }

    #[tokio::test]
    async fn memory_leaky_bucket_rejects_until_interval_passes() {
        let engine = RateLimitEngine::new(None);
        let config = RateLimitConfig {
            rate: 1,
            per: RateLimitPer::Second,
            burst: 1,
            backend: RateLimitBackend::Memory,
            algorithm: RateLimitAlgorithm::LeakyBucket,
            failure_policy: RateLimitFailurePolicy::FailOpen,
            max_wait_ms: 0,
        };

        assert!(is_allowed(engine.check("memory:leaky", &config).await));
        assert!(!is_allowed(engine.check("memory:leaky", &config).await));

        tokio::time::sleep(Duration::from_millis(1100)).await;
        assert!(is_allowed(engine.check("memory:leaky", &config).await));
    }

    #[tokio::test]
    async fn memory_throttle_queue_delays_second_request() {
        let engine = RateLimitEngine::new(None);
        let config = RateLimitConfig {
            rate: 1,
            per: RateLimitPer::Second,
            burst: 1,
            backend: RateLimitBackend::Memory,
            algorithm: RateLimitAlgorithm::ThrottleQueue,
            failure_policy: RateLimitFailurePolicy::FailOpen,
            max_wait_ms: 1500,
        };

        assert!(is_allowed(engine.check("memory:queue", &config).await));
        let delayed = engine.check("memory:queue", &config).await;
        assert!(delayed_ms(delayed).is_some_and(|ms| ms >= 900));
        assert!(!is_allowed(engine.check("memory:queue", &config).await));
    }

    #[tokio::test]
    async fn redis_token_bucket_uses_burst_capacity() {
        let redis: summer_redis::Redis =
            match summer_redis::redis::Client::open("redis://127.0.0.1/") {
                Ok(client) => match client.get_connection_manager().await {
                    Ok(redis) => redis,
                    Err(_) => return,
                },
                Err(_) => return,
            };

        let engine = RateLimitEngine::new(Some(redis));
        let config = RateLimitConfig {
            rate: 1,
            per: RateLimitPer::Second,
            burst: 3,
            backend: RateLimitBackend::Redis,
            algorithm: RateLimitAlgorithm::TokenBucket,
            failure_policy: RateLimitFailurePolicy::FailOpen,
            max_wait_ms: 0,
        };
        let key = format!("redis:token:{}", uuid::Uuid::new_v4());

        assert!(is_allowed(engine.check(&key, &config).await));
        assert!(is_allowed(engine.check(&key, &config).await));
        assert!(is_allowed(engine.check(&key, &config).await));
        assert!(!is_allowed(engine.check(&key, &config).await));
    }

    #[tokio::test]
    async fn redis_sliding_window_rejects_third_request_within_window() {
        let redis: summer_redis::Redis =
            match summer_redis::redis::Client::open("redis://127.0.0.1/") {
                Ok(client) => match client.get_connection_manager().await {
                    Ok(redis) => redis,
                    Err(_) => return,
                },
                Err(_) => return,
            };

        let engine = RateLimitEngine::new(Some(redis));
        let config = RateLimitConfig {
            rate: 2,
            per: RateLimitPer::Second,
            burst: 2,
            backend: RateLimitBackend::Redis,
            algorithm: RateLimitAlgorithm::SlidingWindow,
            failure_policy: RateLimitFailurePolicy::FailOpen,
            max_wait_ms: 0,
        };
        let key = format!("redis:sliding:{}", uuid::Uuid::new_v4());

        assert!(is_allowed(engine.check(&key, &config).await));
        assert!(is_allowed(engine.check(&key, &config).await));
        assert!(!is_allowed(engine.check(&key, &config).await));

        tokio::time::sleep(Duration::from_millis(1100)).await;
        assert!(is_allowed(engine.check(&key, &config).await));
    }

    #[tokio::test]
    async fn redis_fixed_window_rejects_third_request_within_window() {
        let redis: summer_redis::Redis =
            match summer_redis::redis::Client::open("redis://127.0.0.1/") {
                Ok(client) => match client.get_connection_manager().await {
                    Ok(redis) => redis,
                    Err(_) => return,
                },
                Err(_) => return,
            };

        let engine = RateLimitEngine::new(Some(redis));
        let config = RateLimitConfig {
            rate: 2,
            per: RateLimitPer::Second,
            burst: 2,
            backend: RateLimitBackend::Redis,
            algorithm: RateLimitAlgorithm::FixedWindow,
            failure_policy: RateLimitFailurePolicy::FailOpen,
            max_wait_ms: 0,
        };
        let key = format!("redis:fixed:{}", uuid::Uuid::new_v4());

        assert!(is_allowed(engine.check(&key, &config).await));
        assert!(is_allowed(engine.check(&key, &config).await));
        assert!(!is_allowed(engine.check(&key, &config).await));
    }

    #[tokio::test]
    async fn redis_leaky_bucket_rejects_until_interval_passes() {
        let redis: summer_redis::Redis =
            match summer_redis::redis::Client::open("redis://127.0.0.1/") {
                Ok(client) => match client.get_connection_manager().await {
                    Ok(redis) => redis,
                    Err(_) => return,
                },
                Err(_) => return,
            };

        let engine = RateLimitEngine::new(Some(redis));
        let config = RateLimitConfig {
            rate: 1,
            per: RateLimitPer::Second,
            burst: 1,
            backend: RateLimitBackend::Redis,
            algorithm: RateLimitAlgorithm::LeakyBucket,
            failure_policy: RateLimitFailurePolicy::FailOpen,
            max_wait_ms: 0,
        };
        let key = format!("redis:leaky:{}", uuid::Uuid::new_v4());

        assert!(is_allowed(engine.check(&key, &config).await));
        assert!(!is_allowed(engine.check(&key, &config).await));

        tokio::time::sleep(Duration::from_millis(1100)).await;
        assert!(is_allowed(engine.check(&key, &config).await));
    }

    #[tokio::test]
    async fn redis_throttle_queue_delays_second_request() {
        let redis: summer_redis::Redis =
            match summer_redis::redis::Client::open("redis://127.0.0.1/") {
                Ok(client) => match client.get_connection_manager().await {
                    Ok(redis) => redis,
                    Err(_) => return,
                },
                Err(_) => return,
            };

        let engine = RateLimitEngine::new(Some(redis));
        let config = RateLimitConfig {
            rate: 1,
            per: RateLimitPer::Second,
            burst: 1,
            backend: RateLimitBackend::Redis,
            algorithm: RateLimitAlgorithm::ThrottleQueue,
            failure_policy: RateLimitFailurePolicy::FailOpen,
            max_wait_ms: 1500,
        };
        let key = format!("redis:queue:{}", uuid::Uuid::new_v4());

        assert!(is_allowed(engine.check(&key, &config).await));
        let delayed = engine.check(&key, &config).await;
        assert!(delayed_ms(delayed).is_some_and(|ms| ms >= 900));
        assert!(!is_allowed(engine.check(&key, &config).await));
    }

    #[test]
    fn fail_open_allows_on_backend_failure() {
        assert!(matches!(
            RateLimitEngine::apply_failure_policy(
                RateLimitFailurePolicy::FailOpen,
                RateLimitDecision::Rejected,
            ),
            RateLimitDecision::Allowed
        ));
    }

    #[test]
    fn fail_closed_returns_backend_unavailable_on_backend_failure() {
        assert!(matches!(
            RateLimitEngine::apply_failure_policy(
                RateLimitFailurePolicy::FailClosed,
                RateLimitDecision::Allowed,
            ),
            RateLimitDecision::BackendUnavailable
        ));
    }

    #[test]
    fn fallback_memory_uses_memory_result_on_backend_failure() {
        assert!(matches!(
            RateLimitEngine::apply_failure_policy(
                RateLimitFailurePolicy::FallbackMemory,
                RateLimitDecision::Allowed,
            ),
            RateLimitDecision::Allowed
        ));
        assert!(matches!(
            RateLimitEngine::apply_failure_policy(
                RateLimitFailurePolicy::FallbackMemory,
                RateLimitDecision::Delayed(Duration::from_millis(10)),
            ),
            RateLimitDecision::Delayed(_)
        ));
        assert!(matches!(
            RateLimitEngine::apply_failure_policy(
                RateLimitFailurePolicy::FallbackMemory,
                RateLimitDecision::Rejected,
            ),
            RateLimitDecision::Rejected
        ));
    }
}
