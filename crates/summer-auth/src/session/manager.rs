use std::sync::Arc;

use crate::config::AuthConfig;
use crate::error::{AuthError, AuthResult};
use crate::session::model::{DeviceSession, UserProfile, UserSession};
use crate::storage::AuthStorage;
use crate::token::generator::TokenGenerator;
use crate::token::jwt::TokenType;
use crate::token::pair::TokenPair;
use crate::user_type::{DeviceType, LoginId};

// ── Redis key 生成 ──

fn access_key(token: &str) -> String {
    format!("auth:access:{token}")
}

fn refresh_key(token: &str) -> String {
    format!("auth:refresh:{token}")
}

fn blacklist_key(jti: &str) -> String {
    format!("auth:blacklist:{jti}")
}

/// 登录参数
#[derive(Debug, Clone)]
pub struct LoginParams {
    pub login_id: LoginId,
    pub device: DeviceType,
    pub login_ip: String,
    pub user_agent: String,
    pub profile: UserProfile,
}

#[derive(Clone)]
pub struct SessionManager {
    pub(crate) storage: Arc<dyn AuthStorage>,
    pub(crate) config: AuthConfig,
    pub(crate) token_gen: TokenGenerator,
}

impl SessionManager {
    pub fn new(storage: Arc<dyn AuthStorage>, config: AuthConfig) -> Self {
        let token_gen = TokenGenerator::new(&config);
        Self {
            storage,
            config,
            token_gen,
        }
    }

    pub fn config(&self) -> &AuthConfig {
        &self.config
    }

    // ── 内部辅助 ──

    /// 撤销一个设备会话的 token
    async fn revoke_device_tokens(&self, ds: &DeviceSession) {
        if self.token_gen.is_jwt() {
            // JWT 模式：将 JTI 加入黑名单
            if let Some(jti) = &ds.access_jti {
                if let Ok(claims) = self.token_gen.jwt().decode(&ds.access_token) {
                    let remaining = claims.exp - chrono::Local::now().timestamp();
                    if remaining > 0 {
                        let _ = self
                            .storage
                            .set_string(&blacklist_key(jti), "1", remaining)
                            .await;
                    }
                }
            }
            if let Some(jti) = &ds.refresh_jti {
                if let Ok(claims) = self.token_gen.jwt().decode(&ds.refresh_token) {
                    let remaining = claims.exp - chrono::Local::now().timestamp();
                    if remaining > 0 {
                        let _ = self
                            .storage
                            .set_string(&blacklist_key(jti), "1", remaining)
                            .await;
                    }
                }
            }
        } else {
            // UUID 模式：删除反查键
            let _ = self.storage.delete(&access_key(&ds.access_token)).await;
            let _ = self.storage.delete(&refresh_key(&ds.refresh_token)).await;
        }
    }

    /// UUID 模式：写入反查键
    async fn write_reverse_keys(
        &self,
        access_token: &str,
        refresh_token: &str,
        login_id: &LoginId,
    ) -> AuthResult<()> {
        self.storage
            .set_string(
                &access_key(access_token),
                &login_id.encode(),
                self.config.access_timeout,
            )
            .await
            .map_err(|e| AuthError::StorageError(e.to_string()))?;

        self.storage
            .set_string(
                &refresh_key(refresh_token),
                &login_id.encode(),
                self.config.refresh_timeout,
            )
            .await
            .map_err(|e| AuthError::StorageError(e.to_string()))?;

        Ok(())
    }

    async fn get_or_create_session(
        &self,
        login_id: &LoginId,
        profile: &UserProfile,
    ) -> AuthResult<UserSession> {
        self.storage
            .get_session(&login_id.session_key())
            .await
            .map_err(|e| AuthError::StorageError(e.to_string()))
            .map(|opt| {
                opt.unwrap_or_else(|| UserSession {
                    login_id: login_id.clone(),
                    devices: Vec::new(),
                    profile: profile.clone(),
                })
            })
    }

