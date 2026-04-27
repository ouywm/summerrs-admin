use anyhow::Context;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set};
use sha2::{Digest, Sha256};
use summer::plugin::Service;
use summer_ai_model::dto::token::{
    CreateTokenDto, TokenQueryDto, UpdateTokenDto, UpdateTokenStatusDto,
};
use summer_ai_model::entity::billing::token::{self, TokenStatus};
use summer_ai_model::vo::token::{CreatedTokenVo, RotatedTokenKeyVo, TokenDetailVo, TokenVo};
use summer_common::error::{ApiErrors, ApiResult};
use summer_redis::Redis;
use summer_redis::redis::AsyncCommands;
use summer_sea_orm::DbConn;
use summer_sea_orm::pagination::{Page, Pagination, PaginationExt};
use uuid::Uuid;

#[derive(Clone, Service)]
pub struct TokenService {
    #[inject(component)]
    db: DbConn,
    #[inject(component)]
    redis: Redis,
}

impl TokenService {
    pub async fn create(&self, dto: CreateTokenDto, operator: &str) -> ApiResult<CreatedTokenVo> {
        dto.validate_business_rules()
            .map_err(ApiErrors::BadRequest)?;
        ensure_create_status_is_valid(&dto)?;

        let raw_key = new_raw_token();
        let key_hash = sha256_hex(&raw_key);
        let key_prefix = token_prefix(&raw_key);
        self.ensure_unique_key_hash(&key_hash).await?;

        let model = dto
            .into_active_model(operator, &raw_key, &key_hash, &key_prefix)
            .insert(&self.db)
            .await
            .context("创建 API Token 失败")?;

        Ok(CreatedTokenVo {
            token: TokenVo::from_model(model),
            raw_key,
        })
    }

    pub async fn update(&self, id: i64, dto: UpdateTokenDto, operator: &str) -> ApiResult<()> {
        dto.validate_business_rules()
            .map_err(ApiErrors::BadRequest)?;
        let model = self.find_model_by_id(id).await?;
        ensure_update_status_is_valid(&model, &dto)?;

        self.invalidate_token_hash(&model.key_hash).await?;
        let mut active: token::ActiveModel = model.clone().into();
        dto.apply_to(&mut active, operator);
        active
            .update(&self.db)
            .await
            .context("更新 API Token 失败")?;
        self.invalidate_token_hash(&model.key_hash).await?;

        Ok(())
    }

    pub async fn update_status(
        &self,
        id: i64,
        dto: UpdateTokenStatusDto,
        operator: &str,
    ) -> ApiResult<()> {
        let model = self.find_model_by_id(id).await?;
        if dto.status == TokenStatus::Enabled {
            ensure_can_enable(model.unlimited_quota, model.remain_quota, model.expire_time)?;
        }

        self.invalidate_token_hash(&model.key_hash).await?;
        let mut active: token::ActiveModel = model.clone().into();
        active.status = Set(dto.status);
        active.update_by = Set(operator.to_string());
        active
            .update(&self.db)
            .await
            .context("更新 API Token 状态失败")?;
        self.invalidate_token_hash(&model.key_hash).await?;

        Ok(())
    }

    pub async fn rotate_key(&self, id: i64, operator: &str) -> ApiResult<RotatedTokenKeyVo> {
        let model = self.find_model_by_id(id).await?;
        let raw_key = new_raw_token();
        let key_hash = sha256_hex(&raw_key);
        let key_prefix = token_prefix(&raw_key);
        self.ensure_unique_key_hash(&key_hash).await?;

        self.invalidate_token_hash(&model.key_hash).await?;
        let mut active: token::ActiveModel = model.into();
        active.key_hash = Set(key_hash);
        active.key_prefix = Set(key_prefix.clone());
        active.update_by = Set(operator.to_string());
        let updated = active
            .update(&self.db)
            .await
            .context("轮换 API Token Key 失败")?;

        Ok(RotatedTokenKeyVo {
            id: updated.id,
            key_prefix,
            raw_key,
        })
    }

    pub async fn delete(&self, id: i64) -> ApiResult<()> {
        let model = self.find_model_by_id(id).await?;
        self.invalidate_token_hash(&model.key_hash).await?;
        token::Entity::delete_by_id(id)
            .exec(&self.db)
            .await
            .context("删除 API Token 失败")?;
        self.invalidate_token_hash(&model.key_hash).await?;
        Ok(())
    }

