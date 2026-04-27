use std::sync::OnceLock;
use std::time::Duration;

use anyhow::Context;
use base64::Engine;
use base64::engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD};
use bigdecimal::BigDecimal;
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use serde::Deserialize;
use serde_json::{Value, json};
use summer::plugin::Service;
use summer_ai_core::oauth::SessionStore;
use summer_ai_core::oauth::openai::{
    OpenAiCredentialCodec, OpenAiOAuthError, OpenAiOAuthSession, OpenAiStoredCredentials,
    build_authorization_url, build_stored_extra_overlay, enrich_stored_credentials,
    exchange_code as exchange_openai_code, generate_code_challenge, generate_code_verifier,
    generate_session_id, generate_state, refresh_token as refresh_openai_token,
    should_skip_openai_privacy_ensure,
};
use summer_ai_model::dto::openai_oauth::{
    ExchangeOpenAiOAuthCodeDto, GenerateOpenAiOAuthAuthUrlDto, RefreshOpenAiOAuthTokenDto,
};
use summer_ai_model::entity::routing::{channel, channel_account};
use summer_ai_model::vo::openai_oauth::{
    OpenAiOAuthAuthUrlVo, OpenAiOAuthExchangeVo, OpenAiOAuthRefreshVo,
};
use summer_common::error::{ApiErrors, ApiResult};
use summer_redis::Redis;
use summer_redis::redis::AsyncCommands;
use summer_sea_orm::DbConn;

const OPENAI_OAUTH_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const OPENAI_OAUTH_SESSION_TTL: Duration = Duration::from_secs(30 * 60);

