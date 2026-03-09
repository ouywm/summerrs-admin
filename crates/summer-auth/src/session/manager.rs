use std::sync::Arc;

use crate::config::AuthConfig;
use crate::error::{AuthError, AuthResult};
use crate::session::model::{DeviceSession, UserProfile, UserSession};
use crate::storage::AuthStorage;
use crate::token::{TokenGenerator, TokenPair, TokenType};
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

// ── 权限通配符匹配 ──

/// 判断用户持有的权限 `owned` 是否匹配请求的权限 `required`
///
/// 规则（冒号分隔的多段权限码）：
/// - 精确匹配：`system:user:list` 匹配 `system:user:list`
/// - 单个 `*` 匹配所有权限
/// - 末尾 `*` 通配：`system:*` 匹配 `system:user:list`、`system:role:add` 等所有 `system:` 开头的权限
/// - 中间段 `*`：`system:*:list` 匹配 `system:user:list`、`system:role:list`
/// - 段数不匹配时不通配（除非 owned 末尾是 `*`，此时匹配后续所有段）
pub fn permission_matches(owned: &str, required: &str) -> bool {
    // 完全相等，快速返回
    if owned == required {
        return true;
    }

    // 超级权限
    if owned == "*" {
        return true;
    }

    let owned_parts: Vec<&str> = owned.split(':').collect();
    let required_parts: Vec<&str> = required.split(':').collect();

    // 逐段比较
    for (i, owned_seg) in owned_parts.iter().enumerate() {
        if *owned_seg == "*" {
            // 如果 * 是最后一段，匹配 required 剩余所有段
            if i == owned_parts.len() - 1 {
                return true;
            }
            // 中间段 * 匹配 required 对应段的任意值，继续比较后续段
            if i >= required_parts.len() {
                return false;
            }
            continue;
        }

        // 非通配段，必须精确匹配
        if i >= required_parts.len() || *owned_seg != required_parts[i] {
            return false;
        }
    }

    // owned 段数 <= required 段数，且所有 owned 段都匹配了
    // 只有段数完全相等时才算匹配（末尾 * 的情况已在循环内 return）
    owned_parts.len() == required_parts.len()
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

    /// 计算 JWT token 的剩余存活秒数（decode 失败或已过期返回 0）
    fn jwt_remaining_seconds(&self, token: &str) -> i64 {
        self.token_gen
            .jwt()
            .decode(token)
            .map(|claims| (claims.exp - chrono::Local::now().timestamp()).max(0))
            .unwrap_or(0)
    }

    /// 撤销一个设备会话的 token
    async fn revoke_device_tokens(&self, ds: &DeviceSession) {
        if self.token_gen.is_jwt() {
            // JWT 模式：将 JTI 加入黑名单
            if let Some(jti) = &ds.access_jti {
                let remaining = self.jwt_remaining_seconds(&ds.access_token);
                if remaining > 0 {
                    let _ = self
                        .storage
                        .set_string(&blacklist_key(jti), "1", remaining)
                        .await;
                }
            }
            if let Some(jti) = &ds.refresh_jti {
                let remaining = self.jwt_remaining_seconds(&ds.refresh_token);
                if remaining > 0 {
                    let _ = self
                        .storage
                        .set_string(&blacklist_key(jti), "1", remaining)
                        .await;
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
            .await?;

        self.storage
            .set_string(
                &refresh_key(refresh_token),
                &login_id.encode(),
                self.config.refresh_timeout,
            )
            .await?;

        Ok(())
    }

    async fn get_or_create_session(
        &self,
        login_id: &LoginId,
        profile: &UserProfile,
    ) -> AuthResult<UserSession> {
        let opt = self.storage.get_session(&login_id.session_key()).await?;
        Ok(opt.unwrap_or_else(|| UserSession {
            login_id: login_id.clone(),
            devices: Vec::new(),
            profile: profile.clone(),
        }))
    }

    async fn save_session(&self, session: &UserSession) -> AuthResult<()> {
        self.storage
            .set_session(
                &session.login_id.session_key(),
                session,
                self.config.refresh_timeout,
            )
            .await?;
        Ok(())
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
                // JWT 模式：检查已有 access token 是否仍然有效
                let should_reuse = if self.token_gen.is_jwt() {
                    self.token_gen.jwt().decode(&existing.access_token).is_ok()
                } else {
                    true
                };

                if should_reuse {
                    let pair = TokenPair {
                        access_token: existing.access_token.clone(),
                        refresh_token: existing.refresh_token.clone(),
                        expires_in: self.config.access_timeout,
                    };
                    self.save_session(&session).await?;
                    return Ok(pair);
                }
                // JWT 已过期 → 继续往下生成新 token
            }
        }

        // TODO: refresh_token 应始终使用不透明格式（如 "refresh_{timestamp}_{login_id}_{uuid}"），
        //       而非跟随 token_style 使用 JWT。JWT refresh_token 存在泄露 payload 和无法真正撤销的风险。
        //       参考 sa-token-rust 的设计：refresh token 固定为 opaque 格式。
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
            .await?
            .ok_or(AuthError::SessionNotFound)?;

        if let Some(idx) = session.devices.iter().position(|d| &d.device == device) {
            let removed = session.devices.remove(idx);
            self.revoke_device_tokens(&removed).await;
        }

        if session.devices.is_empty() {
            self.storage.delete(&session_key).await?;
        } else {
            self.storage
                .set_session(&session_key, &session, self.config.refresh_timeout)
                .await?;
        }

        Ok(())
    }

    pub async fn logout_all(&self, login_id: &LoginId) -> AuthResult<()> {
        let session_key = login_id.session_key();
        if let Some(session) = self.storage.get_session(&session_key).await? {
            for ds in &session.devices {
                self.revoke_device_tokens(ds).await;
            }
        }
        self.storage.delete(&session_key).await?;
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
                let remaining = self.jwt_remaining_seconds(&session.devices[device_idx].access_token);
                if remaining > 0 {
                    let _ = self
                        .storage
                        .set_string(&blacklist_key(old_jti), "1", remaining)
                        .await;
                }
            }
        } else {
            let old_access = &session.devices[device_idx].access_token;
            let _ = self.storage.delete(&access_key(old_access)).await;
        }

        // 撤销旧 refresh token（轮转：旧的立即失效）
        if self.token_gen.is_jwt() {
            if let Some(old_jti) = &session.devices[device_idx].refresh_jti {
                let remaining = self.jwt_remaining_seconds(&session.devices[device_idx].refresh_token);
                if remaining > 0 {
                    let _ = self
                        .storage
                        .set_string(&blacklist_key(old_jti), "1", remaining)
                        .await;
                }
            }
        } else {
            let _ = self.storage.delete(&refresh_key(refresh_token)).await;
        }

        // TODO: 同 login()，refresh_token 应始终使用不透明格式，
        //       当前跟随 token_style 导致 JWT 模式下 refresh_token 也是 JWT。
        let generated = self.token_gen.generate_pair(
            &login_id,
            self.config.access_timeout,
            self.config.refresh_timeout,
        )?;
        let now = chrono::Local::now().timestamp();

        session.devices[device_idx].access_token = generated.access.token.clone();
        session.devices[device_idx].access_jti = generated.access.jti;
        session.devices[device_idx].refresh_token = generated.refresh.token.clone();
        session.devices[device_idx].refresh_jti = generated.refresh.jti;
        session.devices[device_idx].last_active_time = now;

        self.storage
            .set_session(&session_key, &session, self.config.refresh_timeout)
            .await?;

        // UUID 模式需要写新的反查键
        if !self.token_gen.is_jwt() {
            self.write_reverse_keys(
                &generated.access.token,
                &generated.refresh.token,
                &login_id,
            )
            .await?;
        }

        Ok(TokenPair {
            access_token: generated.access.token,
            refresh_token: generated.refresh.token,
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
        let blacklisted = self.storage.exists(&blacklist_key(&claims.jti)).await?;
        if blacklisted {
            return Err(AuthError::InvalidRefreshToken);
        }

        let session = self
            .storage
            .get_session(&login_id.session_key())
            .await?
            .ok_or(AuthError::SessionNotFound)?;

        let idx = session
            .devices
            .iter()
            .position(|d| d.refresh_jti.as_deref() == Some(claims.jti.as_str()))
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
            .await?
            .ok_or(AuthError::InvalidRefreshToken)?;

        let login_id = LoginId::decode(&value).ok_or(AuthError::InvalidRefreshToken)?;

        let session = self
            .storage
            .get_session(&login_id.session_key())
            .await?
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
                .await?;
            if blacklisted {
                return Err(AuthError::InvalidToken);
            }

            Ok(login_id)
        } else {
            let val = self
                .storage
                .get_string(&access_key(access_token))
                .await?
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
        Ok(self.storage.get_session(&login_id.session_key()).await?)
    }

    // ── RBAC ──

    async fn require_session(&self, login_id: &LoginId) -> AuthResult<UserSession> {
        self.storage
            .get_session(&login_id.session_key())
            .await?
            .ok_or(AuthError::SessionNotFound)
    }

    pub async fn set_roles(&self, login_id: &LoginId, roles: Vec<String>) -> AuthResult<()> {
        let mut session = self.require_session(login_id).await?;
        session.profile.set_roles(roles);
        self.save_session(&session).await
    }

    pub async fn set_permissions(&self, login_id: &LoginId, perms: Vec<String>) -> AuthResult<()> {
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
        Ok(session
            .profile
            .permissions()
            .iter()
            .any(|p| permission_matches(p, perm)))
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

    // ── 批量角色检查 ──

    /// 是否拥有任意一个角色（OR 逻辑）
    pub async fn has_any_role(&self, login_id: &LoginId, roles: &[&str]) -> AuthResult<bool> {
        let session = self.require_session(login_id).await?;
        let user_roles = session.profile.roles();
        Ok(roles.iter().any(|r| user_roles.iter().any(|ur| ur == r)))
    }

    /// 是否拥有全部角色（AND 逻辑）
    pub async fn has_all_roles(&self, login_id: &LoginId, roles: &[&str]) -> AuthResult<bool> {
        let session = self.require_session(login_id).await?;
        let user_roles = session.profile.roles();
        Ok(roles.iter().all(|r| user_roles.iter().any(|ur| ur == r)))
    }

    /// 检查必须拥有任意一个角色，否则返回 NoRole 错误
    pub async fn check_any_role(&self, login_id: &LoginId, roles: &[&str]) -> AuthResult<()> {
        if self.has_any_role(login_id, roles).await? {
            Ok(())
        } else {
            Err(AuthError::NoRole(
                roles.iter().copied().collect::<Vec<_>>().join(" | "),
            ))
        }
    }

    /// 检查必须拥有全部角色，否则返回 NoRole 错误
    pub async fn check_all_roles(&self, login_id: &LoginId, roles: &[&str]) -> AuthResult<()> {
        let session = self.require_session(login_id).await?;
        let user_roles = session.profile.roles();
        for role in roles {
            if !user_roles.iter().any(|ur| ur == role) {
                return Err(AuthError::NoRole(role.to_string()));
            }
        }
        Ok(())
    }

    // ── 批量权限检查 ──

    /// 是否拥有任意一个权限（OR 逻辑，支持通配符）
    pub async fn has_any_permission(
        &self,
        login_id: &LoginId,
        perms: &[&str],
    ) -> AuthResult<bool> {
        let session = self.require_session(login_id).await?;
        let user_perms = session.profile.permissions();
        Ok(perms
            .iter()
            .any(|req| user_perms.iter().any(|owned| permission_matches(owned, req))))
    }

    /// 是否拥有全部权限（AND 逻辑，支持通配符）
    pub async fn has_all_permissions(
        &self,
        login_id: &LoginId,
        perms: &[&str],
    ) -> AuthResult<bool> {
        let session = self.require_session(login_id).await?;
        let user_perms = session.profile.permissions();
        Ok(perms
            .iter()
            .all(|req| user_perms.iter().any(|owned| permission_matches(owned, req))))
    }

    /// 检查必须拥有任意一个权限，否则返回 NoPermission 错误
    pub async fn check_any_permission(
        &self,
        login_id: &LoginId,
        perms: &[&str],
    ) -> AuthResult<()> {
        if self.has_any_permission(login_id, perms).await? {
            Ok(())
        } else {
            Err(AuthError::NoPermission(
                perms.iter().copied().collect::<Vec<_>>().join(" | "),
            ))
        }
    }

    /// 检查必须拥有全部权限，否则返回 NoPermission 错误
    pub async fn check_all_permissions(
        &self,
        login_id: &LoginId,
        perms: &[&str],
    ) -> AuthResult<()> {
        let session = self.require_session(login_id).await?;
        let user_perms = session.profile.permissions();
        for req in perms {
            if !user_perms.iter().any(|owned| permission_matches(owned, req)) {
                return Err(AuthError::NoPermission(req.to_string()));
            }
        }
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::permission_matches;

    #[test]
    fn exact_match() {
        assert!(permission_matches("system:user:list", "system:user:list"));
        assert!(!permission_matches("system:user:list", "system:user:add"));
    }

    #[test]
    fn super_wildcard() {
        assert!(permission_matches("*", "system:user:list"));
        assert!(permission_matches("*", "anything"));
    }

    #[test]
    fn trailing_wildcard() {
        assert!(permission_matches("system:*", "system:user:list"));
        assert!(permission_matches("system:*", "system:role:add"));
        assert!(permission_matches("system:*", "system:anything"));
        assert!(!permission_matches("system:*", "order:list"));
    }

    #[test]
    fn trailing_wildcard_two_levels() {
        assert!(permission_matches("system:user:*", "system:user:list"));
        assert!(permission_matches("system:user:*", "system:user:add"));
        assert!(!permission_matches("system:user:*", "system:role:list"));
    }

    #[test]
    fn middle_wildcard() {
        assert!(permission_matches("system:*:list", "system:user:list"));
        assert!(permission_matches("system:*:list", "system:role:list"));
        assert!(!permission_matches("system:*:list", "system:user:add"));
    }

    #[test]
    fn segment_count_mismatch() {
        // owned 段数少于 required 且末尾非 *
        assert!(!permission_matches("system:user", "system:user:list"));
        // owned 段数多于 required
        assert!(!permission_matches("system:user:list:detail", "system:user:list"));
    }

    #[test]
    fn single_segment() {
        assert!(permission_matches("admin", "admin"));
        assert!(!permission_matches("admin", "user"));
    }
}
