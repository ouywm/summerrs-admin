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

use crate::error::RelayError;
use crate::service::oauth::openai_token_provider::OpenAiTokenProvider;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use summer::plugin::Service;
use summer_ai_core::{
    AdapterError, AdapterKind, AuthData, Endpoint, EndpointScope, ServiceTarget, parse_json_scopes,
};
use summer_ai_model::entity::routing::{channel, channel_account};
use summer_redis::Redis;
use summer_redis::redis;
use summer_redis::redis::AsyncCommands;
use summer_sea_orm::DbConn;

/// Redis key TTL（秒）。故意设短，TTL 兜底失效路径。
const CACHE_TTL_SECS: u64 = 300;

// ---------------------------------------------------------------------------
// channel.channel_type + scope → AdapterKind 映射
// ---------------------------------------------------------------------------

/// 由 `(channel.channel_type, scope)` 推导出 dispatch 用的 [`AdapterKind`]
pub fn resolve_adapter_kind(c: channel::ChannelType, scope: EndpointScope) -> Option<AdapterKind> {
    use EndpointScope as S;
    use channel::ChannelType as C;
    Some(match (c, scope) {
        (C::OpenAi, S::Chat) => AdapterKind::OpenAI,
        (C::OpenAi, S::Responses) => AdapterKind::OpenAIResp,
        (C::Anthropic, S::Chat) => AdapterKind::Claude,
        (C::Azure, S::Chat) => AdapterKind::OpenAICompat,
        (C::Gemini, S::Chat) => AdapterKind::Gemini,
        (C::Ollama, S::Chat) => AdapterKind::OpenAI,
        (C::Ali, S::Chat) => AdapterKind::OpenAI,
        (C::Baidu, S::Chat) => AdapterKind::OpenAI,
        _ => return None,
    })
}

/// 校验 `(channel, scope)` 组合是否可路由。
///
/// 同时满足：
/// - `channel.endpoint_scopes` JSONB 包含该 scope
/// - `(channel.channel_type, scope)` 能映射出 [`AdapterKind`]
fn channel_supports_scope(channel: &channel::Model, scope: EndpointScope) -> bool {
    parse_json_scopes(&channel.endpoint_scopes).contains(&scope)
        && resolve_adapter_kind(channel.channel_type, scope).is_some()
}

// ---------------------------------------------------------------------------
// ChannelStore
// ---------------------------------------------------------------------------

/// `candidates()` 返回的单个路由候选。
///
/// 由 P9 retry 循环迭代消费：失败时切下一个候选，直到成功或清单用尽。
#[derive(Debug, Clone)]
pub struct Candidate {
    pub channel: channel::Model,
    pub account: channel_account::Model,
    pub selected_key: String,
}

/// 渠道 / 账号路由存储（Redis cache + DB fallback）
#[derive(Clone, Service)]
pub struct ChannelStore {
    #[inject(component)]
    db: DbConn,
    #[inject(component)]
    redis: Redis,
}