#[derive(Clone, Service)]
pub struct OpenAiOAuthService {
    #[inject(component)]
    db: DbConn,
    #[inject(component)]
    redis: Redis,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAiIdTokenProfile {
    pub email: Option<String>,
    pub chatgpt_account_id: Option<String>,
    pub chatgpt_user_id: Option<String>,
    pub organization_id: Option<String>,
    pub plan_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OpenAiOAuthAccountPayload {
    pub credential_type: String,
    pub credentials: Value,
    pub extra: Value,
    pub expires_at: Option<sea_orm::prelude::DateTimeWithTimeZone>,
}

enum ExchangeTarget {
    Create {
        channel: channel::Model,
        name: String,
    },
    Update {
        channel: channel::Model,
        account: channel_account::Model,
        name: String,
    },
}

impl OpenAiOAuthService {
    pub async fn generate_auth_url(
        &self,
        dto: GenerateOpenAiOAuthAuthUrlDto,
    ) -> ApiResult<OpenAiOAuthAuthUrlVo> {
        let session_id = generate_session_id();
        let state = generate_state();
        let code_verifier = generate_code_verifier();
        let code_challenge = generate_code_challenge(&code_verifier);
        let auth_url = build_authorization_url(
            &state,
            &code_challenge,
            &dto.redirect_uri,
            OPENAI_OAUTH_CLIENT_ID,
        )
        .context("构造 OpenAI OAuth 授权地址失败")?;

        openai_session_store()
            .set(
                session_id.clone(),
                OpenAiOAuthSession {
                    state,
                    code_verifier,
                    client_id: OPENAI_OAUTH_CLIENT_ID.to_string(),
                    redirect_uri: dto.redirect_uri,
                    created_at: Utc::now(),
                },
            )
            .await;

        Ok(OpenAiOAuthAuthUrlVo {
            auth_url: auth_url.to_string(),
            session_id,
        })
    }

    pub async fn exchange_code(
        &self,
        dto: ExchangeOpenAiOAuthCodeDto,
        operator: &str,
    ) -> ApiResult<OpenAiOAuthExchangeVo> {
        dto.validate_target().map_err(ApiErrors::BadRequest)?;

        let session = openai_session_store()
            .get(&dto.session_id)
            .await
            .ok_or_else(|| ApiErrors::BadRequest("OpenAI OAuth 会话不存在或已过期".to_string()))?;
        if session.state != dto.state {
            return Err(ApiErrors::BadRequest(
                "OpenAI OAuth state 校验失败，请重新开始授权".to_string(),
            ));
        }

        let target = self.resolve_exchange_target(&dto).await?;
        let exchanged_at = Utc::now();
        let response = exchange_openai_code(
            openai_http_client(),
            &dto.code,
            &session.code_verifier,
            &session.redirect_uri,
            &session.client_id,
        )
        .await
        .map_err(|err| map_openai_oauth_error(err, "交换授权码"))?;
        openai_session_store().remove(&dto.session_id).await;

        let mut credentials = OpenAiStoredCredentials::from_exchange_response(
            response,
            session.client_id,
            exchanged_at,
        )
        .map_err(|err| {
            ApiErrors::ServiceUnavailable(format!("OpenAI OAuth 授权结果不完整: {err}"))
        })?;

        let (account_id, created, expires_at, subscription_expires_at) = match target {
            ExchangeTarget::Create { channel, name } => {
                hydrate_openai_credentials(&mut credentials)?;
                let enrichment =
                    enrich_stored_credentials(openai_http_client(), &mut credentials, false).await;
                credentials.token_version = Some(next_token_version(None));
                let account = self
                    .create_openai_account(
                        &channel,
                        &name,
                        &dto,
                        credentials,
                        enrichment.privacy_mode.as_deref(),
                        operator,
                    )
                    .await?;
                let stored = decode_account_credentials(&account)?;
                (
                    account.id,
                    true,
                    stored.expires_at.fixed_offset(),
                    stored.subscription_expires_at.map(|ts| ts.fixed_offset()),
                )
            }
            ExchangeTarget::Update {
                channel,
                account,
                name,
            } => {
                let existing = decode_account_credentials(&account)?;
                credentials.extra = existing.extra.clone();
                credentials.token_version = Some(next_token_version(existing.token_version));
                hydrate_openai_credentials(&mut credentials)?;
                let enrichment = enrich_stored_credentials(
                    openai_http_client(),
                    &mut credentials,
                    should_skip_openai_privacy_ensure(&account.extra),
                )
                .await;
                let account = self
                    .update_openai_account(
                        &channel,
                        account,
                        &name,
                        &dto,
                        credentials,
                        enrichment.privacy_mode.as_deref(),
                        operator,
                    )
                    .await?;
                let stored = decode_account_credentials(&account)?;
                (
                    account.id,
                    false,
                    stored.expires_at.fixed_offset(),
                    stored.subscription_expires_at.map(|ts| ts.fixed_offset()),
                )
            }
        };

        Ok(OpenAiOAuthExchangeVo {
            account_id,
            created,
            expires_at,
            subscription_expires_at,
        })
    }

    pub async fn refresh_token(
        &self,
        dto: RefreshOpenAiOAuthTokenDto,
        operator: &str,
    ) -> ApiResult<OpenAiOAuthRefreshVo> {
        let account = self.find_openai_oauth_account(dto.account_id).await?;
        let current = decode_account_credentials(&account)?;
        let refreshed_at = Utc::now();
        let response = refresh_openai_token(
            openai_http_client(),
            &current.client_id,
            &current.refresh_token,
        )
        .await
        .map_err(|err| map_openai_oauth_error(err, "刷新令牌"))?;

        let mut refreshed = current
            .merge_refresh_response(response, refreshed_at)
            .map_err(|err| {
                ApiErrors::ServiceUnavailable(format!("OpenAI OAuth 刷新结果不完整: {err}"))
            })?;
        refreshed.token_version = Some(next_token_version(current.token_version));
        hydrate_openai_credentials(&mut refreshed)?;
        let enrichment = enrich_stored_credentials(
            openai_http_client(),
            &mut refreshed,
            should_skip_openai_privacy_ensure(&account.extra),
        )
        .await;

        let account = self
            .persist_refreshed_account(
                account,
                refreshed.clone(),
                enrichment.privacy_mode.as_deref(),
                operator,
            )
            .await?;

        Ok(OpenAiOAuthRefreshVo {
            account_id: account.id,
            refreshed_at: refreshed_at.fixed_offset(),
            expires_at: refreshed.expires_at.fixed_offset(),
            subscription_expires_at: refreshed
                .subscription_expires_at
                .map(|ts| ts.fixed_offset()),
        })
    }

    async fn resolve_exchange_target(
        &self,
        dto: &ExchangeOpenAiOAuthCodeDto,
    ) -> ApiResult<ExchangeTarget> {
        if let Some(channel_id) = dto.channel_id {
            let channel = self.find_channel_by_id(channel_id).await?;
            ensure_openai_channel_type(channel.channel_type).map_err(ApiErrors::BadRequest)?;
            let name = required_trimmed(dto.name.as_deref(), "账号名称")?;
            self.ensure_account_name_available(channel.id, &name, None)
                .await?;
            return Ok(ExchangeTarget::Create { channel, name });
        }

        let account_id = dto
            .account_id
            .ok_or_else(|| ApiErrors::BadRequest("缺少 accountId".to_string()))?;
        let account = self.find_account_by_id(account_id).await?;
        if !account.is_oauth() {
            return Err(ApiErrors::BadRequest(format!(
                "渠道账号不是 OAuth 类型: id={account_id}"
            )));
        }

        let channel = self.find_channel_by_id(account.channel_id).await?;
        ensure_openai_channel_type(channel.channel_type).map_err(ApiErrors::BadRequest)?;
        let name = dto
            .name
            .as_deref()
            .map(|value| required_trimmed(Some(value), "账号名称"))
            .transpose()?
            .unwrap_or_else(|| account.name.clone());
        self.ensure_account_name_available(channel.id, &name, Some(account.id))
            .await?;

        Ok(ExchangeTarget::Update {
            channel,
            account,
            name,
        })
    }

    async fn create_openai_account(
        &self,
        channel: &channel::Model,
        name: &str,
        dto: &ExchangeOpenAiOAuthCodeDto,
        credentials: OpenAiStoredCredentials,
        privacy_mode: Option<&str>,
        operator: &str,
    ) -> ApiResult<channel_account::Model> {
        let payload = build_oauth_account_payload_with_privacy(&credentials, privacy_mode);
        let active = channel_account::ActiveModel {
            channel_id: Set(channel.id),
            name: Set(name.to_string()),
            credential_type: Set(payload.credential_type),
            credentials: Set(payload.credentials),
            secret_ref: Set(String::new()),
            status: Set(channel_account::ChannelAccountStatus::Enabled),
            schedulable: Set(true),
            priority: Set(0),
            weight: Set(1),
            rate_multiplier: Set(BigDecimal::from(1)),
            concurrency_limit: Set(0),
            quota_limit: Set(BigDecimal::from(0)),
            quota_used: Set(BigDecimal::from(0)),
            balance: Set(BigDecimal::from(0)),
            balance_updated_at: Set(None),
            response_time: Set(0),
            failure_streak: Set(0),
            last_used_at: Set(None),
            last_error_at: Set(None),
            last_error_code: Set(String::new()),
            last_error_message: Set(String::new()),
            rate_limited_until: Set(None),
            overload_until: Set(None),
            expires_at: Set(payload.expires_at),
            test_model: Set(resolve_test_model(
                dto.test_model.as_deref(),
                &channel.test_model,
            )),
            test_time: Set(None),
            extra: Set(payload.extra),
            deleted_at: Set(None),
            remark: Set(dto.remark.clone().unwrap_or_default()),
            create_by: Set(operator.to_string()),
            update_by: Set(operator.to_string()),
            disabled_api_keys: Set(json!([])),
            ..Default::default()
        };

        let model = active
            .insert(&self.db)
            .await
            .context("创建 OpenAI OAuth 渠道账号失败")
            .map_err(ApiErrors::from)?;
        self.invalidate_runtime_channel_account_ids(model.channel_id)
            .await?;
        self.invalidate_runtime_account_cache(model.id).await?;
        Ok(model)
    }

    async fn update_openai_account(
        &self,
        _channel: &channel::Model,
        account: channel_account::Model,
        name: &str,
        dto: &ExchangeOpenAiOAuthCodeDto,
        credentials: OpenAiStoredCredentials,
        privacy_mode: Option<&str>,
        operator: &str,
    ) -> ApiResult<channel_account::Model> {
        let payload = build_oauth_account_payload_with_privacy(&credentials, privacy_mode);
        let mut active: channel_account::ActiveModel = account.clone().into();
        active.name = Set(name.to_string());
        active.credential_type = Set(payload.credential_type);
        active.credentials = Set(payload.credentials);
        active.expires_at = Set(payload.expires_at);
        active.extra = Set(merge_account_extra(&account.extra, &payload.extra));
        active.test_model = Set(resolve_test_model(
            dto.test_model.as_deref(),
            &account.test_model,
        ));
        if let Some(remark) = &dto.remark {
            active.remark = Set(remark.clone());
        }
        active.last_error_at = Set(None);
        active.last_error_code = Set(String::new());
        active.last_error_message = Set(String::new());
        active.update_by = Set(operator.to_string());

        let model = active
            .update(&self.db)
            .await
            .context("更新 OpenAI OAuth 渠道账号失败")
            .map_err(ApiErrors::from)?;
        self.invalidate_runtime_account_cache(model.id).await?;
        Ok(model)
    }

    async fn persist_refreshed_account(
        &self,
        account: channel_account::Model,
        credentials: OpenAiStoredCredentials,
        privacy_mode: Option<&str>,
        operator: &str,
    ) -> ApiResult<channel_account::Model> {
        let payload = build_oauth_account_payload_with_privacy(&credentials, privacy_mode);
        let mut active: channel_account::ActiveModel = account.clone().into();
        active.credential_type = Set(payload.credential_type);
        active.credentials = Set(payload.credentials);
        active.expires_at = Set(payload.expires_at);
        active.extra = Set(merge_account_extra(&account.extra, &payload.extra));
        active.last_error_at = Set(None);
        active.last_error_code = Set(String::new());
        active.last_error_message = Set(String::new());
        active.update_by = Set(operator.to_string());

        let model = active
            .update(&self.db)
            .await
            .context("刷新 OpenAI OAuth 渠道账号失败")
            .map_err(ApiErrors::from)?;
        self.invalidate_runtime_account_cache(model.id).await?;
        Ok(model)
    }

    async fn ensure_account_name_available(
        &self,
        channel_id: i64,
        name: &str,
        exclude_account_id: Option<i64>,
    ) -> ApiResult<()> {
        let mut query = channel_account::Entity::find()
            .filter(channel_account::Column::DeletedAt.is_null())
            .filter(channel_account::Column::ChannelId.eq(channel_id))
            .filter(channel_account::Column::Name.eq(name.to_string()));
        if let Some(account_id) = exclude_account_id {
            query = query.filter(channel_account::Column::Id.ne(account_id));
        }

        let exists = query
            .one(&self.db)
            .await
            .context("检查 OpenAI OAuth 渠道账号名称唯一性失败")?;
        if exists.is_some() {
            return Err(ApiErrors::Conflict(format!(
                "渠道账号名称已存在: channel_id={channel_id}, name={name}"
            )));
        }
        Ok(())
    }

    async fn find_channel_by_id(&self, id: i64) -> ApiResult<channel::Model> {
        channel::Entity::find_by_id(id)
            .filter(channel::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .context("查询 OpenAI OAuth 渠道失败")?
            .ok_or_else(|| ApiErrors::NotFound(format!("渠道不存在: id={id}")))
    }

    async fn find_account_by_id(&self, id: i64) -> ApiResult<channel_account::Model> {
        channel_account::Entity::find_by_id(id)
            .filter(channel_account::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .context("查询 OpenAI OAuth 渠道账号失败")?
            .ok_or_else(|| ApiErrors::NotFound(format!("渠道账号不存在: id={id}")))
    }

    async fn find_openai_oauth_account(&self, id: i64) -> ApiResult<channel_account::Model> {
        let account = self.find_account_by_id(id).await?;
        if !account.is_oauth() {
            return Err(ApiErrors::BadRequest(format!(
                "渠道账号不是 OAuth 类型: id={id}"
            )));
        }
        let channel = self.find_channel_by_id(account.channel_id).await?;
        ensure_openai_channel_type(channel.channel_type).map_err(ApiErrors::BadRequest)?;
        Ok(account)
    }

    async fn invalidate_runtime_account_cache(&self, account_id: i64) -> ApiResult<()> {
        let mut conn = self.redis.clone();
        conn.del::<_, ()>(runtime_account_key(account_id))
            .await
            .map_err(|err| {
                ApiErrors::ServiceUnavailable(format!("刷新 Relay 账号缓存失败: {err}"))
            })?;
        Ok(())
    }

    async fn invalidate_runtime_channel_account_ids(&self, channel_id: i64) -> ApiResult<()> {
        let mut conn = self.redis.clone();
        conn.del::<_, ()>(runtime_channel_account_ids_key(channel_id))
            .await
            .map_err(|err| {
                ApiErrors::ServiceUnavailable(format!("刷新 Relay 账号列表缓存失败: {err}"))
            })?;
        Ok(())
    }
}

pub fn ensure_openai_channel_type(channel_type: channel::ChannelType) -> Result<(), String> {
    if channel_type == channel::ChannelType::OpenAi {
        return Ok(());
    }
    Err(format!(
        "当前渠道类型不支持 OpenAI OAuth 托管账号: {channel_type:?}"
    ))
}

pub fn decode_openai_id_token_profile(id_token: &str) -> Result<OpenAiIdTokenProfile, String> {
    let payload = id_token
        .split('.')
        .nth(1)
        .ok_or_else(|| "OpenAI id_token 不是合法 JWT".to_string())?;
    let decoded = decode_base64url(payload)
        .map_err(|err| format!("OpenAI id_token payload 解码失败: {err}"))?;
    let claims = serde_json::from_slice::<OpenAiIdTokenClaims>(&decoded)
        .map_err(|err| format!("OpenAI id_token claims 解析失败: {err}"))?;

    Ok(OpenAiIdTokenProfile {
        email: claims.email.and_then(non_empty_owned),
        chatgpt_account_id: claims
            .openai_auth
            .as_ref()
            .and_then(|auth| auth.chatgpt_account_id.clone())
            .and_then(non_empty_owned),
        chatgpt_user_id: claims
            .openai_auth
            .as_ref()
            .and_then(|auth| auth.chatgpt_user_id.clone())
            .and_then(non_empty_owned),
        organization_id: claims
            .openai_auth
            .as_ref()
            .and_then(resolve_organization_id),
        plan_type: claims
            .openai_auth
            .as_ref()
            .and_then(|auth| auth.chatgpt_plan_type.clone())
            .and_then(non_empty_owned),
    })
}

pub fn build_oauth_account_payload(
    credentials: &OpenAiStoredCredentials,
) -> OpenAiOAuthAccountPayload {
    build_oauth_account_payload_with_privacy(credentials, None)
}

pub fn build_oauth_account_payload_with_privacy(
    credentials: &OpenAiStoredCredentials,
    privacy_mode: Option<&str>,
) -> OpenAiOAuthAccountPayload {
    let codec = OpenAiCredentialCodec;
    let extra = build_stored_extra_overlay(credentials, privacy_mode);

    OpenAiOAuthAccountPayload {
        credential_type: "oauth".to_string(),
        credentials: codec.encode_stored(credentials),
        extra: Value::Object(extra),
        expires_at: Some(credentials.expires_at.fixed_offset()),
    }
}

fn openai_session_store() -> &'static SessionStore<OpenAiOAuthSession> {
    static STORE: OnceLock<SessionStore<OpenAiOAuthSession>> = OnceLock::new();
    STORE.get_or_init(|| SessionStore::new(OPENAI_OAUTH_SESSION_TTL))
}

fn openai_http_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(reqwest::Client::new)
}

fn map_openai_oauth_error(err: OpenAiOAuthError, action: &str) -> ApiErrors {
    match err {
        OpenAiOAuthError::Transport(source) => {
            ApiErrors::ServiceUnavailable(format!("OpenAI OAuth {action}请求失败: {source}"))
        }
        OpenAiOAuthError::Decode { context, .. } => {
            ApiErrors::ServiceUnavailable(format!("OpenAI OAuth {action}响应解析失败: {context}"))
        }
        OpenAiOAuthError::HttpStatus {
            status,
            provider_error,
            ..
        } => {
            let message = provider_error
                .as_ref()
                .and_then(|item| {
                    item.error_description
                        .as_deref()
                        .or(item.error.as_deref())
                        .map(str::to_string)
                })
                .unwrap_or_else(|| format!("status={status}"));
            if status.is_client_error() {
                ApiErrors::BadRequest(format!("OpenAI OAuth {action}失败: {message}"))
            } else {
                ApiErrors::ServiceUnavailable(format!("OpenAI OAuth {action}暂时不可用: {message}"))
            }
        }
    }
}

fn hydrate_openai_credentials(credentials: &mut OpenAiStoredCredentials) -> ApiResult<()> {
    let profile = match decode_openai_id_token_profile(&credentials.id_token) {
        Ok(profile) => profile,
        Err(err) => {
            tracing::warn!(error = %err, "openai id_token profile decode failed");
            return Ok(());
        }
    };

    merge_optional_field(&mut credentials.email, profile.email);
    merge_optional_field(
        &mut credentials.chatgpt_account_id,
        profile.chatgpt_account_id,
    );
    merge_optional_field(&mut credentials.chatgpt_user_id, profile.chatgpt_user_id);
    merge_optional_field(&mut credentials.organization_id, profile.organization_id);
    merge_optional_field(&mut credentials.plan_type, profile.plan_type);
    Ok(())
}

fn decode_account_credentials(
    account: &channel_account::Model,
) -> ApiResult<OpenAiStoredCredentials> {
    let raw = account
        .credentials
        .get("oauth")
        .unwrap_or(&account.credentials);
    OpenAiCredentialCodec
        .decode(raw)
        .map_err(|err| ApiErrors::BadRequest(format!("渠道账号 OAuth 凭证格式不正确: {err}")))
}

fn required_trimmed(value: Option<&str>, field: &str) -> ApiResult<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| ApiErrors::BadRequest(format!("{field}不能为空")))
}

