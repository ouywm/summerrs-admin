use anyhow::Context;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use summer::plugin::Service;
use summer_common::error::{ApiErrors, ApiResult};
use summer_sea_orm::DbConn;

use summer_ai_model::entity::token::{self, TokenStatus};
use summer_ai_model::entity::user_quota;

#[derive(Debug, Clone)]
pub struct TokenInfo {
    pub token_id: i64,
    pub user_id: i64,
    pub project_id: i64,
    pub service_account_id: i64,
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

#[derive(Clone, Service)]
pub struct TokenService {
    #[inject(component)]
    db: DbConn,
}

impl TokenService {
    // TODO(redis-auth-cache):
    // Cache token auth snapshots by `key_hash` in Redis to avoid hitting DB on
    // every relay request. The cache entry should include the fields needed by
    // `TokenInfo` plus the resolved `group` value, so we also avoid the extra
    // `user_quota` query when `group_code_override` is empty.
    //
    // Suggested follow-up when wiring Redis:
    // 1. Read Redis first with `token:auth:{key_hash}` and fall back to DB on miss.
    // 2. Use a short TTL plus explicit invalidation on token/quota/admin updates.
    // 3. Keep quota settlement / rate-limit runtime state in Redis too, instead of
    //    extending this DB read path.
    pub async fn validate(&self, raw_key: &str) -> ApiResult<TokenInfo> {
        use sha2::{Digest, Sha256};

        let key_hash = hex::encode(Sha256::digest(raw_key.as_bytes()));
        let tk = token::Entity::find()
            .filter(token::Column::KeyHash.eq(&key_hash))
            .one(&self.db)
            .await
            .context("failed to query token")
            .map_err(ApiErrors::Internal)?
            .ok_or_else(|| ApiErrors::Unauthorized("invalid API key".into()))?;

        if tk.status != TokenStatus::Enabled {
            return Err(ApiErrors::Unauthorized("token is not enabled".into()));
        }

        if let Some(expire_time) = &tk.expire_time
            && *expire_time < chrono::Utc::now().fixed_offset()
        {
            return Err(ApiErrors::Unauthorized("token has expired".into()));
        }

        if !tk.unlimited_quota && tk.remain_quota <= 0 {
            return Err(ApiErrors::Forbidden("quota exceeded".into()));
        }

        let allowed_models = json_string_array(&tk.models);
        let endpoint_scopes = json_string_array(&tk.endpoint_scopes);

        let group = if tk.group_code_override.is_empty() {
            let uq = user_quota::Entity::find()
                .filter(user_quota::Column::UserId.eq(tk.user_id))
                .one(&self.db)
                .await
                .context("failed to query user quota")
                .map_err(ApiErrors::Internal)?;
            uq.map(|q| q.channel_group)
                .unwrap_or_else(|| "default".to_string())
        } else {
            tk.group_code_override.clone()
        };

        Ok(TokenInfo {
            token_id: tk.id,
            user_id: tk.user_id,
            project_id: tk.project_id,
            service_account_id: tk.service_account_id,
            name: tk.name,
            group,
            remain_quota: tk.remain_quota,
            unlimited_quota: tk.unlimited_quota,
            rpm_limit: tk.rpm_limit,
            tpm_limit: tk.tpm_limit,
            concurrency_limit: tk.concurrency_limit,
            allowed_models,
            endpoint_scopes,
        })
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
}

fn json_string_array(value: &serde_json::Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|value| value.as_str().map(ToOwned::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::TokenInfo;

    fn token_info() -> TokenInfo {
        TokenInfo {
            token_id: 1,
            user_id: 2,
            project_id: 3,
            service_account_id: 4,
            name: "demo".into(),
            group: "default".into(),
            remain_quota: 100,
            unlimited_quota: false,
            rpm_limit: 0,
            tpm_limit: 0,
            concurrency_limit: 0,
            allowed_models: vec!["gpt-4o".into()],
            endpoint_scopes: vec!["chat".into()],
        }
    }

    #[test]
    fn ensure_model_allowed_respects_whitelist() {
        let token = token_info();
        assert!(token.ensure_model_allowed("gpt-4o").is_ok());
        assert!(token.ensure_model_allowed("gpt-4.1").is_err());
    }

    #[test]
    fn ensure_endpoint_allowed_respects_scope_list() {
        let token = token_info();
        assert!(token.ensure_endpoint_allowed("chat").is_ok());
        assert!(token.ensure_endpoint_allowed("embeddings").is_err());
    }
}
