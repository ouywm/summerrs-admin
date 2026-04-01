use anyhow::Context;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use summer::plugin::Service;
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};

use summer_ai_model::dto::token::{
    CreateTokenDto, QueryTokenDto, RechargeTokenDto, UpdateTokenDto,
};
use summer_ai_model::entity::token::{self, TokenStatus};
use summer_ai_model::vo::token::{TokenCreatedVo, TokenVo};
use summer_common::error::{ApiErrors, ApiResult};

use crate::service::runtime_cache::RuntimeCacheService;

const VALIDATED_TOKEN_CACHE_TTL_SECONDS: u64 = 10;

/// Token 验证后的信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenInfo {
    pub token_id: i64,
    pub user_id: i64,
    pub name: String,
    pub group: String,
    pub remain_quota: i64,
    pub unlimited_quota: bool,
    pub rpm_limit: i32,
    pub tpm_limit: i64,
    pub concurrency_limit: i32,
    pub allowed_models: Vec<String>,
    pub endpoint_scopes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ValidatedTokenCacheEntry {
    token_info: TokenInfo,
    status: TokenStatus,
    expire_time: Option<chrono::DateTime<chrono::FixedOffset>>,
}

#[derive(Clone, Service)]
pub struct TokenService {
    #[inject(component)]
    db: DbConn,
    #[inject(component)]
    cache: RuntimeCacheService,
    #[inject(component)]
    ip_batch_sender: Option<Arc<tokio::sync::mpsc::Sender<(i64, String)>>>,
}

impl TokenService {
    pub fn new(db: DbConn, cache: RuntimeCacheService) -> Self {
        Self {
            db,
            cache,
            ip_batch_sender: None,
        }
    }

    /// 验证 Bearer token，返回 TokenInfo
    ///
    /// 流程：
    /// 1. 提取 "sk-xxx" → SHA-256 哈希 → 查 ai.token.key_hash
    /// 2. 检查 status == Enabled
    /// 3. 检查 expire_time 未过期
    /// 4. 检查配额是否可用
    /// 5. 更新 access_time
    /// 6. 返回 TokenInfo（含可选模型白名单，供业务层自行校验）
    pub async fn validate(&self, raw_key: &str) -> ApiResult<TokenInfo> {
        use sha2::{Digest, Sha256};

        let key_hash = hex::encode(Sha256::digest(raw_key.as_bytes()));
        let cache_key = token_cache_key(&key_hash);
        if let Some(entry) = self
            .cache
            .get_json::<ValidatedTokenCacheEntry>(&cache_key)
            .await?
        {
            if entry.status != TokenStatus::Enabled {
                return Err(ApiErrors::Unauthorized(format!(
                    "token is {}",
                    entry.status
                )));
            }
            if let Some(expire_time) = entry.expire_time
                && expire_time < chrono::Utc::now().fixed_offset()
            {
                if let Err(e) = self.cache.delete(&cache_key).await {
                    tracing::warn!(error = %e, "failed to delete expired token from cache");
                }
                return Err(ApiErrors::Unauthorized("token has expired".into()));
            }
            return Ok(entry.token_info);
        }

        let tk = token::Entity::find()
            .filter(token::Column::KeyHash.eq(&key_hash))
            .one(&self.db)
            .await
            .context("failed to query token")
            .map_err(ApiErrors::Internal)?
            .ok_or_else(|| ApiErrors::Unauthorized("invalid API key".into()))?;

        // 2. 检查状态
        if tk.status != TokenStatus::Enabled {
            return Err(ApiErrors::Unauthorized(format!("token is {}", tk.status)));
        }

        // 3. 检查过期
        if let Some(expire_time) = &tk.expire_time
            && *expire_time < chrono::Utc::now().fixed_offset()
        {
            return Err(ApiErrors::Unauthorized("token has expired".into()));
        }

        // 4. 解析模型白名单（是否允许访问由业务层自行决定）
        let allowed_models: Vec<String> = tk
            .models
            .as_array()
            .map(|models| {
                models
                    .iter()
                    .filter_map(|v| v.as_str().map(ToOwned::to_owned))
                    .collect()
            })
            .unwrap_or_default();

        let endpoint_scopes: Vec<String> = tk
            .endpoint_scopes
            .as_array()
            .map(|scopes| {
                scopes
                    .iter()
                    .filter_map(|v| v.as_str().map(ToOwned::to_owned))
                    .collect()
            })
            .unwrap_or_default();

        // 5. 检查配额
        if !tk.unlimited_quota && tk.remain_quota <= 0 {
            return Err(ApiErrors::Forbidden("quota exceeded".into()));
        }

        // 6. 异步更新访问时间（不阻塞验证流程）
        let db = self.db.clone();
        let token_id = tk.id;
        tokio::spawn(async move {
            let now = chrono::Utc::now().fixed_offset();
            if let Err(e) = token::Entity::update_many()
                .col_expr(
                    token::Column::AccessTime,
                    sea_orm::sea_query::Expr::value(now),
                )
                .filter(token::Column::Id.eq(token_id))
                .exec(&db)
                .await
            {
                tracing::warn!(error = %e, "failed to update token access time");
            }
        });

        // 7. 确定分组
        let group = if tk.group_code_override.is_empty() {
            // 查 user_quota 的 channel_group
            use summer_ai_model::entity::user_quota;
            let uq = user_quota::Entity::find()
                .filter(user_quota::Column::UserId.eq(tk.user_id))
                .one(&self.db)
                .await
                .context("failed to query user quota")
                .map_err(ApiErrors::Internal)?;
            uq.map(|q| q.channel_group)
                .unwrap_or_else(|| "default".into())
        } else {
            tk.group_code_override.clone()
        };

        let status = tk.status;
        let expire_time = tk.expire_time;
        let token_info = TokenInfo {
            token_id: tk.id,
            user_id: tk.user_id,
            name: tk.name,
            group,
            remain_quota: tk.remain_quota,
            unlimited_quota: tk.unlimited_quota,
            rpm_limit: tk.rpm_limit,
            tpm_limit: tk.tpm_limit,
            concurrency_limit: tk.concurrency_limit,
            allowed_models,
            endpoint_scopes,
        };
        let cache = self.cache.clone();
        let cached_token_info = token_info.clone();
        tokio::spawn(async move {
            let entry = ValidatedTokenCacheEntry {
                token_info: cached_token_info,
                status,
                expire_time,
            };
            if let Err(error) = cache
                .set_json(&cache_key, &entry, VALIDATED_TOKEN_CACHE_TTL_SECONDS)
                .await
            {
                tracing::warn!("failed to cache validated token info: {error}");
            }
        });

        Ok(token_info)
    }

