use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use parking_lot::RwLock;

use crate::storage::AuthStorage;

/// 每隔多少次写操作执行一次过期清理
const CLEANUP_INTERVAL: u64 = 100;

struct Entry {
    value: String,
    expires_at: Option<i64>,
}

impl Entry {
    fn is_expired(&self) -> bool {
        match self.expires_at {
            Some(ts) => chrono::Local::now().timestamp() > ts,
            None => false,
        }
    }
}

/// 内存存储实现（开发/测试用）
#[derive(Clone)]
pub struct MemoryStorage {
    data: Arc<RwLock<HashMap<String, Entry>>>,
    write_count: Arc<AtomicU64>,
}

impl MemoryStorage {
    pub fn new() -> Self {
        Self {
            data: Arc::new(RwLock::new(HashMap::new())),
            write_count: Arc::new(AtomicU64::new(0)),
        }
    }

    /// 手动清理所有过期条目
    pub fn purge_expired(&self) {
        let now = chrono::Local::now().timestamp();
        self.data
            .write()
            .retain(|_, entry| entry.expires_at.is_none_or(|ts| now <= ts));
    }

    /// 每 N 次写操作后自动清理
    fn maybe_cleanup(&self) {
        let count = self.write_count.fetch_add(1, Ordering::Relaxed);
        if count % CLEANUP_INTERVAL == 0 {
            self.purge_expired();
        }
    }
}

impl Default for MemoryStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl AuthStorage for MemoryStorage {
    async fn set_string(&self, key: &str, value: &str, ttl_seconds: i64) -> anyhow::Result<()> {
        let expires_at = if ttl_seconds > 0 {
            Some(chrono::Local::now().timestamp() + ttl_seconds)
        } else {
            None
        };
        self.data.write().insert(
            key.to_string(),
            Entry {
                value: value.to_string(),
                expires_at,
            },
        );
        self.maybe_cleanup();
        Ok(())
    }

    async fn get_string(&self, key: &str) -> anyhow::Result<Option<String>> {
        let data = self.data.read();
        match data.get(key) {
            Some(entry) if !entry.is_expired() => Ok(Some(entry.value.clone())),
            Some(_) => {
                drop(data);
                self.data.write().remove(key);
                Ok(None)
            }
            None => Ok(None),
        }
    }

    async fn delete(&self, key: &str) -> anyhow::Result<()> {
        self.data.write().remove(key);
        Ok(())
    }

    async fn keys_by_prefix(&self, prefix: &str) -> anyhow::Result<Vec<String>> {
        let now = chrono::Local::now().timestamp();
        let data = self.data.read();
        let keys: Vec<String> = data
            .iter()
            .filter(|(k, entry)| {
                k.starts_with(prefix) && entry.expires_at.is_none_or(|ts| now <= ts)
            })
            .map(|(k, _)| k.clone())
            .collect();
        Ok(keys)
    }

    /// 使用读锁检查键是否存在（避免 clone value）
    async fn exists(&self, key: &str) -> anyhow::Result<bool> {
        let data = self.data.read();
        match data.get(key) {
            Some(entry) if !entry.is_expired() => Ok(true),
            _ => Ok(false),
        }
    }
}
