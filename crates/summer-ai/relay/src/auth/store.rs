//! AI API Token 查询与缓存。
//!
//! 客户端以 `Authorization: Bearer <raw_token>` 调用；本模块做两件事：
//!
//! 1. **取** — Redis `ai:tk:{sha256(raw_token)}` 命中直接返；miss 走 DB 查
//!    `ai.token.key_hash = sha256(raw_token)`，找到后回写 Redis。
//! 2. **校** — 只有 `status=Enabled` 且未过期且（无限额或剩余配额>0）才算有效。
//!
//! IP 白黑名单、rpm/tpm 限流、模型白名单等**不在本层**——后续独立中间件处理。

use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use sha2::{Digest, Sha256};
use summer_ai_model::entity::billing::token;
use summer_redis::Redis;
use summer_redis::redis::AsyncCommands;
use summer_sea_orm::DbConn;

use crate::error::RelayError;

/// Redis cache TTL（秒）——与 `ChannelStore` 保持一致；token 变更不频繁，300s 可接受。
const TOKEN_CACHE_TTL_SECS: u64 = 300;

/// Token 查询 + 缓存。`#[derive(Service)]` 自动从 Component registry 注入 DB/Redis。
#[derive(Clone)]
pub struct AiTokenStore {
    db: DbConn,
    redis: Redis,
}

impl AiTokenStore {
    /// 手动构造（给 plugin 初始化和测试用）。
    pub fn new(db: DbConn, redis: Redis) -> Self {
        Self { db, redis }
    }

    /// 给定客户端传来的原始 Bearer token，返回有效的 token Model。
    ///
    /// - `Ok(Some(m))` — 命中且通过校验
    /// - `Ok(None)` — 不存在 / status 非启用 / 额度耗尽
    /// - `Err(RelayError::TokenExpired)` — 仅过期场景（给调用方区分错误信息）
    pub async fn lookup(&self, raw_token: &str) -> Result<Option<token::Model>, RelayError> {
        let key_hash = sha256_hex(raw_token.as_bytes());
        let redis_key = Self::cache_key(&key_hash);

        // L1: Redis
        if let Some(cached) = self.redis_get_json::<token::Model>(&redis_key).await? {
            return classify(cached);
        }

        // L2: DB
        let Some(model) = token::Entity::find()
            .filter(token::Column::KeyHash.eq(&key_hash))
            .one(&self.db)
            .await
            .map_err(RelayError::Database)?
        else {
            return Ok(None);
        };

        // 回写 Redis（即便 status 不合法也缓存——下次一样快拒绝）
        self.redis_set_json(&redis_key, &model).await?;
        classify(model)
    }

    /// 主动失效指定 token 的 Redis cache（给 admin 禁用 / 删 token 的钩子用）。
    pub async fn invalidate(&self, raw_token: &str) -> Result<(), RelayError> {
        let key_hash = sha256_hex(raw_token.as_bytes());
        let key = Self::cache_key(&key_hash);
        let mut conn = self.redis.clone();
        conn.del::<_, ()>(key)
            .await
            .map_err(|e| RelayError::Redis(e.to_string()))?;
        Ok(())
    }

    /// 按 hash 失效（admin 只持有 key_hash 不持有原文时用）。
    pub async fn invalidate_by_hash(&self, key_hash: &str) -> Result<(), RelayError> {
        let key = Self::cache_key(key_hash);
        let mut conn = self.redis.clone();
        conn.del::<_, ()>(key)
            .await
            .map_err(|e| RelayError::Redis(e.to_string()))?;
        Ok(())
    }

    fn cache_key(key_hash: &str) -> String {
        format!("ai:tk:{key_hash}")
    }

    async fn redis_get_json<T: serde::de::DeserializeOwned>(
        &self,
        key: &str,
    ) -> Result<Option<T>, RelayError> {
        let mut conn = self.redis.clone();
        let raw: Option<String> = conn
            .get(key)
            .await
            .map_err(|e| RelayError::Redis(e.to_string()))?;
        match raw {
            Some(s) => match serde_json::from_str::<T>(&s) {
                Ok(v) => Ok(Some(v)),
                Err(e) => {
                    tracing::warn!(%key, ?e, "token cache parse failed, treat as miss");
                    Ok(None)
                }
            },
            None => Ok(None),
        }
    }

    async fn redis_set_json<T: serde::Serialize>(
        &self,
        key: &str,
        value: &T,
    ) -> Result<(), RelayError> {
        let s = serde_json::to_string(value)
            .map_err(|e| RelayError::Redis(format!("serialize {key}: {e}")))?;
        let mut conn = self.redis.clone();
        conn.set_ex::<_, _, ()>(key, s, TOKEN_CACHE_TTL_SECS)
            .await
            .map_err(|e| RelayError::Redis(e.to_string()))?;
        Ok(())
    }
}

