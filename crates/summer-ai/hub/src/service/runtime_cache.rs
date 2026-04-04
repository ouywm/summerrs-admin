use anyhow::Context;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use summer::plugin::Service;
use summer_common::error::{ApiErrors, ApiResult};
use summer_redis::Redis;
use summer_redis::redis;
use summer_redis::redis::AsyncCommands;

const HASH_COUNTER_MUTATION_LUA: &str = r#"
local ttl = tonumber(ARGV[1])
local changed = 0
local index = 2
local values = {}

while index <= #ARGV do
    local field = ARGV[index]
    local delta = tonumber(ARGV[index + 1])
    local current = tonumber(redis.call('HGET', KEYS[1], field) or '0')
    local next_value = current + delta
    if next_value < 0 then
        next_value = 0
    end
    if next_value ~= current then
        changed = 1
    end
    if next_value == 0 then
        redis.call('HDEL', KEYS[1], field)
    else
        redis.call('HSET', KEYS[1], field, next_value)
    end
    table.insert(values, next_value)
    index = index + 2
end

if redis.call('HLEN', KEYS[1]) == 0 then
    redis.call('DEL', KEYS[1])
elseif ttl > 0 then
    redis.call('EXPIRE', KEYS[1], ttl)
end

local result = { changed }
for i = 1, #values do
    table.insert(result, values[i])
end
return result
"#;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HashCounterMutationResult {
    pub changed: bool,
    pub values: Vec<i64>,
}

#[derive(Clone, Service)]
pub struct RuntimeCacheService {
    #[inject(component)]
    redis: Redis,
}

impl RuntimeCacheService {
    pub fn new(redis: Redis) -> Self {
        Self { redis }
    }

    pub fn connection(&self) -> Redis {
        self.redis.clone()
    }

    pub async fn get_json<T>(&self, key: &str) -> ApiResult<Option<T>>
    where
        T: DeserializeOwned,
    {
        let mut conn = self.redis.clone();
        let raw: Option<String> = conn
            .get(key)
            .await
            .with_context(|| format!("failed to get cache key {key}"))
            .map_err(ApiErrors::Internal)?;

        raw.map(|raw| {
            serde_json::from_str(&raw)
                .with_context(|| format!("failed to deserialize cache value for key {key}"))
                .map_err(ApiErrors::Internal)
        })
        .transpose()
    }

    pub async fn set_json<T>(&self, key: &str, value: &T, ttl_seconds: u64) -> ApiResult<()>
    where
        T: Serialize,
    {
        let payload = serde_json::to_string(value)
            .with_context(|| format!("failed to serialize cache value for key {key}"))
            .map_err(ApiErrors::Internal)?;
        let mut conn = self.redis.clone();

        if ttl_seconds > 0 {
            conn.set_ex::<_, _, ()>(key, payload, ttl_seconds)
                .await
                .with_context(|| format!("failed to set cache key {key}"))
                .map_err(ApiErrors::Internal)?;
        } else {
            conn.set::<_, _, ()>(key, payload)
                .await
                .with_context(|| format!("failed to set cache key {key}"))
                .map_err(ApiErrors::Internal)?;
        }

        Ok(())
    }

    pub async fn set_json_if_absent<T>(
        &self,
        key: &str,
        value: &T,
        ttl_seconds: u64,
    ) -> ApiResult<bool>
    where
        T: Serialize,
    {
        let payload = serde_json::to_string(value)
            .with_context(|| format!("failed to serialize cache value for key {key}"))
            .map_err(ApiErrors::Internal)?;
        let mut conn = self.redis.clone();

        let result: Option<String> = redis::cmd("SET")
            .arg(key)
            .arg(payload)
            .arg("NX")
            .arg("EX")
            .arg(ttl_seconds)
            .query_async(&mut conn)
            .await
            .with_context(|| format!("failed to set cache key {key} if absent"))
            .map_err(ApiErrors::Internal)?;

        Ok(result.is_some())
    }

    pub async fn delete(&self, key: &str) -> ApiResult<()> {
        let mut conn = self.redis.clone();
        conn.del::<_, ()>(key)
            .await
            .with_context(|| format!("failed to delete cache key {key}"))
            .map_err(ApiErrors::Internal)?;
        Ok(())
    }