fn resolve_test_model(value: Option<&str>, fallback: &str) -> String {
    value
        .map(str::trim)
        .map(str::to_string)
        .unwrap_or_else(|| fallback.to_string())
}

fn merge_account_extra(existing: &Value, overlay: &Value) -> Value {
    let mut merged = existing.as_object().cloned().unwrap_or_default();
    if let Some(overlay_object) = overlay.as_object() {
        for (key, value) in overlay_object {
            merged.insert(key.clone(), value.clone());
        }
    }
    Value::Object(merged)
}

fn merge_optional_field(slot: &mut Option<String>, value: Option<String>) {
    if let Some(value) = value {
        *slot = Some(value);
    }
}

fn next_token_version(current: Option<i64>) -> i64 {
    current.unwrap_or(0) + 1
}

fn runtime_channel_account_ids_key(channel_id: i64) -> String {
    format!("ai:ch:ac:{channel_id}")
}

fn runtime_account_key(account_id: i64) -> String {
    format!("ai:ch:a:{account_id}")
}

fn resolve_organization_id(auth: &OpenAiIdTokenAuthClaims) -> Option<String> {
    auth.organizations
        .iter()
        .find(|org| org.is_default)
        .and_then(|org| org.id.clone())
        .and_then(non_empty_owned)
        .or_else(|| {
            auth.organizations
                .iter()
                .find_map(|org| org.id.clone())
                .and_then(non_empty_owned)
        })
        .or_else(|| auth.poid.clone().and_then(non_empty_owned))
}