    async fn save_session(&self, session: &UserSession) -> AuthResult<()> {
        self.storage
            .set_session(
                &session.login_id.session_key(),
                session,
                self.config.refresh_timeout,
            )
            .await
            .map_err(|e| AuthError::StorageError(e.to_string()))
    }

    /// 处理设备策略（并发登录、最大设备数）
    async fn handle_device_policy(
        &self,
        session: &mut UserSession,
        device: &DeviceType,
        new_device_session: &DeviceSession,
    ) -> AuthResult<()> {
        if !self.config.concurrent_login {
            for ds in &session.devices {
                self.revoke_device_tokens(ds).await;
            }
            session.devices.clear();
        } else {
            if let Some(idx) = session.devices.iter().position(|d| &d.device == device) {
                let old = session.devices.remove(idx);
                self.revoke_device_tokens(&old).await;
            }
        }

        if self.config.max_devices > 0 && session.devices.len() >= self.config.max_devices {
            let oldest = session.devices.remove(0);
            self.revoke_device_tokens(&oldest).await;
        }

        session.devices.push(new_device_session.clone());
        Ok(())
    }

    // ── 登录 ──

    pub async fn login(&self, params: LoginParams) -> AuthResult<TokenPair> {
        let mut session = self
            .get_or_create_session(&params.login_id, &params.profile)
            .await?;
        session.profile = params.profile.clone();

        // share_token：复用已有 token
        if self.config.concurrent_login && self.config.share_token {
            if let Some(existing) = session.devices.iter().find(|d| d.device == params.device) {
                let pair = TokenPair {
                    access_token: existing.access_token.clone(),
                    refresh_token: existing.refresh_token.clone(),
                    expires_in: self.config.access_timeout,
                };
                self.save_session(&session).await?;
                return Ok(pair);
            }
        }

        // 统一生成 token 对
        let generated = self.token_gen.generate_pair(
            &params.login_id,
            self.config.access_timeout,
            self.config.refresh_timeout,
        )?;

        let now = chrono::Local::now().timestamp();
        let device_session = DeviceSession {
            device: params.device.clone(),
            access_token: generated.access.token.clone(),
            refresh_token: generated.refresh.token.clone(),
            login_time: now,
            last_active_time: now,
            login_ip: params.login_ip,
            user_agent: params.user_agent,
            access_jti: generated.access.jti,
            refresh_jti: generated.refresh.jti,
        };

        self.handle_device_policy(&mut session, &params.device, &device_session)
            .await?;
        self.save_session(&session).await?;

        // UUID 模式需要写反查键，JWT 不需要
        if !self.token_gen.is_jwt() {
            self.write_reverse_keys(
                &generated.access.token,
                &generated.refresh.token,
                &params.login_id,
            )
            .await?;
        }

        Ok(TokenPair {
            access_token: generated.access.token,
            refresh_token: generated.refresh.token,
            expires_in: self.config.access_timeout,
        })
    }

    // ── 登出 ──

    pub async fn logout(&self, login_id: &LoginId, device: &DeviceType) -> AuthResult<()> {
        let session_key = login_id.session_key();
        let mut session = self
            .storage
            .get_session(&session_key)
            .await
            .map_err(|e| AuthError::StorageError(e.to_string()))?
            .ok_or(AuthError::SessionNotFound)?;

        if let Some(idx) = session.devices.iter().position(|d| &d.device == device) {
            let removed = session.devices.remove(idx);
            self.revoke_device_tokens(&removed).await;
        }

        if session.devices.is_empty() {
            self.storage
                .delete(&session_key)
                .await
                .map_err(|e| AuthError::StorageError(e.to_string()))?;
        } else {
            self.storage
                .set_session(&session_key, &session, self.config.refresh_timeout)
                .await
                .map_err(|e| AuthError::StorageError(e.to_string()))?;
        }

        Ok(())
    }

    pub async fn logout_all(&self, login_id: &LoginId) -> AuthResult<()> {
        let session_key = login_id.session_key();
        if let Some(session) = self
            .storage
            .get_session(&session_key)
            .await
            .map_err(|e| AuthError::StorageError(e.to_string()))?
        {
            for ds in &session.devices {
                self.revoke_device_tokens(ds).await;
            }
        }
        self.storage
            .delete(&session_key)
            .await
            .map_err(|e| AuthError::StorageError(e.to_string()))?;
        Ok(())
    }