    pub async fn batch_delete(&self, ids: Vec<i64>) -> ApiResult<u64> {
        if ids.is_empty() {
            return Err(ApiErrors::BadRequest("ids 不能为空".to_string()));
        }

        let models = token::Entity::find()
            .filter(token::Column::Id.is_in(ids))
            .all(&self.db)
            .await
            .context("查询待删除 API Token 失败")?;

        for model in &models {
            self.invalidate_token_hash(&model.key_hash).await?;
        }

        let affected = token::Entity::delete_many()
            .filter(token::Column::Id.is_in(models.iter().map(|m| m.id).collect::<Vec<_>>()))
            .exec(&self.db)
            .await
            .context("批量删除 API Token 失败")?
            .rows_affected;

        for model in &models {
            self.invalidate_token_hash(&model.key_hash).await?;
        }

        Ok(affected)
    }

    pub async fn list(
        &self,
        query: TokenQueryDto,
        pagination: Pagination,
    ) -> ApiResult<Page<TokenVo>> {
        let page: Page<token::Model> = token::Entity::find()
            .filter(query)
            .order_by_desc(token::Column::Id)
            .page(&self.db, &pagination)
            .await
            .context("查询 API Token 列表失败")?;

        Ok(page.map(TokenVo::from_model))
    }

    pub async fn detail(&self, id: i64) -> ApiResult<TokenDetailVo> {
        let model = self.find_model_by_id(id).await?;
        Ok(TokenDetailVo::from_model(model))
    }

    async fn find_model_by_id(&self, id: i64) -> ApiResult<token::Model> {
        token::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("查询 API Token 详情失败")?
            .ok_or_else(|| ApiErrors::NotFound(format!("API Token 不存在: id={id}")))
    }

    async fn ensure_unique_key_hash(&self, key_hash: &str) -> ApiResult<()> {
        let exists = token::Entity::find()
            .filter(token::Column::KeyHash.eq(key_hash))
            .one(&self.db)
            .await
            .context("检查 API Token Key 唯一性失败")?;
        if exists.is_some() {
            return Err(ApiErrors::Conflict(
                "API Token Key 已存在，请重试".to_string(),
            ));
        }
        Ok(())
    }

    async fn invalidate_token_hash(&self, key_hash: &str) -> ApiResult<()> {
        let mut conn = self.redis.clone();
        conn.del::<_, ()>(token_cache_key(key_hash))
            .await
            .context("清理 API Token 缓存失败")?;
        Ok(())
    }
}

pub fn new_raw_token() -> String {
    format!("sk-{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

pub fn sha256_hex(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    hex::encode(hasher.finalize())
}

pub fn token_prefix(raw_key: &str) -> String {
    let prefix_len = raw_key.len().min(11);
    raw_key[..prefix_len].to_string()
}

pub fn token_cache_key(key_hash: &str) -> String {
    format!("ai:tk:{key_hash}")
}

fn ensure_create_status_is_valid(dto: &CreateTokenDto) -> ApiResult<()> {
    if dto.status.unwrap_or(TokenStatus::Enabled) == TokenStatus::Enabled {
        ensure_can_enable(
            dto.unlimited_quota.unwrap_or(false),
            dto.remain_quota.unwrap_or(0),
            dto.expire_time,
        )?;
    }
    Ok(())
}

fn ensure_update_status_is_valid(model: &token::Model, dto: &UpdateTokenDto) -> ApiResult<()> {
    let next_status = dto.status.unwrap_or(model.status);
    if next_status == TokenStatus::Enabled {
        ensure_can_enable(
            dto.unlimited_quota.unwrap_or(model.unlimited_quota),
            dto.remain_quota.unwrap_or(model.remain_quota),
            dto.expire_time.or(model.expire_time),
        )?;
    }
    Ok(())
}

fn ensure_can_enable(
    unlimited_quota: bool,
    remain_quota: i64,
    expire_time: Option<chrono::DateTime<chrono::FixedOffset>>,
) -> ApiResult<()> {
    if !unlimited_quota && remain_quota <= 0 {
        return Err(ApiErrors::BadRequest(
            "非无限额度 Token 剩余额度必须大于 0 才能启用".to_string(),
        ));
    }
    if expire_time.is_some_and(|expire| expire <= chrono::Utc::now().fixed_offset()) {
        return Err(ApiErrors::BadRequest(
            "已过期 Token 不能启用，请先设置未来过期时间".to_string(),
        ));
    }
    Ok(())
}