impl ChannelStore {
    /// 按逻辑模型名列出全部可用候选(channel × account × key),按 P9 retry 期望的顺序排列。
    ///
    /// 排列规则:
    /// 1. channel 按 `priority` 降序分组;同组内按 `weight` 加权随机**洗牌**(不是只挑一个)
    /// 2. 每个 channel 下再按同规则展开 `account`——过滤掉 `rate_limited_until` / `overload_until`
    ///    未过期的 account
    /// 3. 每个 account 把 `enabled_api_keys()` **全部展开**为多个候选——P9 韧性层语义:
    ///    SameChannel 错(典型 429)切到同 account 的下一个 key 重试,而不是直接换 channel
    ///
    /// 返 `Vec<Candidate>`,可能为空(表示路由空 —— handler 返 `NoAvailableChannel`)。
    pub async fn candidates(
        &self,
        logical_model: &str,
        scope: EndpointScope,
    ) -> Result<Vec<Candidate>, RelayError> {
        let channel_ids = self.load_model_channels(logical_model).await?;
        if channel_ids.is_empty() {
            return Ok(Vec::new());
        }

        let channels: Vec<channel::Model> = self
            .load_channels(&channel_ids)
            .await?
            .into_iter()
            .filter(|channel| channel_supports_scope(channel, scope))
            .collect();
        if channels.is_empty() {
            return Ok(Vec::new());
        }

        // 组装 Candidate 顺序:channel 洗牌 × account 洗牌 × 同 account 内 key 全展开
        let channel_order = weighted_shuffle(channels, |c| (c.priority, c.weight));
        let now = chrono::Utc::now().fixed_offset();

        let mut out: Vec<Candidate> = Vec::new();
        for channel in channel_order {
            let account_ids = self.load_channel_accounts(channel.id).await?;
            if account_ids.is_empty() {
                continue;
            }
            let accounts = self.load_accounts(&account_ids).await?;
            let alive: Vec<channel_account::Model> = accounts
                .into_iter()
                .filter(|a| a.rate_limited_until.map(|t| t < now).unwrap_or(true))
                .filter(|a| a.overload_until.map(|t| t < now).unwrap_or(true))
                .collect();
            let account_order = weighted_shuffle(alive, |a| (a.priority, a.weight));
            for account in account_order {
                // OAuth account: selected_key 是空串(凭证从 access_token 拿,见 resolve_auth_data)。
                // 当前只有 OpenAI 渠道支持 OAuth credentials。
                if account.is_oauth() {
                    if channel.channel_type == channel::ChannelType::OpenAi {
                        out.push(Candidate {
                            channel: channel.clone(),
                            account: account.clone(),
                            selected_key: String::new(),
                        });
                    } else {
                        tracing::debug!(
                            account_id = account.id,
                            channel_type = ?channel.channel_type,
                            "oauth credentials only supported for OpenAI channels, skipping"
                        );
                    }
                    continue;
                }

                let keys = account.enabled_api_keys();
                if keys.is_empty() {
                    tracing::debug!(
                        account_id = account.id,
                        channel_id = account.channel_id,
                        "account has no enabled api key, skipping"
                    );
                    continue;
                }
                for key in keys {
                    out.push(Candidate {
                        channel: channel.clone(),
                        account: account.clone(),
                        selected_key: key,
                    });
                }
            }
        }
        Ok(out)
    }

    pub async fn build_service_target(
        &self,
        http: &reqwest::Client,
        channel: &channel::Model,
        account: &channel_account::Model,
        selected_key: &str,
        logical_model: &str,
        scope: EndpointScope,
    ) -> Result<ServiceTarget, RelayError> {
        let auth =
            resolve_auth_data(&self.db, &self.redis, http, channel, account, selected_key).await?;
        build_service_target_with_auth(channel, account, auth, logical_model, scope)
    }