    // ── 刷新 ──

    pub async fn refresh(&self, refresh_token: &str) -> AuthResult<TokenPair> {
        // 解析 refresh token → 拿到 login_id + 定位设备
        let (login_id, device_idx, mut session) = if self.token_gen.is_jwt() {
            self.resolve_refresh_jwt(refresh_token).await?
        } else {
            self.resolve_refresh_opaque(refresh_token).await?
        };

        let session_key = login_id.session_key();

        // 撤销旧 access token
        if self.token_gen.is_jwt() {
            if let Some(old_jti) = &session.devices[device_idx].access_jti {
                let _ = self
                    .storage
                    .set_string(
                        &blacklist_key(old_jti),
                        "1",
                        self.config.access_timeout,
                    )
                    .await;
            }
        } else {
            let old_access = &session.devices[device_idx].access_token;
            let _ = self.storage.delete(&access_key(old_access)).await;
        }

        // 生成新 access token
        let new_access = self
            .token_gen
            .generate_access(&login_id, self.config.access_timeout)?;
        let now = chrono::Local::now().timestamp();

        session.devices[device_idx].access_token = new_access.token.clone();
        session.devices[device_idx].access_jti = new_access.jti;
        session.devices[device_idx].last_active_time = now;

        self.storage
            .set_session(&session_key, &session, self.config.refresh_timeout)
            .await
            .map_err(|e| AuthError::StorageError(e.to_string()))?;

        // UUID 模式需要写新的 access 反查键
        if !self.token_gen.is_jwt() {
            self.storage
                .set_string(
                    &access_key(&new_access.token),
                    &login_id.encode(),
                    self.config.access_timeout,
                )
                .await
                .map_err(|e| AuthError::StorageError(e.to_string()))?;
        }

        Ok(TokenPair {
            access_token: new_access.token,
            refresh_token: refresh_token.to_string(),
            expires_in: self.config.access_timeout,
        })
    }

    /// JWT 模式：解析 refresh token
    async fn resolve_refresh_jwt(
        &self,
        refresh_token: &str,
    ) -> AuthResult<(LoginId, usize, UserSession)> {
        let jwt = self.token_gen.jwt();
        let (login_id, claims) = jwt.extract_login_id(refresh_token).map_err(|e| match e {
            AuthError::TokenExpired | AuthError::InvalidToken => AuthError::InvalidRefreshToken,
            other => other,
        })?;

        if claims.typ != TokenType::Refresh {
            return Err(AuthError::InvalidRefreshToken);
        }

        // 黑名单检查
        let blacklisted = self
            .storage
            .exists(&blacklist_key(&claims.jti))
            .await
            .map_err(|e| AuthError::StorageError(e.to_string()))?;
        if blacklisted {
            return Err(AuthError::InvalidRefreshToken);
        }

        let session = self
            .storage
            .get_session(&login_id.session_key())
            .await
            .map_err(|e| AuthError::StorageError(e.to_string()))?
            .ok_or(AuthError::SessionNotFound)?;

        let idx = session
            .devices
            .iter()
            .position(|d| d.refresh_jti.as_deref() == Some(&claims.jti))
            .ok_or(AuthError::InvalidRefreshToken)?;

        Ok((login_id, idx, session))
    }

    /// UUID 模式：解析 refresh token
    async fn resolve_refresh_opaque(
        &self,
        refresh_token: &str,
    ) -> AuthResult<(LoginId, usize, UserSession)> {
        let value = self
            .storage
            .get_string(&refresh_key(refresh_token))
            .await
            .map_err(|e| AuthError::StorageError(e.to_string()))?
            .ok_or(AuthError::InvalidRefreshToken)?;

        let login_id = LoginId::decode(&value).ok_or(AuthError::InvalidRefreshToken)?;

        let session = self
            .storage
            .get_session(&login_id.session_key())
            .await
            .map_err(|e| AuthError::StorageError(e.to_string()))?
            .ok_or(AuthError::SessionNotFound)?;

        let idx = session
            .devices
            .iter()
            .position(|d| d.refresh_token == refresh_token)
            .ok_or(AuthError::InvalidRefreshToken)?;

        Ok((login_id, idx, session))
    }

