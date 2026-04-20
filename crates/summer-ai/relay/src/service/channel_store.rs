//! ChannelStore —— 渠道/账号路由存储（Redis + DB 双层）。
//!
//! **DB 是权威源**，Redis 只是读缓存。每次 `pick` 沿以下四个 key 命名空间查询：
//!
//! | key | 值 | miss 时 |
//! |---|---|---|
//! | `ai:ch:m:{logical_model}` | JSON `[channel_id,…]` | 扫描 `ai.channel`，过滤启用+未归档+`models` 包含 `logical_model` |
//! | `ai:ch:c:{channel_id}` | JSON `channel::Model` | 查 `ai.channel.id = {channel_id}` |
//! | `ai:ch:ac:{channel_id}` | JSON `[account_id,…]` | 查 `ai.channel_account` where channel_id+schedulable+enabled |
//! | `ai:ch:a:{account_id}` | JSON `channel_account::Model` | 查 `ai.channel_account.id = {account_id}` |
//!
//! 所有 key 统一 300s TTL（兜底：即使 invalidate 事件丢失，5 分钟内也会自动刷新）。
//!
//! # 选路规则
//!
//! 1. **按模型过滤**：`channel.models` JSONB 数组包含 `logical_model`
//! 2. **状态过滤**：channel 启用+未归档；account 启用+schedulable+`rate_limited_until` 过期
//! 3. **优先级降序**：仅在当前最高 `priority` 组里挑
//! 4. **权重加权随机**：同优先级内按 `weight` 加权随机（weight ≤ 0 折成 1）
//!
//! # 失效
//!
//! 当前依赖 TTL 兜底；后续接入 `summer-stream` 后由 admin CRUD 发事件主动失效。

use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use summer::plugin::Service;
use summer_ai_core::{AdapterKind, AuthData, Endpoint, ServiceTarget};
use summer_ai_model::entity::channels::{channel, channel_account};
use summer_redis::Redis;
use summer_redis::redis;
use summer_redis::redis::AsyncCommands;
use summer_sea_orm::DbConn;

use crate::error::RelayError;
use crate::service::key_picker::{KeyPicker, RandomKeyPicker};

/// Redis key TTL（秒）。故意设短，TTL 兜底失效路径。
const CACHE_TTL_SECS: u64 = 300;

// ---------------------------------------------------------------------------
// Channel type (DB) → AdapterKind 映射
// ---------------------------------------------------------------------------

pub fn channel_type_to_adapter_kind(kind: channel::ChannelType) -> AdapterKind {
    use channel::ChannelType as DbKind;
    match kind {
        DbKind::OpenAi => AdapterKind::OpenAI,
        DbKind::Anthropic => AdapterKind::Claude,
        DbKind::Azure => AdapterKind::Azure,
        DbKind::Gemini => AdapterKind::Gemini,
        DbKind::Ollama => AdapterKind::Ollama,
        DbKind::Ali => AdapterKind::Aliyun,
        // Baidu 暂没有 dedicated adapter，兜底为 OpenAICompat
        DbKind::Baidu => AdapterKind::OpenAICompat,
    }
}

// ---------------------------------------------------------------------------
// ChannelStore
// ---------------------------------------------------------------------------

/// 渠道 / 账号路由存储（Redis cache + DB fallback）
#[derive(Clone, Service)]
pub struct ChannelStore {
    #[inject(component)]
    db: DbConn,
    #[inject(component)]
    redis: Redis,
}