    /// 兼容旧调用方的薄包装：返 `candidates()` 的第一个候选。
    ///
    /// 新代码（pipeline P9 retry）应直接用 `candidates()`；该方法保留给未接入 retry 的调用点。
    #[deprecated(note = "use candidates(logical_model, scope) instead")]
    pub async fn pick(
        &self,
        logical_model: &str,
    ) -> Result<Option<(channel::Model, channel_account::Model, String)>, RelayError> {
        let mut list = self.candidates(logical_model, EndpointScope::Chat).await?;
        if list.is_empty() {
            Ok(None)
        } else {
            let c = list.remove(0);
            Ok(Some((c.channel, c.account, c.selected_key)))
        }
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

/// 解析 account 在本次请求该使用的鉴权数据。
pub async fn resolve_auth_data(
    db: &DbConn,
    redis: &Redis,
    http: &reqwest::Client,
    channel: &channel::Model,
    account: &channel_account::Model,
    selected_key: &str,
) -> Result<AuthData, RelayError> {
    if account.is_oauth() {
        if channel.channel_type != channel::ChannelType::OpenAi {
            return Err(RelayError::NotImplemented("oauth credentials"));
        }
        return OpenAiTokenProvider::new(db, redis, http)
            .auth_data_for_account(account)
            .await;
    }

    if selected_key.is_empty() {
        return Err(RelayError::MissingConfig(
            "channel_account selected_key is empty (multi-key pool exhausted?)",
        ));
    }
    Ok(AuthData::from_single(selected_key.to_string()))
}

/// 用已解析好的 auth 构造 `ServiceTarget`。
pub fn build_service_target_with_auth(
    channel: &channel::Model,
    _account: &channel_account::Model,
    auth: AuthData,
    logical_model: &str,
    scope: EndpointScope,
) -> Result<ServiceTarget, RelayError> {
    let kind = resolve_adapter_kind(channel.channel_type, scope).ok_or(RelayError::Adapter(
        AdapterError::Unsupported {
            adapter: "channel_store",
            feature: "channel_type_scope_combination",
        },
    ))?;
    let actual_model = channel.resolve_upstream_model(logical_model);
    let endpoint = Endpoint::from_owned(channel.base_url.clone());

    let mut target = ServiceTarget::new(
        endpoint,
        auth,
        summer_ai_core::ModelIden::new(kind, actual_model),
    );

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

    Ok(target)
}

// ---------------------------------------------------------------------------
// weighted_shuffle —— 纯函数（测试覆盖）
// ---------------------------------------------------------------------------

/// 把 `items` 按 `priority` 降序分组；组内按 `weight` 反复加权随机抽取（不放回），
/// 组间按 priority 降序拼接后返回。
///
/// 第一个元素的分布 = "一次加权随机"的结果；P9 retry 外层把整个序列作为尝试顺序消费。
fn weighted_shuffle<T, K>(items: Vec<T>, key: K) -> Vec<T>
where
    K: Fn(&T) -> (i32, i32),
{
    if items.is_empty() {
        return Vec::new();
    }

    let mut by_priority: std::collections::BTreeMap<i32, Vec<T>> = Default::default();
    for it in items {
        let (pri, _) = key(&it);
        by_priority.entry(pri).or_default().push(it);
    }

    let mut out: Vec<T> = Vec::new();
    // BTreeMap 迭代升序；要降序
    for (_pri, mut group) in by_priority.into_iter().rev() {
        while !group.is_empty() {
            let total: u64 = group.iter().map(|t| key(t).1.max(1) as u64).sum();
            let picked_idx = if total == 0 {
                0
            } else {
                let mut roll = rand::random_range(0..total);
                let mut idx = group.len() - 1;
                for (i, t) in group.iter().enumerate() {
                    let w = key(t).1.max(1) as u64;
                    if roll < w {
                        idx = i;
                        break;
                    }
                    roll -= w;
                }
                idx
            };
            out.push(group.remove(picked_idx));
        }
    }
    out
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
            vendor_code: "openai".to_string(),
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
    fn build_service_target_maps_chat_scope_and_applies_model_mapping() {
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

        let target = build_service_target_with_auth(
            &channel,
            &account,
            AuthData::from_single("sk-a"),
            "gpt-4",
            EndpointScope::Chat,
        )
        .unwrap();
        assert_eq!(target.kind(), AdapterKind::OpenAI);
        assert_eq!(target.actual_model(), "gpt-4-turbo");
        assert_eq!(target.endpoint.trimmed(), "https://api.openai.com/v1");
    }

    #[test]
    fn build_service_target_maps_responses_scope_to_openai_resp() {
        let channel = mk_channel(
            1,
            vec!["gpt-5"],
            1,
            100,
            channel::ChannelType::OpenAi,
            "https://api.openai.com/v1",
        );
        let account = mk_account(10, 1, "sk-a", 1, 100);

        let target = build_service_target_with_auth(
            &channel,
            &account,
            AuthData::from_single("sk-a"),
            "gpt-5",
            EndpointScope::Responses,
        )
        .unwrap();

        assert_eq!(target.kind(), AdapterKind::OpenAIResp);
    }

    #[test]
    fn build_service_target_anthropic_chat_routes_to_claude() {
        let channel = mk_channel(
            1,
            vec!["claude-sonnet"],
            1,
            100,
            channel::ChannelType::Anthropic,
            "https://api.anthropic.com",
        );
        let account = mk_account(10, 1, "sk-a", 1, 100);

        let target = build_service_target_with_auth(
            &channel,
            &account,
            AuthData::from_single("sk-a"),
            "claude-sonnet",
            EndpointScope::Chat,
        )
        .unwrap();

        assert_eq!(target.kind(), AdapterKind::Claude);
        assert_eq!(target.endpoint.trimmed(), "https://api.anthropic.com");
    }

    #[test]
    fn build_service_target_with_auth_accepts_oauth_account_and_empty_selected_key() {
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
            "access_token": "at",
            "refresh_token": "rt",
            "id_token": "id",
            "expires_at": "2026-04-20T12:00:00Z",
            "client_id": "app_test"
        });
        let target = build_service_target_with_auth(
            &channel,
            &account,
            AuthData::from_single("at"),
            "x",
            EndpointScope::Chat,
        )
        .unwrap();
        assert_eq!(target.actual_model(), "x");
        assert_eq!(target.auth.resolve().unwrap().as_deref(), Some("at"));
    }