    /// 生成新的 API Key，返回 (明文 key, prefix, hash)
    pub fn generate_api_key() -> (String, String, String) {
        use rand::RngExt;
        use sha2::{Digest, Sha256};

        let random: String = rand::rng()
            .sample_iter(&rand::distr::Alphanumeric)
            .take(48)
            .map(char::from)
            .collect();
        let key = format!("sk-{random}");
        let prefix = key[..12].to_string();
        let hash = hex::encode(Sha256::digest(key.as_bytes()));
        (key, prefix, hash)
    }

    pub fn update_last_used_ip_async(&self, token_id: i64, client_ip: impl Into<String>) {
        if let Some(ref sender) = self.ip_batch_sender {
            let _ = sender.try_send((token_id, client_ip.into()));
        }
    }

    /// Start the background batch writer for last-used IP updates.
    ///
    /// Deduplicates by token_id (keeps the latest IP) and flushes to DB
    /// every 10 seconds or when the batch reaches 64 entries.
    pub fn start_ip_batch_writer(&mut self) {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<(i64, String)>(512);
        let db = self.db.clone();

        tokio::spawn(async move {
            let mut pending: HashMap<i64, String> = HashMap::new();
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            loop {
                tokio::select! {
                    Some((token_id, ip)) = rx.recv() => {
                        pending.insert(token_id, ip);
                        if pending.len() >= 64 {
                            flush_ip_batch(&db, &mut pending).await;
                        }
                    }
                    _ = interval.tick() => {
                        if !pending.is_empty() {
                            flush_ip_batch(&db, &mut pending).await;
                        }
                    }
                }
            }
        });

        self.ip_batch_sender = Some(Arc::new(tx));
    }

    pub async fn list_tokens(
        &self,
        query: QueryTokenDto,
        pagination: Pagination,
    ) -> ApiResult<Page<TokenVo>> {
        let page = token::Entity::find()
            .filter(query)
            .order_by_desc(token::Column::UpdateTime)
            .order_by_desc(token::Column::Id)
            .page(&self.db, &pagination)
            .await
            .context("查询令牌列表失败")?;

        Ok(page.map(TokenVo::from_model))
    }

    pub async fn get_token(&self, id: i64) -> ApiResult<TokenVo> {
        Ok(TokenVo::from_model(self.find_token_model(id).await?))
    }

    pub async fn create_token(
        &self,
        dto: CreateTokenDto,
        operator: &str,
    ) -> ApiResult<TokenCreatedVo> {
        let (key, prefix, hash) = Self::generate_api_key();
        let model = dto
            .into_active_model(hash, prefix, operator)
            .map_err(ApiErrors::BadRequest)?
            .insert(&self.db)
            .await
            .context("创建令牌失败")?;

        Ok(TokenCreatedVo {
            key,
            token: TokenVo::from_model(model),
        })
    }