impl ChannelStore {
    /// 按逻辑模型名挑 `(channel, account, selected_key)` 组合。
    ///
    /// - 第 1-5 步同旧版：按 model/channel/account 筛选 + 权重随机
    /// - **第 6 步（新）**：从被选 account 的 `enabled_api_keys()` 里用 [`RandomKeyPicker`] 挑一个 key
    /// - **account 全禁跳过**：如果某 account 所有 key 都在 `disabled_api_keys` 列表里，跳到下一个候选
    ///
    /// 全部 channel / account 都选不到 key → 返 `Ok(None)`（handler 侧返 `NoAvailableChannel`）。
    pub async fn pick(
        &self,
        logical_model: &str,
    ) -> Result<Option<(channel::Model, channel_account::Model, String)>, RelayError> {
        // 第 1 步：模型 → channel_ids
        let channel_ids = self.load_model_channels(logical_model).await?;
        if channel_ids.is_empty() {
            return Ok(None);
        }

        // 第 2 步：channel_ids → channels
        let channels = self.load_channels(&channel_ids).await?;
        if channels.is_empty() {
            return Ok(None);
        }

        // 第 3 步：权重选 channel
        let refs: Vec<&channel::Model> = channels.iter().collect();
        let Some(picked_channel) = weighted_pick(&refs, |c| (c.priority, c.weight)).cloned() else {
            return Ok(None);
        };

        // 第 4 步：channel_id → account_ids
        let account_ids = self.load_channel_accounts(picked_channel.id).await?;
        if account_ids.is_empty() {
            return Ok(None);
        }

        // 第 5 步：account_ids → accounts + cooldown 过滤
        let accounts = self.load_accounts(&account_ids).await?;
        let now = chrono::Utc::now().fixed_offset();
        let mut candidates: Vec<&channel_account::Model> = accounts
            .iter()
            .filter(|a| a.rate_limited_until.map(|t| t < now).unwrap_or(true))
            .collect();

        // 第 6 步：循环选 account + key —— account 全禁则剔除，选下一个
        let picker = RandomKeyPicker;
        while !candidates.is_empty() {
            let Some(picked) = weighted_pick(&candidates, |a| (a.priority, a.weight)) else {
                return Ok(None);
            };
            // 拿到一个引用后，尝试选一个可用 key
            let enabled_keys = picked.enabled_api_keys();
            if let Some(selected) = picker.pick(&enabled_keys) {
                let selected_key = selected.to_string();
                let picked_account = (*picked).clone();
                return Ok(Some((picked_channel, picked_account, selected_key)));
            }
            // 该 account 所有 key 都被禁（或根本没 key）→ 从候选里剔除，下一轮
            tracing::debug!(
                account_id = picked.id,
                channel_id = picked.channel_id,
                "account has no enabled api key, skipping"
            );
            let banned_id = picked.id;
            candidates.retain(|a| a.id != banned_id);
        }
        Ok(None)
    }

    // ---------------- Redis key helpers ----------------

    fn k_model(model: &str) -> String {
        format!("ai:ch:m:{model}")
    }
    fn k_channel(id: i64) -> String {
        format!("ai:ch:c:{id}")
    }
    fn k_channel_accounts(channel_id: i64) -> String {
        format!("ai:ch:ac:{channel_id}")
    }
    fn k_account(id: i64) -> String {
        format!("ai:ch:a:{id}")
    }

    // ---------------- load_model_channels ----------------

    async fn load_model_channels(&self, logical_model: &str) -> Result<Vec<i64>, RelayError> {
        let key = Self::k_model(logical_model);
        if let Some(cached) = self.redis_get_json::<Vec<i64>>(&key).await? {
            return Ok(cached);
        }

        // DB fallback：扫所有启用+未归档 channels，过滤 models 包含
        let channels: Vec<channel::Model> = channel::Entity::find()
            .filter(channel::Column::Status.eq(channel::ChannelStatus::Enabled))
            .filter(channel::Column::DeletedAt.is_null())
            .all(&self.db)
            .await
            .map_err(RelayError::Database)?;

        let ids: Vec<i64> = channels
            .iter()
            .filter(|c| {
                c.models
                    .as_array()
                    .map(|arr| arr.iter().any(|v| v.as_str() == Some(logical_model)))
                    .unwrap_or(false)
            })
            .map(|c| c.id)
            .collect();

        self.redis_set_json(&key, &ids).await?;

        // 顺便预热 channel 元数据
        for c in channels {
            if ids.contains(&c.id) {
                let ck = Self::k_channel(c.id);
                self.redis_set_json(&ck, &c).await?;
            }
        }

        Ok(ids)
    }

    // ---------------- load_channels (MGET + per-miss fallback) ----------------

    async fn load_channels(&self, ids: &[i64]) -> Result<Vec<channel::Model>, RelayError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let keys: Vec<String> = ids.iter().map(|id| Self::k_channel(*id)).collect();
        let raws: Vec<Option<String>> = self.redis_mget(&keys).await?;

