use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr};
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

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

const REDIS_SLIDING_WINDOW_SCRIPT: &str = r#"
redis.call("ZREMRANGEBYSCORE", KEYS[1], "-inf", ARGV[1] - ARGV[2])
local current = redis.call("ZCARD", KEYS[1])
if current >= tonumber(ARGV[3]) then
    redis.call("EXPIRE", KEYS[1], ARGV[5])
    return 0
end
redis.call("ZADD", KEYS[1], ARGV[1], ARGV[4])
redis.call("EXPIRE", KEYS[1], ARGV[5])
return 1
"#;

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
            Self::Hour => Quota::with_period(std::time::Duration::from_secs(3600))
                .expect("valid hour quota")
                .allow_burst(rate),
            Self::Day => Quota::with_period(std::time::Duration::from_secs(86400))
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

#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    pub rate: u32,
    pub per: RateLimitPer,
    pub burst: u32,
    pub backend: RateLimitBackend,
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
        if self.engine.check(key, &config).await {
            Ok(())
        } else {
            Err(ApiErrors::TooManyRequests(message.to_string()))
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
    redis: Option<summer_redis::Redis>,
}

impl RateLimitEngine {
    pub fn new(redis: Option<summer_redis::Redis>) -> Self {
        Self {
            memory_limiters: Arc::new(RwLock::new(HashMap::new())),
            redis,
        }
    }

    pub async fn check(&self, key: &str, config: &RateLimitConfig) -> bool {
        match config.backend {
            RateLimitBackend::Memory => self.check_memory(key, config),
            RateLimitBackend::Redis => self.check_redis(key, config).await,
        }
    }

    fn check_memory(&self, _key: &str, config: &RateLimitConfig) -> bool {
        let limiter_key = format!(
            "{}:{}:{}",
            config.rate,
            config.window_seconds(),
            config.burst
        );
        let limiter = {
            let mut limiters = self.memory_limiters.write();
            limiters
                .entry(limiter_key)
                .or_insert_with(|| Arc::new(RateLimiter::keyed(config.to_quota())))
                .clone()
        };

        limiter.check_key(&_key.to_string()).is_ok()
    }

    async fn check_redis(&self, key: &str, config: &RateLimitConfig) -> bool {
        let Some(redis) = &self.redis else {
            return self.check_memory(key, config);
        };

        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis() as i64)
            .unwrap_or_default();
        let window_ms = (config.window_seconds() * 1000) as i64;
        let expire_seconds = config.window_seconds().max(1) * 2;
        let member = format!("{now_ms}:{}", uuid::Uuid::new_v4());
        let redis_key = format!(
            "rate-limit:{}:{}:{}:{}",
            config.rate,
            config.window_seconds(),
            config.burst,
            key
        );
        let mut conn = redis.clone();

        summer_redis::redis::Script::new(REDIS_SLIDING_WINDOW_SCRIPT)
            .key(redis_key)
            .arg(now_ms)
            .arg(window_ms)
            .arg(config.rate as i64)
            .arg(member)
            .arg(expire_seconds as i64)
            .invoke_async::<i32>(&mut conn)
            .await
            .map(|result| result == 1)
            .unwrap_or_else(|_| self.check_memory(key, config))
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
}

#[cfg(test)]
mod tests {
    use super::*;

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
    async fn memory_limiter_rejects_after_burst() {
        let engine = RateLimitEngine::new(None);
        let config = RateLimitConfig {
            rate: 2,
            per: RateLimitPer::Second,
            burst: 2,
            backend: RateLimitBackend::Memory,
        };

        assert!(engine.check("ip:1", &config).await);
        assert!(engine.check("ip:1", &config).await);
        assert!(!engine.check("ip:1", &config).await);
    }

    #[tokio::test]
    async fn memory_limiter_uses_burst_capacity() {
        let engine = RateLimitEngine::new(None);
        let config = RateLimitConfig {
            rate: 1,
            per: RateLimitPer::Second,
            burst: 3,
            backend: RateLimitBackend::Memory,
        };

        assert!(engine.check("ip:burst", &config).await);
        assert!(engine.check("ip:burst", &config).await);
        assert!(engine.check("ip:burst", &config).await);
        assert!(!engine.check("ip:burst", &config).await);
    }

    #[tokio::test]
    async fn memory_limiter_refills_at_rate_not_burst() {
        let engine = RateLimitEngine::new(None);
        let config = RateLimitConfig {
            rate: 1,
            per: RateLimitPer::Second,
            burst: 3,
            backend: RateLimitBackend::Memory,
        };

        assert!(engine.check("ip:refill", &config).await);
        assert!(engine.check("ip:refill", &config).await);
        assert!(engine.check("ip:refill", &config).await);
        assert!(!engine.check("ip:refill", &config).await);

        tokio::time::sleep(std::time::Duration::from_millis(1100)).await;

        assert!(engine.check("ip:refill", &config).await);
        assert!(!engine.check("ip:refill", &config).await);
    }

    #[tokio::test]
    async fn redis_backend_is_shared_across_engines() {
        let redis: summer_redis::Redis =
            match summer_redis::redis::Client::open("redis://127.0.0.1/") {
                Ok(client) => match client.get_connection_manager().await {
                    Ok(redis) => redis,
                    Err(_) => return,
                },
                Err(_) => return,
            };

        let engine_a = RateLimitEngine::new(Some(redis.clone()));
        let engine_b = RateLimitEngine::new(Some(redis));
        let config = RateLimitConfig {
            rate: 2,
            per: RateLimitPer::Second,
            burst: 2,
            backend: RateLimitBackend::Redis,
        };

        assert!(engine_a.check("redis:shared", &config).await);
        assert!(engine_b.check("redis:shared", &config).await);
        assert!(!engine_a.check("redis:shared", &config).await);
    }
}