fn decode_base64url(input: &str) -> Result<Vec<u8>, base64::DecodeError> {
    URL_SAFE_NO_PAD
        .decode(input)
        .or_else(|_| URL_SAFE.decode(input))
}

fn non_empty_owned(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[derive(Debug, Deserialize)]
struct OpenAiIdTokenClaims {
    #[serde(default)]
    email: Option<String>,
    #[serde(default, rename = "https://api.openai.com/auth")]
    openai_auth: Option<OpenAiIdTokenAuthClaims>,
}

#[derive(Debug, Deserialize)]
struct OpenAiIdTokenAuthClaims {
    #[serde(default)]
    chatgpt_account_id: Option<String>,
    #[serde(default)]
    chatgpt_user_id: Option<String>,
    #[serde(default)]
    chatgpt_plan_type: Option<String>,
    #[serde(default)]
    poid: Option<String>,
    #[serde(default)]
    organizations: Vec<OpenAiIdTokenOrganizationClaim>,
}

#[derive(Debug, Deserialize)]
struct OpenAiIdTokenOrganizationClaim {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    is_default: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;

    #[test]
    fn hydrate_openai_credentials_preserves_existing_fields_when_claims_are_missing() {
        let mut credentials = OpenAiStoredCredentials {
            access_token: "at".into(),
            refresh_token: "rt".into(),
            id_token: fake_jwt(serde_json::json!({
                "https://api.openai.com/auth": {
                    "organizations": []
                }
            })),
            expires_at: Utc::now(),
            client_id: "app_test".into(),
            email: Some("kept@example.com".into()),
            chatgpt_account_id: Some("acc_1".into()),
            chatgpt_user_id: Some("user_1".into()),
            organization_id: Some("org_1".into()),
            plan_type: Some("plus".into()),
            subscription_expires_at: None,
            token_version: Some(1),
            extra: serde_json::Map::new(),
        };

        hydrate_openai_credentials(&mut credentials).expect("hydrate");

        assert_eq!(credentials.email.as_deref(), Some("kept@example.com"));
        assert_eq!(credentials.chatgpt_account_id.as_deref(), Some("acc_1"));
        assert_eq!(credentials.chatgpt_user_id.as_deref(), Some("user_1"));
        assert_eq!(credentials.organization_id.as_deref(), Some("org_1"));
        assert_eq!(credentials.plan_type.as_deref(), Some("plus"));
    }

    #[test]
    fn hydrate_openai_credentials_treats_invalid_id_token_as_best_effort() {
        let mut credentials = OpenAiStoredCredentials {
            access_token: "at".into(),
            refresh_token: "rt".into(),
            id_token: "not-a-jwt".into(),
            expires_at: Utc::now(),
            client_id: "app_test".into(),
            email: Some("kept@example.com".into()),
            chatgpt_account_id: None,
            chatgpt_user_id: None,
            organization_id: None,
            plan_type: Some("plus".into()),
            subscription_expires_at: None,
            token_version: Some(1),
            extra: serde_json::Map::new(),
        };

        hydrate_openai_credentials(&mut credentials).expect("hydrate");

        assert_eq!(credentials.email.as_deref(), Some("kept@example.com"));
        assert_eq!(credentials.plan_type.as_deref(), Some("plus"));
    }

    #[test]
    fn build_oauth_account_payload_with_privacy_sets_privacy_mode_overlay() {
        let credentials = OpenAiStoredCredentials {
            access_token: "at".into(),
            refresh_token: "rt".into(),
            id_token: "id".into(),
            expires_at: Utc::now(),
            client_id: "app_test".into(),
            email: Some("user@example.com".into()),
            chatgpt_account_id: None,
            chatgpt_user_id: None,
            organization_id: Some("org_1".into()),
            plan_type: Some("plus".into()),
            subscription_expires_at: None,
            token_version: Some(1),
            extra: serde_json::Map::new(),
        };

        let payload = build_oauth_account_payload_with_privacy(&credentials, Some("training_off"));
        assert_eq!(payload.extra["oauth_provider"], "openai");
        assert_eq!(payload.extra["oauth_email"], "user@example.com");
        assert_eq!(payload.extra["oauth_plan_type"], "plus");
        assert_eq!(payload.extra["oauth_organization_id"], "org_1");
        assert_eq!(payload.extra["privacy_mode"], "training_off");
    }

    fn fake_jwt(payload: serde_json::Value) -> String {
        let header = URL_SAFE_NO_PAD.encode(r#"{"alg":"none","typ":"JWT"}"#);
        let payload = URL_SAFE_NO_PAD.encode(payload.to_string());
        format!("{header}.{payload}.signature")
    }
}
