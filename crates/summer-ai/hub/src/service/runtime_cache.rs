use anyhow::Context;
use serde::Serialize;
use serde::de::DeserializeOwned;
use summer::plugin::Service;
use summer_common::error::{ApiErrors, ApiResult};
use summer_redis::Redis;
use summer_redis::redis;
use summer_redis::redis::AsyncCommands;

#[derive(Clone, Service)]
pub struct RuntimeCacheService {
    #[inject(component)]
    redis: Redis,
}

impl RuntimeCacheService {
    pub fn new(redis: Redis) -> Self {
        Self { redis }
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
}