    // ── Token 验证 ──

    pub async fn validate_token(&self, access_token: &str) -> AuthResult<LoginId> {
        if self.token_gen.is_jwt() {
            let jwt = self.token_gen.jwt();
            let (login_id, claims) = jwt.extract_login_id(access_token)?;

            if claims.typ != TokenType::Access {
                return Err(AuthError::InvalidToken);
            }

            let blacklisted = self
                .storage
                .exists(&blacklist_key(&claims.jti))
                .await
                .map_err(|e| AuthError::StorageError(e.to_string()))?;
            if blacklisted {
                return Err(AuthError::InvalidToken);
            }

            Ok(login_id)
        } else {
            let val = self
                .storage
                .get_string(&access_key(access_token))
                .await
                .map_err(|e| AuthError::StorageError(e.to_string()))?
                .ok_or(AuthError::InvalidToken)?;

            LoginId::decode(&val).ok_or(AuthError::InvalidToken)
        }
    }

    /// 自动续期（中间件调用）
    pub async fn renew_access_token(&self, access_token: &str, login_id: &LoginId) {
        // JWT 模式 exp 固定，无法续期
        if self.token_gen.is_jwt() {
            return;
        }

        let _ = self
            .storage
            .set_string(
                &access_key(access_token),
                &login_id.encode(),
                self.config.access_timeout,
            )
            .await;
    }

    pub async fn get_session(&self, login_id: &LoginId) -> AuthResult<Option<UserSession>> {
        self.storage
            .get_session(&login_id.session_key())
            .await
            .map_err(|e| AuthError::StorageError(e.to_string()))
    }

    // ── RBAC ──

    async fn require_session(&self, login_id: &LoginId) -> AuthResult<UserSession> {
        self.storage
            .get_session(&login_id.session_key())
            .await
            .map_err(|e| AuthError::StorageError(e.to_string()))?
            .ok_or(AuthError::SessionNotFound)
    }

    pub async fn set_roles(&self, login_id: &LoginId, roles: Vec<String>) -> AuthResult<()> {
        let mut session = self.require_session(login_id).await?;
        session.profile.set_roles(roles);
        self.save_session(&session).await
    }

    pub async fn set_permissions(
        &self,
        login_id: &LoginId,
        perms: Vec<String>,
    ) -> AuthResult<()> {
        let mut session = self.require_session(login_id).await?;
        session.profile.set_permissions(perms);
        self.save_session(&session).await
    }

    pub async fn has_role(&self, login_id: &LoginId, role: &str) -> AuthResult<bool> {
        let session = self.require_session(login_id).await?;
        Ok(session.profile.roles().iter().any(|r| r == role))
    }

    pub async fn has_permission(&self, login_id: &LoginId, perm: &str) -> AuthResult<bool> {
        let session = self.require_session(login_id).await?;
        Ok(session.profile.permissions().iter().any(|p| p == perm))
    }

    pub async fn check_role(&self, login_id: &LoginId, role: &str) -> AuthResult<()> {
        if self.has_role(login_id, role).await? {
            Ok(())
        } else {
            Err(AuthError::NoRole(role.to_string()))
        }
    }

    pub async fn check_permission(&self, login_id: &LoginId, perm: &str) -> AuthResult<()> {
        if self.has_permission(login_id, perm).await? {
            Ok(())
        } else {
            Err(AuthError::NoPermission(perm.to_string()))
        }
    }

    pub async fn kick_out(
        &self,
        login_id: &LoginId,
        device: Option<&DeviceType>,
    ) -> AuthResult<()> {
        match device {
            Some(d) => self.logout(login_id, d).await,
            None => self.logout_all(login_id).await,
        }
    }
}