        let mut results: Vec<channel::Model> = Vec::with_capacity(ids.len());
        let mut misses: Vec<i64> = Vec::new();
        for (i, raw) in raws.into_iter().enumerate() {
            match raw {
                Some(s) => match serde_json::from_str::<channel::Model>(&s) {
                    Ok(m) => results.push(m),
                    Err(e) => {
                        // 解析失败视为 miss（schema 变化 / 手工改 Redis）
                        tracing::warn!(id = ids[i], ?e, "channel cache parse failed, fallback db");
                        misses.push(ids[i]);
                    }
                },
                None => misses.push(ids[i]),
            }
        }

        if !misses.is_empty() {
            let fetched: Vec<channel::Model> = channel::Entity::find()
                .filter(channel::Column::Id.is_in(misses.clone()))
                .filter(channel::Column::Status.eq(channel::ChannelStatus::Enabled))
                .filter(channel::Column::DeletedAt.is_null())
                .all(&self.db)
                .await
                .map_err(RelayError::Database)?;
            for c in &fetched {
                let k = Self::k_channel(c.id);
                self.redis_set_json(&k, c).await?;
            }
            results.extend(fetched);
        }

        Ok(results)
    }

    // ---------------- load_channel_accounts ----------------

    async fn load_channel_accounts(&self, channel_id: i64) -> Result<Vec<i64>, RelayError> {
        let key = Self::k_channel_accounts(channel_id);
        if let Some(cached) = self.redis_get_json::<Vec<i64>>(&key).await? {
            return Ok(cached);
        }

        let accounts: Vec<channel_account::Model> =
            channel_account::Entity::find_schedulable_by_channel_ids(&self.db, &[channel_id])
                .await
                .map_err(RelayError::Database)?;

        let ids: Vec<i64> = accounts.iter().map(|a| a.id).collect();
        self.redis_set_json(&key, &ids).await?;

        // 预热 account meta
        for a in &accounts {
            let k = Self::k_account(a.id);
            self.redis_set_json(&k, a).await?;
        }

        Ok(ids)
    }

    // ---------------- load_accounts ----------------

    async fn load_accounts(&self, ids: &[i64]) -> Result<Vec<channel_account::Model>, RelayError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let keys: Vec<String> = ids.iter().map(|id| Self::k_account(*id)).collect();
        let raws: Vec<Option<String>> = self.redis_mget(&keys).await?;

        let mut results: Vec<channel_account::Model> = Vec::with_capacity(ids.len());
        let mut misses: Vec<i64> = Vec::new();
        for (i, raw) in raws.into_iter().enumerate() {
            match raw {
                Some(s) => match serde_json::from_str::<channel_account::Model>(&s) {
                    Ok(m) => results.push(m),
                    Err(e) => {
                        tracing::warn!(id = ids[i], ?e, "account cache parse failed, fallback db");
                        misses.push(ids[i]);
                    }
                },
                None => misses.push(ids[i]),
            }
        }

        if !misses.is_empty() {
            let fetched: Vec<channel_account::Model> = channel_account::Entity::find()
                .filter(channel_account::Column::Id.is_in(misses.clone()))
                .filter(channel_account::Column::DeletedAt.is_null())
                .filter(channel_account::Column::Schedulable.eq(true))
                .filter(
                    channel_account::Column::Status
                        .eq(channel_account::ChannelAccountStatus::Enabled),
                )
                .all(&self.db)
                .await
                .map_err(RelayError::Database)?;
            for a in &fetched {
                let k = Self::k_account(a.id);
                self.redis_set_json(&k, a).await?;
            }
            results.extend(fetched);
        }

        Ok(results)
    }

    // ---------------- Invalidation API（后续给 stream listener 用）----------------

    /// 失效指定 channel 的 meta + 该 channel 的 account 列表索引。
    ///
    /// 不会递归失效单个 account key（account 变更走 `invalidate_account`）。
    pub async fn invalidate_channel(&self, channel_id: i64) -> Result<(), RelayError> {
        let mut conn = self.redis.clone();
        let keys = vec![
            Self::k_channel(channel_id),
            Self::k_channel_accounts(channel_id),
        ];
        conn.del::<_, ()>(keys)
            .await
            .map_err(|e| RelayError::Redis(e.to_string()))?;
        Ok(())
    }

    /// 失效某个模型的倒排索引。
    pub async fn invalidate_model(&self, logical_model: &str) -> Result<(), RelayError> {
        let mut conn = self.redis.clone();
        conn.del::<_, ()>(Self::k_model(logical_model))
            .await
            .map_err(|e| RelayError::Redis(e.to_string()))?;
        Ok(())
    }

    /// 失效单个 account meta。
    pub async fn invalidate_account(&self, account_id: i64) -> Result<(), RelayError> {
        let mut conn = self.redis.clone();
        conn.del::<_, ()>(Self::k_account(account_id))
            .await
            .map_err(|e| RelayError::Redis(e.to_string()))?;
        Ok(())
    }

    /// 扫描并清空所有 `ai:ch:*` key。管理面"一键刷新"用。
    pub async fn invalidate_all(&self) -> Result<usize, RelayError> {
        let mut conn = self.redis.clone();
        let mut cursor: u64 = 0;
        let mut total: usize = 0;
        loop {
            let (next_cursor, batch): (u64, Vec<String>) = redis::cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg("ai:ch:*")
                .arg("COUNT")
                .arg(500)
                .query_async(&mut conn)
                .await
                .map_err(|e| RelayError::Redis(e.to_string()))?;
            if !batch.is_empty() {
                total += batch.len();
                let _: () = conn
                    .del(&batch)
                    .await
                    .map_err(|e| RelayError::Redis(e.to_string()))?;
            }
            cursor = next_cursor;
            if cursor == 0 {
                break;
            }
        }
        Ok(total)
    }

    // ---------------- Redis primitives ----------------

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
                    tracing::warn!(%key, ?e, "redis cache parse failed, treating as miss");
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
        conn.set_ex::<_, _, ()>(key, s, CACHE_TTL_SECS)
            .await
            .map_err(|e| RelayError::Redis(e.to_string()))?;
        Ok(())
    }

    async fn redis_mget(&self, keys: &[String]) -> Result<Vec<Option<String>>, RelayError> {
        if keys.is_empty() {
            return Ok(Vec::new());
        }
        let mut conn = self.redis.clone();
        // 单 key 时 redis crate 返回 Option<String> 而非 Vec，统一走 cmd("MGET") 规避
        let raws: Vec<Option<String>> = redis::cmd("MGET")
            .arg(keys)
            .query_async(&mut conn)
            .await
            .map_err(|e| RelayError::Redis(e.to_string()))?;
        Ok(raws)
    }
}