    pub async fn get_i64(&self, key: &str) -> ApiResult<Option<i64>> {
        let mut conn = self.redis.clone();
        conn.get(key)
            .await
            .with_context(|| format!("failed to get integer cache key {key}"))
            .map_err(ApiErrors::Internal)
    }

    pub async fn incr(&self, key: &str) -> ApiResult<i64> {
        let mut conn = self.redis.clone();
        conn.incr(key, 1_i64)
            .await
            .with_context(|| format!("failed to increment cache key {key}"))
            .map_err(ApiErrors::Internal)
    }

    pub async fn incr_by(&self, key: &str, value: i64) -> ApiResult<i64> {
        let mut conn = self.redis.clone();
        conn.incr(key, value)
            .await
            .with_context(|| format!("failed to increment cache key {key} by {value}"))
            .map_err(ApiErrors::Internal)
    }

    pub async fn decr_by(&self, key: &str, value: i64) -> ApiResult<i64> {
        let mut conn = self.redis.clone();
        conn.decr(key, value)
            .await
            .with_context(|| format!("failed to decrement cache key {key} by {value}"))
            .map_err(ApiErrors::Internal)
    }

    pub async fn expire(&self, key: &str, ttl_seconds: i64) -> ApiResult<()> {
        let mut conn = self.redis.clone();
        conn.expire::<_, ()>(key, ttl_seconds)
            .await
            .with_context(|| format!("failed to expire cache key {key}"))
            .map_err(ApiErrors::Internal)?;
        Ok(())
    }

    /// Atomically increment a key by 1 and set TTL if the key is new.
    ///
    /// Uses a Lua script to ensure INCR and EXPIRE are executed atomically,
    /// preventing keys from persisting forever if a crash occurs between the
    /// two operations.
    pub async fn incr_with_expire(&self, key: &str, ttl_seconds: i64) -> ApiResult<i64> {
        const LUA: &str = r#"
local current = redis.call('INCR', KEYS[1])
if current == 1 then
    redis.call('EXPIRE', KEYS[1], ARGV[1])
end
return current
"#;
        let mut conn = self.redis.clone();
        redis::cmd("EVAL")
            .arg(LUA)
            .arg(1)
            .arg(key)
            .arg(ttl_seconds)
            .query_async(&mut conn)
            .await
            .with_context(|| format!("failed to atomic incr+expire cache key {key}"))
            .map_err(ApiErrors::Internal)
    }

    /// Atomically increment a key by `value` and set TTL if the key is new.
    pub async fn incr_by_with_expire(
        &self,
        key: &str,
        value: i64,
        ttl_seconds: i64,
    ) -> ApiResult<i64> {
        const LUA: &str = r#"
local current = redis.call('INCRBY', KEYS[1], ARGV[1])
if current == tonumber(ARGV[1]) then
    redis.call('EXPIRE', KEYS[1], ARGV[2])
end
return current
"#;
        let mut conn = self.redis.clone();
        redis::cmd("EVAL")
            .arg(LUA)
            .arg(1)
            .arg(key)
            .arg(value)
            .arg(ttl_seconds)
            .query_async(&mut conn)
            .await
            .with_context(|| format!("failed to atomic incr_by+expire cache key {key}"))
            .map_err(ApiErrors::Internal)
    }

    pub async fn hash_get_all_i64(&self, key: &str) -> ApiResult<HashMap<String, i64>> {
        let mut conn = self.redis.clone();
        conn.hgetall(key)
            .await
            .with_context(|| format!("failed to load hash cache key {key}"))
            .map_err(ApiErrors::Internal)
    }

