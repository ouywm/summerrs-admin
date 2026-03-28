use anyhow::Context;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set};
use summer::plugin::Service;
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};

use summer_ai_model::dto::token::{
    CreateTokenDto, QueryTokenDto, RechargeTokenDto, UpdateTokenDto,
};
use summer_ai_model::entity::token::{self, TokenStatus};
use summer_ai_model::vo::token::{TokenCreatedVo, TokenVo};
use summer_common::error::{ApiErrors, ApiResult};

/// Token 验证后的信息
#[derive(Debug, Clone)]
pub struct TokenInfo {
    pub token_id: i64,
    pub user_id: i64,
    pub name: String,
    pub group: String,
    pub remain_quota: i64,
    pub unlimited_quota: bool,
    pub rpm_limit: i32,
    pub tpm_limit: i64,
    pub allowed_models: Vec<String>,
}

#[derive(Clone, Service)]
pub struct TokenService {
    #[inject(component)]
    db: DbConn,
}

impl TokenService {
    pub fn new(db: DbConn) -> Self {
        Self { db }
    }

    pub async fn list_tokens(
        &self,
        query: QueryTokenDto,
        pagination: Pagination,
    ) -> ApiResult<Page<TokenVo>> {
        let page = token::Entity::find()
            .filter(query)
            .order_by_desc(token::Column::Id)
            .page(&self.db, &pagination)
            .await
            .context("failed to list tokens")
            .map_err(ApiErrors::Internal)?;

        Ok(page.map(TokenVo::from_model))
    }

    pub async fn get_token(&self, id: i64) -> ApiResult<TokenVo> {
        let model = token::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("failed to query token")
            .map_err(ApiErrors::Internal)?
            .ok_or_else(|| ApiErrors::NotFound("token not found".into()))?;
        Ok(TokenVo::from_model(model))
    }

    pub async fn create_token(
        &self,
        dto: CreateTokenDto,
        operator: &str,
    ) -> ApiResult<TokenCreatedVo> {
        let (key, key_prefix, key_hash) = Self::generate_api_key();
        let model = dto
            .into_active_model(key_hash, key_prefix, operator)
            .insert(&self.db)
            .await
            .context("failed to create token")
            .map_err(ApiErrors::Internal)?;

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
        let mut active: token::ActiveModel = token::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("failed to query token")
            .map_err(ApiErrors::Internal)?
            .ok_or_else(|| ApiErrors::NotFound("token not found".into()))?
            .into();

        dto.apply_to(&mut active, operator);
        active
            .update(&self.db)
            .await
            .context("failed to update token")
            .map_err(ApiErrors::Internal)?;
        Ok(())
    }

    pub async fn disable_token(&self, id: i64, operator: &str) -> ApiResult<()> {
        let mut active: token::ActiveModel = token::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("failed to query token")
            .map_err(ApiErrors::Internal)?
            .ok_or_else(|| ApiErrors::NotFound("token not found".into()))?
            .into();

        active.status = Set(TokenStatus::Disabled);
        active.update_by = Set(operator.to_string());
        active
            .update(&self.db)
            .await
            .context("failed to disable token")
            .map_err(ApiErrors::Internal)?;
        Ok(())
    }

    pub async fn recharge_token(
        &self,
        id: i64,
        dto: RechargeTokenDto,
        operator: &str,
    ) -> ApiResult<()> {
        let mut active: token::ActiveModel = token::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("failed to query token")
            .map_err(ApiErrors::Internal)?
            .ok_or_else(|| ApiErrors::NotFound("token not found".into()))?
            .into();

        let current_quota = active.remain_quota.clone().take().unwrap_or_default();
        active.remain_quota = Set(current_quota.saturating_add(dto.quota));
        if !dto.remark.trim().is_empty() {
            active.remark = Set(dto.remark);
        }
        if matches!(
            active.status.clone().take().unwrap_or(TokenStatus::Enabled),
            TokenStatus::Exhausted
        ) {
            active.status = Set(TokenStatus::Enabled);
        }
        active.update_by = Set(operator.to_string());
        active
            .update(&self.db)
            .await
            .context("failed to recharge token")
            .map_err(ApiErrors::Internal)?;
        Ok(())
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

        // 1. 计算 hash 查找 token
        let key_hash = hex::encode(Sha256::digest(raw_key.as_bytes()));
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

        // 5. 检查配额
        if !tk.unlimited_quota && tk.remain_quota <= 0 {
            return Err(ApiErrors::Forbidden("quota exceeded".into()));
        }

        // 6. 异步更新访问时间（不阻塞验证流程）
        let db = self.db.clone();
        let token_id = tk.id;
        tokio::spawn(async move {
            let now = chrono::Utc::now().fixed_offset();
            let _ = token::Entity::update_many()
                .col_expr(
                    token::Column::AccessTime,
                    sea_orm::sea_query::Expr::value(now),
                )
                .filter(token::Column::Id.eq(token_id))
                .exec(&db)
                .await;
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

        Ok(TokenInfo {
            token_id: tk.id,
            user_id: tk.user_id,
            name: tk.name,
            group,
            remain_quota: tk.remain_quota,
            unlimited_quota: tk.unlimited_quota,
            rpm_limit: tk.rpm_limit,
            tpm_limit: tk.tpm_limit,
            allowed_models,
        })
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
        let db = self.db.clone();
        let client_ip = client_ip.into();

        tokio::spawn(async move {
            let _ = token::Entity::update_many()
                .col_expr(
                    token::Column::LastUsedIp,
                    sea_orm::sea_query::Expr::value(client_ip),
                )
                .filter(token::Column::Id.eq(token_id))
                .exec(&db)
                .await;
        });
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
}