// ---------------------------------------------------------------------------
// ServiceTarget builder
// ---------------------------------------------------------------------------

/// 从 `(channel, account, selected_key, logical_model)` 构造 `ServiceTarget` + `AdapterKind`。
///
/// - **selected_key** 由上层 `ChannelStore::pick` + `KeyPicker` 决定，本函数只认 pre-selected key
/// - **OAuth account**（`account.is_oauth()`）本期未落地，返 `NotImplemented`
/// - `actual_model` 应用 `channel.model_mapping` JSONB（缺省保留 logical_model）
/// - `endpoint` 取 `channel.base_url`
/// - `extra_headers` 从 `channel.config.extra_headers` 读（如果有）
pub fn build_service_target(
    channel: &channel::Model,
    account: &channel_account::Model,
    selected_key: &str,
    logical_model: &str,
) -> Result<(AdapterKind, ServiceTarget), RelayError> {
    if account.is_oauth() {
        return Err(RelayError::NotImplemented("oauth credentials"));
    }
    if selected_key.is_empty() {
        return Err(RelayError::MissingConfig(
            "channel_account selected_key is empty (multi-key pool exhausted?)",
        ));
    }

    let actual_model = channel.resolve_upstream_model(logical_model);
    let endpoint = Endpoint::from_owned(channel.base_url.clone());
    let auth = AuthData::from_single(selected_key.to_string());

    let mut target = ServiceTarget {
        endpoint,
        auth,
        actual_model,
        extra_headers: Default::default(),
    };

    if let Some(obj) = channel
        .config
        .get("extra_headers")
        .and_then(|v| v.as_object())
    {
        for (k, v) in obj {
            if let Some(s) = v.as_str() {
                target = target.with_header(k.clone(), s.to_string());
            }
        }
    }

    Ok((channel_type_to_adapter_kind(channel.channel_type), target))
}

// ---------------------------------------------------------------------------
// weighted_pick —— 纯函数（从前代内存版本继承下来，测试覆盖）
// ---------------------------------------------------------------------------

