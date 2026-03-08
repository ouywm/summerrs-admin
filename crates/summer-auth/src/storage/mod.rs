use crate::session::model::UserSession;

#[cfg(feature = "memory")]
pub mod memory;
#[cfg(feature = "redis")]
pub mod redis;

/// 认证存储抽象 trait
#[async_trait::async_trait]
pub trait AuthStorage: Send + Sync + 'static {
    /// 存储 UserSession（JSON 序列化）
    async fn set_session(
        &self,
        key: &str,
        session: &UserSession,
        ttl_seconds: i64,
    ) -> anyhow::Result<()>;

    /// 获取 UserSession
    async fn get_session(&self, key: &str) -> anyhow::Result<Option<UserSession>>;

    /// 存储字符串
    async fn set_string(&self, key: &str, value: &str, ttl_seconds: i64) -> anyhow::Result<()>;

    /// 获取字符串
    async fn get_string(&self, key: &str) -> anyhow::Result<Option<String>>;

    /// 删除键
    async fn delete(&self, key: &str) -> anyhow::Result<()>;

    /// 按前缀搜索所有键
    async fn keys_by_prefix(&self, prefix: &str) -> anyhow::Result<Vec<String>>;

    /// 检查键是否存在（用于黑名单快速检查）
    async fn exists(&self, key: &str) -> anyhow::Result<bool> {
        Ok(self.get_string(key).await?.is_some())
    }
}