    pub async fn update_token(
        &self,
        id: i64,
        dto: UpdateTokenDto,
        operator: &str,
    ) -> ApiResult<()> {
        let model = self.find_token_model(id).await?;
        let mut active: token::ActiveModel = model.clone().into();
        dto.apply_to(&mut active, operator)
            .map_err(ApiErrors::BadRequest)?;
        active.update(&self.db).await.context("更新令牌失败")?;
        self.invalidate_token_cache(&model.key_hash).await?;
        Ok(())
    }

    pub async fn disable_token(&self, id: i64, operator: &str) -> ApiResult<()> {
        let model = self.find_token_model(id).await?;
        let mut active: token::ActiveModel = model.clone().into();
        active.status = Set(TokenStatus::Disabled);
        active.update_by = Set(operator.to_string());
        active.update(&self.db).await.context("禁用令牌失败")?;
        self.invalidate_token_cache(&model.key_hash).await?;
        Ok(())
    }

    pub async fn recharge_token(
        &self,
        id: i64,
        dto: RechargeTokenDto,
        operator: &str,
    ) -> ApiResult<()> {
        let model = self.find_token_model(id).await?;
        let mut active: token::ActiveModel = model.clone().into();
        active.remain_quota = Set(model.remain_quota + dto.quota);
        active.update_by = Set(operator.to_string());
        if !dto.remark.is_empty() {
            active.remark = Set(dto.remark);
        }
        active.update(&self.db).await.context("充值令牌额度失败")?;
        self.invalidate_token_cache(&model.key_hash).await?;
        Ok(())
    }

    async fn find_token_model(&self, id: i64) -> ApiResult<token::Model> {
        token::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询令牌详情失败")?
            .ok_or_else(|| ApiErrors::NotFound("令牌不存在".to_string()))
    }

    async fn invalidate_token_cache(&self, key_hash: &str) -> ApiResult<()> {
        self.cache.delete(&token_cache_key(key_hash)).await
    }
}

impl TokenInfo {
    pub fn ensure_model_allowed(&self, model: &str) -> ApiResult<()> {
        if model.is_empty()
            || self.allowed_models.is_empty()
            || self.allowed_models.iter().any(|allowed| allowed == model)
        {
            return Ok(());
        }

        Err(ApiErrors::Forbidden(format!(
            "token is not allowed to access model: {model}"
        )))
    }

    pub fn ensure_endpoint_allowed(&self, scope: &str) -> ApiResult<()> {
        if scope.is_empty()
            || self.endpoint_scopes.is_empty()
            || self.endpoint_scopes.iter().any(|allowed| allowed == scope)
        {
            return Ok(());
        }

        Err(ApiErrors::BadRequest(format!(
            "endpoint is not enabled for this token: {scope}"
        )))
    }

    pub fn ensure_endpoint_scope_allowed(&self, scope: &str) -> ApiResult<()> {
        self.ensure_endpoint_allowed(scope)
    }
}

fn token_cache_key(key_hash: &str) -> String {
    format!("ai:cache:token:{key_hash}")
}

async fn flush_ip_batch(db: &DbConn, pending: &mut HashMap<i64, String>) {
    for (token_id, ip) in pending.drain() {
        if let Err(e) = token::Entity::update_many()
            .col_expr(
                token::Column::LastUsedIp,
                sea_orm::sea_query::Expr::value(ip),
            )
            .filter(token::Column::Id.eq(token_id))
            .exec(db)
            .await
        {
            tracing::warn!(error = %e, token_id, "failed to batch-update token last used ip");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_token_info(endpoint_scopes: &[&str]) -> TokenInfo {
        TokenInfo {
            token_id: 1,
            user_id: 1,
            name: "demo".into(),
            group: "default".into(),
            remain_quota: 100,
            unlimited_quota: false,
            rpm_limit: 60,
            tpm_limit: 1000,
            concurrency_limit: 2,
            allowed_models: Vec::new(),
            endpoint_scopes: endpoint_scopes
                .iter()
                .map(|scope| (*scope).to_string())
                .collect(),
        }
    }

    #[test]
    fn endpoint_scope_check_allows_empty_whitelist() {
        let token = sample_token_info(&[]);
        assert!(token.ensure_endpoint_allowed("responses").is_ok());
    }

    #[test]
    fn endpoint_scope_check_rejects_unknown_scope() {
        let token = sample_token_info(&["chat"]);
        let error = token.ensure_endpoint_allowed("responses").unwrap_err();
        assert!(matches!(error, ApiErrors::BadRequest(_)));
        assert!(error.to_string().contains("responses"));
    }

    #[test]
    fn endpoint_scope_check_accepts_configured_scope() {
        let token = sample_token_info(&["chat", "responses"]);
        assert!(token.ensure_endpoint_allowed("responses").is_ok());
    }
}