/// 在候选中取最高 `priority` 一组，然后在该组内按 `weight` 加权随机。
///
/// `key` 返回 `(priority, weight)`。`weight ≤ 0` 折成 1。
fn weighted_pick<'a, T, K>(items: &'a [&'a T], key: K) -> Option<&'a T>
where
    K: Fn(&T) -> (i32, i32),
{
    if items.is_empty() {
        return None;
    }
    let top_priority = items.iter().map(|t| key(t).0).max()?;

    let top: Vec<(&T, i32)> = items
        .iter()
        .filter_map(|t| {
            let (pri, w) = key(t);
            if pri == top_priority {
                Some((*t, w.max(1)))
            } else {
                None
            }
        })
        .collect();

    if top.is_empty() {
        return None;
    }
    let total: u64 = top.iter().map(|(_, w)| *w as u64).sum();
    if total == 0 {
        return top.first().map(|(t, _)| *t);
    }

    let mut roll = rand::random_range(0..total);
    for (item, w) in &top {
        let w = *w as u64;
        if roll < w {
            return Some(*item);
        }
        roll -= w;
    }
    top.last().map(|(t, _)| *t)
}

// ---------------------------------------------------------------------------
// Tests —— 只测纯函数部分；Redis/DB 读路径放集成测试。
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm::prelude::BigDecimal;

    fn mk_channel(
        id: i64,
        models: Vec<&str>,
        priority: i32,
        weight: i32,
        channel_type: channel::ChannelType,
        base_url: &str,
    ) -> channel::Model {
        channel::Model {
            id,
            name: format!("ch-{id}"),
            channel_type,
            vendor_code: "vendor".to_string(),
            base_url: base_url.to_string(),
            status: channel::ChannelStatus::Enabled,
            models: serde_json::json!(models),
            model_mapping: serde_json::json!({}),
            channel_group: String::new(),
            endpoint_scopes: serde_json::json!([]),
            capabilities: serde_json::json!([]),
            weight,
            priority,
            config: serde_json::json!({}),
            auto_ban: false,
            test_model: String::new(),
            used_quota: 0,
            balance: BigDecimal::from(0),
            balance_updated_at: None,
            response_time: 0,
            success_rate: BigDecimal::from(0),
            failure_streak: 0,
            last_used_at: None,
            last_error_at: None,
            last_error_code: String::new(),
            last_error_message: String::new(),
            last_health_status: channel::ChannelLastHealthStatus::Unknown,
            deleted_at: None,
            remark: String::new(),
            create_by: String::new(),
            create_time: chrono::Utc::now().fixed_offset(),
            update_by: String::new(),
            update_time: chrono::Utc::now().fixed_offset(),
        }
    }

    fn mk_account(
        id: i64,
        channel_id: i64,
        api_key: &str,
        priority: i32,
        weight: i32,
    ) -> channel_account::Model {
        channel_account::Model {
            id,
            channel_id,
            name: format!("acc-{id}"),
            credential_type: "api_key".to_string(),
            credentials: serde_json::json!({"api_key": api_key}),
            secret_ref: String::new(),
            status: channel_account::ChannelAccountStatus::Enabled,
            schedulable: true,
            priority,
            weight,
            rate_multiplier: BigDecimal::from(1),
            concurrency_limit: 0,
            quota_limit: BigDecimal::from(0),
            quota_used: BigDecimal::from(0),
            balance: BigDecimal::from(0),
            balance_updated_at: None,
            response_time: 0,
            failure_streak: 0,
            last_used_at: None,
            last_error_at: None,
            last_error_code: String::new(),
            last_error_message: String::new(),
            rate_limited_until: None,
            overload_until: None,
            expires_at: None,
            test_model: String::new(),
            test_time: None,
            extra: serde_json::json!({}),
            deleted_at: None,
            remark: String::new(),
            create_by: String::new(),
            create_time: chrono::Utc::now().fixed_offset(),
            update_by: String::new(),
            update_time: chrono::Utc::now().fixed_offset(),
            disabled_api_keys: serde_json::json!([]),
        }
    }

    // ---------- build_service_target ----------

    #[test]
    fn build_service_target_maps_channel_type_and_applies_model_mapping() {
        let mut channel = mk_channel(
            1,
            vec!["gpt-4"],
            1,
            100,
            channel::ChannelType::OpenAi,
            "https://api.openai.com/v1",
        );
        channel.model_mapping = serde_json::json!({"gpt-4": "gpt-4-turbo"});
        let account = mk_account(10, 1, "sk-a", 1, 100);

        let (kind, target) = build_service_target(&channel, &account, "sk-a", "gpt-4").unwrap();
        assert_eq!(kind, AdapterKind::OpenAI);
        assert_eq!(target.actual_model, "gpt-4-turbo");
        assert_eq!(target.endpoint.trimmed(), "https://api.openai.com/v1");
    }

    #[test]
    fn build_service_target_errors_on_empty_key() {
        let channel = mk_channel(
            1,
            vec!["x"],
            1,
            100,
            channel::ChannelType::OpenAi,
            "https://a",
        );
        let account = mk_account(10, 1, "sk-a", 1, 100);
        let err = build_service_target(&channel, &account, "", "x").unwrap_err();
        assert!(matches!(err, RelayError::MissingConfig(_)));
    }

    #[test]
    fn build_service_target_oauth_account_returns_not_implemented() {
        let channel = mk_channel(
            1,
            vec!["x"],
            1,
            100,
            channel::ChannelType::OpenAi,
            "https://a",
        );
        let mut account = mk_account(10, 1, "sk-a", 1, 100);
        account.credential_type = "oauth".into();
        account.credentials = serde_json::json!({
            "oauth": {
                "access_token": "at",
                "refresh_token": "rt",
                "expires_at": "2026-04-20T12:00:00Z"
            }
        });
        let err = build_service_target(&channel, &account, "ignored", "x").unwrap_err();
        assert!(matches!(err, RelayError::NotImplemented(m) if m.contains("oauth")));
    }

    #[test]
    fn channel_type_to_adapter_kind_basic_cases() {
        use channel::ChannelType as D;
        assert_eq!(channel_type_to_adapter_kind(D::OpenAi), AdapterKind::OpenAI);
        assert_eq!(
            channel_type_to_adapter_kind(D::Anthropic),
            AdapterKind::Claude
        );
        assert_eq!(channel_type_to_adapter_kind(D::Gemini), AdapterKind::Gemini);
        assert_eq!(channel_type_to_adapter_kind(D::Azure), AdapterKind::Azure);
        assert_eq!(channel_type_to_adapter_kind(D::Ollama), AdapterKind::Ollama);
    }

    // ---------- weighted_pick（用 tuple stub，不依赖 channel::Model）----------

    #[derive(Debug, Clone, Copy)]
    struct Item {
        id: i32,
        priority: i32,
        weight: i32,
    }

    fn key_of(i: &Item) -> (i32, i32) {
        (i.priority, i.weight)
    }

    #[test]
    fn weighted_pick_returns_none_on_empty() {
        let items: Vec<&Item> = Vec::new();
        assert!(weighted_pick(&items, key_of).is_none());
    }

    #[test]
    fn weighted_pick_top_priority_wins() {
        let a = Item {
            id: 1,
            priority: 1,
            weight: 100,
        };
        let b = Item {
            id: 2,
            priority: 10,
            weight: 100,
        };
        let items = vec![&a, &b];
        for _ in 0..50 {
            assert_eq!(weighted_pick(&items, key_of).unwrap().id, 2);
        }
    }

    #[test]
    fn weighted_pick_weight_distribution_within_same_priority() {
        let a = Item {
            id: 1,
            priority: 1,
            weight: 99,
        };
        let b = Item {
            id: 2,
            priority: 1,
            weight: 1,
        };
        let items = vec![&a, &b];
        let n = 400;
        let mut count_a = 0;
        for _ in 0..n {
            if weighted_pick(&items, key_of).unwrap().id == 1 {
                count_a += 1;
            }
        }
        assert!(count_a > n * 80 / 100, "a hits = {count_a} of {n}");
    }

    #[test]
    fn weighted_pick_zero_weight_falls_back_to_min_one() {
        let a = Item {
            id: 1,
            priority: 1,
            weight: 0,
        };
        let b = Item {
            id: 2,
            priority: 1,
            weight: -5,
        };
        let items = vec![&a, &b];
        // 两个 weight 被折成 1:1，任何一个都可能被选
        let picked = weighted_pick(&items, key_of).unwrap();
        assert!(picked.id == 1 || picked.id == 2);
    }
}