    pub async fn mutate_hash_counters(
        &self,
        key: &str,
        ttl_seconds: u64,
        deltas: &[(&str, i64)],
    ) -> ApiResult<HashCounterMutationResult> {
        if deltas.is_empty() {
            return Ok(HashCounterMutationResult {
                changed: false,
                values: Vec::new(),
            });
        }

        let mut conn = self.redis.clone();
        let mut cmd = redis::cmd("EVAL");
        cmd.arg(HASH_COUNTER_MUTATION_LUA)
            .arg(1)
            .arg(key)
            .arg(ttl_seconds);
        for (field, delta) in deltas {
            cmd.arg(field).arg(*delta);
        }

        let raw: Vec<i64> = cmd
            .query_async(&mut conn)
            .await
            .with_context(|| format!("failed to mutate hash counters for key {key}"))
            .map_err(ApiErrors::Internal)?;

        let changed = raw.first().copied().unwrap_or_default() != 0;
        Ok(HashCounterMutationResult {
            changed,
            values: raw.into_iter().skip(1).collect(),
        })
    }

    pub async fn sorted_set_add(&self, key: &str, score: i64, member: &str) -> ApiResult<()> {
        let mut conn = self.redis.clone();
        redis::cmd("ZADD")
            .arg(key)
            .arg(score)
            .arg(member)
            .query_async::<()>(&mut conn)
            .await
            .with_context(|| format!("failed to add sorted-set member for key {key}"))
            .map_err(ApiErrors::Internal)?;
        Ok(())
    }

    pub async fn sorted_set_count_by_score(
        &self,
        key: &str,
        min_score: i64,
        max_score: i64,
    ) -> ApiResult<i64> {
        let mut conn = self.redis.clone();
        redis::cmd("ZCOUNT")
            .arg(key)
            .arg(min_score)
            .arg(max_score)
            .query_async(&mut conn)
            .await
            .with_context(|| format!("failed to count sorted-set members for key {key}"))
            .map_err(ApiErrors::Internal)
    }

    pub async fn sorted_set_remove_by_score(
        &self,
        key: &str,
        min_score: i64,
        max_score: i64,
    ) -> ApiResult<i64> {
        let mut conn = self.redis.clone();
        redis::cmd("ZREMRANGEBYSCORE")
            .arg(key)
            .arg(min_score)
            .arg(max_score)
            .query_async(&mut conn)
            .await
            .with_context(|| format!("failed to trim sorted-set members for key {key}"))
            .map_err(ApiErrors::Internal)
    }

    // ── Redis Degradation Helpers ──────────────────────────────────────

    /// Read-through with graceful degradation: returns `None` on Redis failure
    /// instead of propagating the error, allowing callers to fall back to DB.
    pub async fn get_json_graceful<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        match self.get_json::<T>(key).await {
            Ok(value) => value,
            Err(e) => {
                tracing::warn!(error = %e, key, "redis degraded: cache read failed, falling back");
                None
            }
        }
    }

    /// Write-through with graceful degradation: logs a warning on Redis failure
    /// instead of propagating the error. Data will be re-cached on next read.
    pub async fn set_json_graceful<T: Serialize>(&self, key: &str, value: &T, ttl_seconds: u64) {
        if let Err(e) = self.set_json(key, value, ttl_seconds).await {
            tracing::warn!(error = %e, key, "redis degraded: cache write failed, skipping");
        }
    }

    /// Atomic increment with graceful degradation for rate limiting.
    ///
    /// On Redis failure, returns `Ok(0)` (effectively allowing the request)
    /// and logs a warning. This is a deliberate fail-open for rate limiting
    /// to avoid blocking all traffic when Redis is temporarily unavailable.
    pub async fn incr_with_expire_graceful(&self, key: &str, ttl_seconds: i64) -> i64 {
        match self.incr_with_expire(key, ttl_seconds).await {
            Ok(value) => value,
            Err(e) => {
                tracing::warn!(
                    error = %e, key,
                    "redis degraded: rate limit incr failed, allowing request"
                );
                0
            }
        }
    }

    /// Atomic increment-by with graceful degradation for rate limiting.
    pub async fn incr_by_with_expire_graceful(
        &self,
        key: &str,
        value: i64,
        ttl_seconds: i64,
    ) -> i64 {
        match self.incr_by_with_expire(key, value, ttl_seconds).await {
            Ok(result) => result,
            Err(e) => {
                tracing::warn!(
                    error = %e, key,
                    "redis degraded: rate limit incr_by failed, allowing request"
                );
                0
            }
        }
    }
}