/// 按业务规则给 token Model 分类——从 lookup / 测试共用。
fn classify(m: token::Model) -> Result<Option<token::Model>, RelayError> {
    let now = chrono::Utc::now().fixed_offset();

    if let Some(expire) = m.expire_time
        && expire < now
    {
        return Err(RelayError::TokenExpired);
    }

    match m.status {
        token::TokenStatus::Enabled => {
            if !m.unlimited_quota && m.remain_quota <= 0 {
                return Err(RelayError::QuotaExhausted);
            }
            Ok(Some(m))
        }
        token::TokenStatus::Expired => Err(RelayError::TokenExpired),
        token::TokenStatus::QuotaExhausted => Err(RelayError::QuotaExhausted),
        token::TokenStatus::Disabled => Ok(None),
    }
}

/// SHA-256 hex（小写）。`ai.token.key_hash` 列的规范格式。
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use sea_orm::prelude::BigDecimal;

    fn mk_model(
        status: token::TokenStatus,
        unlimited: bool,
        remain: i64,
        expire: Option<chrono::DateTime<chrono::FixedOffset>>,
    ) -> token::Model {
        token::Model {
            id: 1,
            user_id: 1,
            service_account_id: 0,
            project_id: 0,
            name: "t".into(),
            key_hash: "h".into(),
            key_prefix: "sk-t".into(),
            status,
            remain_quota: remain,
            used_quota: 0,
            unlimited_quota: unlimited,
            models: serde_json::json!([]),
            endpoint_scopes: serde_json::json!([]),
            ip_whitelist: serde_json::json!([]),
            ip_blacklist: serde_json::json!([]),
            group_code_override: String::new(),
            rpm_limit: 0,
            tpm_limit: 0,
            concurrency_limit: 0,
            daily_quota_limit: 0,
            monthly_quota_limit: 0,
            daily_used_quota: 0,
            monthly_used_quota: 0,
            daily_window_start: None,
            monthly_window_start: None,
            expire_time: expire,
            access_time: None,
            last_used_ip: String::new(),
            last_user_agent: String::new(),
            remark: String::new(),
            create_by: String::new(),
            create_time: Utc::now().fixed_offset(),
            update_by: String::new(),
            update_time: Utc::now().fixed_offset(),
        }
    }

    #[allow(dead_code)]
    fn _compiles(_b: BigDecimal) {}

    #[test]
    fn sha256_hex_matches_known_value() {
        // echo -n "abc" | shasum -a 256
        // ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn classify_enabled_unlimited_returns_some() {
        let m = mk_model(token::TokenStatus::Enabled, true, 0, None);
        assert!(classify(m).unwrap().is_some());
    }

    #[test]
    fn classify_enabled_with_remain_returns_some() {
        let m = mk_model(token::TokenStatus::Enabled, false, 10, None);
        assert!(classify(m).unwrap().is_some());
    }

    #[test]
    fn classify_enabled_no_quota_returns_quota_exhausted() {
        let m = mk_model(token::TokenStatus::Enabled, false, 0, None);
        assert!(matches!(classify(m), Err(RelayError::QuotaExhausted)));
    }

    #[test]
    fn classify_disabled_returns_none() {
        let m = mk_model(token::TokenStatus::Disabled, true, 1000, None);
        assert!(classify(m).unwrap().is_none());
    }

    #[test]
    fn classify_status_expired_returns_token_expired() {
        let m = mk_model(token::TokenStatus::Expired, true, 1000, None);
        assert!(matches!(classify(m), Err(RelayError::TokenExpired)));
    }

    #[test]
    fn classify_status_quota_exhausted_returns_quota_exhausted() {
        let m = mk_model(token::TokenStatus::QuotaExhausted, true, 1000, None);
        assert!(matches!(classify(m), Err(RelayError::QuotaExhausted)));
    }

    #[test]
    fn classify_expire_time_in_past_returns_token_expired() {
        let past = (Utc::now() - Duration::hours(1)).fixed_offset();
        let m = mk_model(token::TokenStatus::Enabled, true, 100, Some(past));
        assert!(matches!(classify(m), Err(RelayError::TokenExpired)));
    }

    #[test]
    fn classify_expire_time_in_future_returns_some() {
        let future = (Utc::now() + Duration::hours(1)).fixed_offset();
        let m = mk_model(token::TokenStatus::Enabled, true, 100, Some(future));
        assert!(classify(m).unwrap().is_some());
    }
}