    #[test]
    fn resolve_adapter_kind_uses_channel_type_and_scope() {
        use channel::ChannelType as C;

        assert_eq!(
            resolve_adapter_kind(C::OpenAi, EndpointScope::Chat),
            Some(AdapterKind::OpenAI)
        );
        assert_eq!(
            resolve_adapter_kind(C::OpenAi, EndpointScope::Responses),
            Some(AdapterKind::OpenAIResp)
        );
        assert_eq!(
            resolve_adapter_kind(C::Anthropic, EndpointScope::Chat),
            Some(AdapterKind::Claude)
        );
        assert_eq!(
            resolve_adapter_kind(C::Anthropic, EndpointScope::Responses),
            None
        );
        assert_eq!(
            resolve_adapter_kind(C::Gemini, EndpointScope::Chat),
            Some(AdapterKind::Gemini)
        );
    }

    #[test]
    fn channel_supports_scope_combines_endpoint_scopes_and_channel_type() {
        let mut channel = mk_channel(
            1,
            vec!["gpt-4"],
            1,
            100,
            channel::ChannelType::OpenAi,
            "https://api.openai.com/v1",
        );

        // 同时勾选 + channel_type 支持 → ok
        channel.endpoint_scopes = serde_json::json!(["chat", "responses"]);
        assert!(channel_supports_scope(&channel, EndpointScope::Chat));
        assert!(channel_supports_scope(&channel, EndpointScope::Responses));
        // channel 没勾 embeddings → 拒
        assert!(!channel_supports_scope(&channel, EndpointScope::Embeddings));

        // channel_type=Anthropic 不支持 Responses 场景
        let mut anthropic_channel = mk_channel(
            2,
            vec!["claude"],
            1,
            100,
            channel::ChannelType::Anthropic,
            "https://api.anthropic.com",
        );
        anthropic_channel.endpoint_scopes = serde_json::json!(["chat", "responses"]);
        assert!(channel_supports_scope(
            &anthropic_channel,
            EndpointScope::Chat
        ));
        assert!(!channel_supports_scope(
            &anthropic_channel,
            EndpointScope::Responses
        ));
    }

    // ---------- weighted_shuffle（用 tuple stub，不依赖 channel::Model）----------

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
    fn weighted_shuffle_empty_input_returns_empty() {
        let out = weighted_shuffle::<Item, _>(Vec::new(), key_of);
        assert!(out.is_empty());
    }

    #[test]
    fn weighted_shuffle_orders_by_priority_desc() {
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
        // 运行多轮：高 priority 永远排第一
        for _ in 0..50 {
            let out = weighted_shuffle(vec![a, b], key_of);
            assert_eq!(out[0].id, 2);
            assert_eq!(out[1].id, 1);
        }
    }

    #[test]
    fn weighted_shuffle_returns_full_permutation() {
        let a = Item {
            id: 1,
            priority: 1,
            weight: 50,
        };
        let b = Item {
            id: 2,
            priority: 1,
            weight: 50,
        };
        let c = Item {
            id: 3,
            priority: 1,
            weight: 50,
        };
        let out = weighted_shuffle(vec![a, b, c], key_of);
        assert_eq!(out.len(), 3);
        let ids: std::collections::HashSet<_> = out.iter().map(|i| i.id).collect();
        assert_eq!(ids, [1, 2, 3].into_iter().collect());
    }

    #[test]
    fn weighted_shuffle_first_element_follows_weight_distribution() {
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
        let n = 400;
        let mut count_a = 0;
        for _ in 0..n {
            let out = weighted_shuffle(vec![a, b], key_of);
            if out[0].id == 1 {
                count_a += 1;
            }
        }
        // 99:1 权重 → 第一个应有 >= 80% 概率是 a
        assert!(count_a > n * 80 / 100, "a first hits = {count_a} of {n}");
    }

    #[test]
    fn weighted_shuffle_zero_or_negative_weight_folds_to_one() {
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
        let out = weighted_shuffle(vec![a, b], key_of);
        assert_eq!(out.len(), 2);
    }
}
