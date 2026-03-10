use summer_redis::redis;
use summer_redis::redis::AsyncCommands;
use summer_redis::Redis;

use crate::storage::AuthStorage;

/// Redis 存储实现
#[derive(Clone)]
pub struct RedisStorage {
    conn: Redis,
}

impl RedisStorage {
    pub fn new(conn: Redis) -> Self {
        Self { conn }
    }
}

#[async_trait::async_trait]
impl AuthStorage for RedisStorage {
    async fn set_string(&self, key: &str, value: &str, ttl_seconds: i64) -> anyhow::Result<()> {
        let mut conn = self.conn.clone();
        if ttl_seconds > 0 {
            conn.set_ex::<_, _, ()>(key, value, ttl_seconds as u64)
                .await?;
        } else {
            conn.set::<_, _, ()>(key, value).await?;
        }
        Ok(())
    }

    async fn get_string(&self, key: &str) -> anyhow::Result<Option<String>> {
        let mut conn = self.conn.clone();
        let val: Option<String> = conn.get(key).await?;
        Ok(val)
    }

    async fn delete(&self, key: &str) -> anyhow::Result<()> {
        let mut conn = self.conn.clone();
        conn.del::<_, ()>(key).await?;
        Ok(())
    }

    /// 按前缀搜索所有键（使用 SCAN 代替 KEYS，生产安全）
    async fn keys_by_prefix(&self, prefix: &str) -> anyhow::Result<Vec<String>> {
        let mut conn = self.conn.clone();
        let pattern = format!("{prefix}*");
        let mut keys = Vec::new();
        let mut cursor: u64 = 0;

        loop {
            let (next_cursor, batch): (u64, Vec<String>) = redis::cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg(&pattern)
                .arg("COUNT")
                .arg(100)
                .query_async(&mut conn)
                .await?;

            keys.extend(batch);
            cursor = next_cursor;

            if cursor == 0 {
                break;
            }
        }

        Ok(keys)
    }

    /// 使用 Redis 原生 EXISTS 命令检查键是否存在
    async fn exists(&self, key: &str) -> anyhow::Result<bool> {
        let mut conn = self.conn.clone();
        let exists: bool = conn.exists(key).await?;
        Ok(exists)
    }
}
