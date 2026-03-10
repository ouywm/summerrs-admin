use std::sync::Arc;

use parking_lot::RwLock;

use crate::bitmap::PermissionMap;
use crate::config::AuthConfig;
use crate::error::{AuthError, AuthResult};
use crate::session::model::{DeviceInfo, DeviceSession, UserProfile, ValidatedAccess};
use crate::storage::AuthStorage;
use crate::token::{TokenGenerator, TokenPair};
use crate::user_type::{DeviceType, LoginId};

// ── Redis key 生成 ──

fn refresh_key(rid: &str) -> String {
    format!("auth:refresh:{rid}")
}

fn device_key(login_id: &LoginId, device: &DeviceType) -> String {
    format!("auth:device:{}:{}", login_id.encode(), device.as_str())
}

fn deny_key(login_id: &LoginId) -> String {
    format!("auth:deny:{}", login_id.encode())
}

fn device_prefix(login_id: &LoginId) -> String {
    format!("auth:device:{}:", login_id.encode())
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
    pub(crate) perm_map: Arc<RwLock<Option<PermissionMap>>>,
}

impl SessionManager {
    pub fn new(storage: Arc<dyn AuthStorage>, config: AuthConfig) -> Self {
        let token_gen = TokenGenerator::new(&config);
        Self {
            storage,
            config,
            token_gen,
            perm_map: Arc::new(RwLock::new(None)),
        }
    }

    pub fn config(&self) -> &AuthConfig {
        &self.config
    }

    /// 设置权限映射表（启动时或权限变更时调用）
    pub fn set_permission_map(&self, map: PermissionMap) {
        *self.perm_map.write() = Some(map);
    }

    /// 读取权限映射表（clone）
    pub fn permission_map(&self) -> Option<PermissionMap> {
        self.perm_map.read().clone()
    }

    // ── 内部辅助 ──

    /// 存储设备信息到 Redis
    async fn store_device_info(
        &self,
        login_id: &LoginId,
        device: &DeviceType,
        rid: &str,
        login_ip: &str,
        user_agent: &str,
    ) -> AuthResult<()> {
        let info = DeviceInfo {
            rid: rid.to_string(),
            login_time: chrono::Local::now().timestamp_millis(),
            login_ip: login_ip.to_string(),
            user_agent: user_agent.to_string(),
        };
        let json =
            serde_json::to_string(&info).map_err(|e| AuthError::Internal(e.to_string()))?;

        self.storage
            .set_string(
                &device_key(login_id, device),
                &json,
                self.config.refresh_timeout,
            )
            .await?;
        Ok(())
    }

    /// 清理单个设备（删除 device key + 对应 refresh key）
    async fn cleanup_device(&self, login_id: &LoginId, device: &DeviceType) {
        let dk = device_key(login_id, device);
        if let Ok(Some(json)) = self.storage.get_string(&dk).await {
            if let Ok(info) = serde_json::from_str::<DeviceInfo>(&json) {
                let _ = self.storage.delete(&refresh_key(&info.rid)).await;
            }
        }
        let _ = self.storage.delete(&dk).await;
    }

    /// 从 device key 解析设备类型
    fn parse_device_from_key(key: &str, login_id: &LoginId) -> Option<DeviceType> {
        let prefix = format!("auth:device:{}:", login_id.encode());
        key.strip_prefix(&prefix).map(DeviceType::from)
    }

    /// 处理设备策略（并发登录、最大设备数）
    async fn handle_device_policy(
        &self,
        login_id: &LoginId,
        device: &DeviceType,
    ) -> AuthResult<()> {
        if !self.config.concurrent_login {
            // 不允许并发登录：清除所有设备
            let keys = self
                .storage
                .keys_by_prefix(&device_prefix(login_id))
                .await?;
            for key in &keys {
                if let Some(d) = Self::parse_device_from_key(key, login_id) {
                    self.cleanup_device(login_id, &d).await;
                }
            }
        } else {
            // 同设备重复登录：清除旧设备
            self.cleanup_device(login_id, device).await;
        }

        // 检查最大设备数
        if self.config.max_devices > 0 {
            let keys = self
                .storage
                .keys_by_prefix(&device_prefix(login_id))
                .await?;
            if keys.len() >= self.config.max_devices {
                // 踢掉最旧的设备（按 login_time 排序）
                let mut devices_with_time = Vec::new();
                for key in &keys {
                    if let Ok(Some(json)) = self.storage.get_string(key).await {
                        if let Ok(info) = serde_json::from_str::<DeviceInfo>(&json) {
                            devices_with_time.push((key.clone(), info));
                        }
                    }
                }
                devices_with_time.sort_by_key(|(_, info)| info.login_time);

                // 需要移除的数量
                let to_remove = devices_with_time.len() + 1 - self.config.max_devices;
                for (key, info) in devices_with_time.iter().take(to_remove) {
                    let _ = self.storage.delete(&refresh_key(&info.rid)).await;
                    let _ = self.storage.delete(key).await;
                }
            }
        }

        Ok(())
    }

    // ── 登录 ──

    pub async fn login(&self, params: LoginParams) -> AuthResult<TokenPair> {
        // 处理设备策略
        self.handle_device_policy(&params.login_id, &params.device)
            .await?;

        // 编码权限位图（如果有映射表）
        let pb = self.perm_map.read().as_ref().and_then(|map| {
            crate::bitmap::encode(params.profile.permissions(), map)
        });

        // 生成 Access JWT
        let (access_token, _access_claims) = self.token_gen.generate_access(
            &params.login_id,
            &params.device,
            &params.profile,
            pb.as_deref(),
            self.config.access_timeout,
        )?;

        // 生成 Refresh JWT
        let (refresh_token, refresh_claims) = self
            .token_gen
            .generate_refresh(&params.login_id, self.config.refresh_timeout)?;

        // 存储 refresh key: auth:refresh:{rid} → "login_id:device"
        let refresh_value = format!(
            "{}:{}",
            params.login_id.encode(),
            params.device.as_str()
        );
        self.storage
            .set_string(
                &refresh_key(&refresh_claims.rid),
                &refresh_value,
                self.config.refresh_timeout,
            )
            .await?;

        // 存储 device info
        self.store_device_info(
            &params.login_id,
            &params.device,
            &refresh_claims.rid,
            &params.login_ip,
            &params.user_agent,
        )
        .await?;

        Ok(TokenPair {
            access_token,
            refresh_token,
            expires_in: self.config.access_timeout,
        })
    }

    // ── 登出 ──

    pub async fn logout(&self, login_id: &LoginId, device: &DeviceType) -> AuthResult<()> {
        // 清理设备
        self.cleanup_device(login_id, device).await;

        // 设置 deny key（让该用户已签发的 access token 失效）
        // 使用时间戳方案：refresh 后签发的新 token (iat >= deny_ts) 放行
        let deny_value = format!("refresh:{}", chrono::Local::now().timestamp());
        self.storage
            .set_string(
                &deny_key(login_id),
                &deny_value,
                self.config.access_timeout,
            )
            .await?;

        Ok(())
    }

    pub async fn logout_all(&self, login_id: &LoginId) -> AuthResult<()> {
        // 扫描删除所有设备
        let keys = self
            .storage
            .keys_by_prefix(&device_prefix(login_id))
            .await?;
        for key in &keys {
            if let Ok(Some(json)) = self.storage.get_string(key).await {
                if let Ok(info) = serde_json::from_str::<DeviceInfo>(&json) {
                    let _ = self.storage.delete(&refresh_key(&info.rid)).await;
                }
            }
            let _ = self.storage.delete(key).await;
        }

        // 设置 deny key（时间戳方案）
        let deny_value = format!("refresh:{}", chrono::Local::now().timestamp());
        self.storage
            .set_string(
                &deny_key(login_id),
                &deny_value,
                self.config.access_timeout,
            )
            .await?;

        Ok(())
    }

    // ── 刷新 ──

    /// 刷新 token（需要调用方提供最新 profile）
    pub async fn refresh(
        &self,
        refresh_token: &str,
        profile: &UserProfile,
    ) -> AuthResult<TokenPair> {
        // 1. 解码 Refresh JWT
        let refresh_claims = self.token_gen.jwt().decode_refresh(refresh_token)?;
        let login_id =
            LoginId::decode(&refresh_claims.sub).ok_or(AuthError::InvalidRefreshToken)?;

        // 2. 检查 Redis 中 refresh key 是否存在
        let rk = refresh_key(&refresh_claims.rid);
        let refresh_value = self
            .storage
            .get_string(&rk)
            .await?
            .ok_or(AuthError::InvalidRefreshToken)?;

        // 解析 login_id:device
        let (_, device_str) = refresh_value
            .rsplit_once(':')
            .ok_or(AuthError::InvalidRefreshToken)?;
        let device = DeviceType::from(device_str);

        // 3. 检查 deny key
        if let Some(deny_value) = self.storage.get_string(&deny_key(&login_id)).await? {
            if deny_value == "banned" {
                return Err(AuthError::AccountBanned);
            }
            // deny="refresh:xxx" 或其他值时允许 refresh 操作（这正是刷新的目的）
        }

        // 4. 删除旧的 refresh key（轮转）
        let _ = self.storage.delete(&rk).await;

        // 5. 编码权限位图（如果有映射表）
        let pb = self.perm_map.read().as_ref().and_then(|map| {
            crate::bitmap::encode(profile.permissions(), map)
        });

        // 6. 生成新的 Access JWT（包含最新 profile）
        let (new_access_token, _) = self.token_gen.generate_access(
            &login_id,
            &device,
            profile,
            pb.as_deref(),
            self.config.access_timeout,
        )?;

        // 7. 生成新的 Refresh JWT
        let (new_refresh_token, new_refresh_claims) = self
            .token_gen
            .generate_refresh(&login_id, self.config.refresh_timeout)?;

        // 8. 存储新的 refresh key
        let new_refresh_value = format!("{}:{}", login_id.encode(), device.as_str());
        self.storage
            .set_string(
                &refresh_key(&new_refresh_claims.rid),
                &new_refresh_value,
                self.config.refresh_timeout,
            )
            .await?;

        // 9. 更新 device info
        let dk = device_key(&login_id, &device);
        if let Ok(Some(json)) = self.storage.get_string(&dk).await {
            if let Ok(mut info) = serde_json::from_str::<DeviceInfo>(&json) {
                info.rid = new_refresh_claims.rid;
                if let Ok(new_json) = serde_json::to_string(&info) {
                    let _ = self
                        .storage
                        .set_string(&dk, &new_json, self.config.refresh_timeout)
                        .await;
                }
            }
        }

        // 10. deny key 不删除，自然过期（修复多设备竞态：所有设备都必须 refresh 才能获得新权限）

        Ok(TokenPair {
            access_token: new_access_token,
            refresh_token: new_refresh_token,
            expires_in: self.config.access_timeout,
        })
    }

    // ── Token 验证 ──

    /// 验证 Access JWT + deny check → 返回 ValidatedAccess
    pub async fn validate_token(&self, access_token: &str) -> AuthResult<ValidatedAccess> {
        // 1. JWT 验证签名 + exp（本地，零 IO）
        let claims = self.token_gen.jwt().decode_access(access_token)?;
        let login_id = LoginId::decode(&claims.sub).ok_or(AuthError::InvalidToken)?;

        // 2. Redis GET deny key（~0.1ms）
        if let Some(deny_value) = self.storage.get_string(&deny_key(&login_id)).await? {
            match deny_value.as_str() {
                "banned" => return Err(AuthError::AccountBanned),
                v if v.starts_with("refresh:") => {
                    // 时间戳方案：token 签发时间 <= deny 设置时间 → 需要刷新
                    // refresh 后签发的新 token (iat > deny_ts) 自动放行
                    if let Ok(deny_ts) = v[8..].parse::<i64>() {
                        if claims.iat <= deny_ts {
                            return Err(AuthError::RefreshRequired);
                        }
                        // token 在 deny 之后签发，权限已是最新，放行
                    } else {
                        tracing::warn!("畸形 deny 值: {}, login_id: {}", deny_value, login_id.encode());
                        return Err(AuthError::InvalidToken);
                    }
                }
                _ => {
                    tracing::warn!("未知 deny 值: {}, login_id: {}", deny_value, login_id.encode());
                    return Err(AuthError::InvalidToken);
                }
            }
        }

        // 3. 解码权限：bitmap 优先，降级为 permissions 数组
        let permissions = if let Some(ref pb) = claims.pb {
            self.perm_map
                .read()
                .as_ref()
                .map(|map| crate::bitmap::decode(pb, map))
                .unwrap_or_else(|| claims.permissions.clone())
        } else {
            claims.permissions.clone()
        };

        // 4. 从 JWT claims 构造 ValidatedAccess
        Ok(ValidatedAccess {
            login_id,
            device: DeviceType::from(claims.dev.as_str()),
            user_name: claims.user_name,
            nick_name: claims.nick_name,
            roles: claims.roles,
            permissions,
        })
    }

    /// 仅解析 Refresh JWT（不查 Redis），返回 LoginId
    /// 给应用层拿 login_id 查 DB 用
    pub fn parse_refresh_token(&self, token: &str) -> AuthResult<LoginId> {
        let claims = self.token_gen.jwt().decode_refresh(token)?;
        LoginId::decode(&claims.sub).ok_or(AuthError::InvalidRefreshToken)
    }

    // ── 封禁/解封 ──

    /// 封禁用户（deny=banned + 清理所有设备/refresh）
    pub async fn ban_user(&self, login_id: &LoginId) -> AuthResult<()> {
        // 清理所有设备
        let keys = self
            .storage
            .keys_by_prefix(&device_prefix(login_id))
            .await?;
        for key in &keys {
            if let Ok(Some(json)) = self.storage.get_string(key).await {
                if let Ok(info) = serde_json::from_str::<DeviceInfo>(&json) {
                    let _ = self.storage.delete(&refresh_key(&info.rid)).await;
                }
            }
            let _ = self.storage.delete(key).await;
        }

        // 设置永久封禁（用 refresh_timeout 作为 TTL，足够长）
        self.storage
            .set_string(
                &deny_key(login_id),
                "banned",
                self.config.refresh_timeout,
            )
            .await?;

        Ok(())
    }

    /// 解封用户
    pub async fn unban_user(&self, login_id: &LoginId) -> AuthResult<()> {
        self.storage.delete(&deny_key(login_id)).await?;
        Ok(())
    }

    /// 强制刷新（设 deny="refresh:{timestamp}" TTL=access_timeout）
    /// 用于权限变更后让用户重新获取最新权限
    /// 时间戳方案：refresh 后签发的新 token (iat >= deny_ts) 自动放行，
    /// 旧 token (iat < deny_ts) 返回 RefreshRequired，消除多设备竞态
    pub async fn force_refresh(&self, login_id: &LoginId) -> AuthResult<()> {
        let deny_value = format!("refresh:{}", chrono::Local::now().timestamp());
        self.storage
            .set_string(
                &deny_key(login_id),
                &deny_value,
                self.config.access_timeout,
            )
            .await?;
        Ok(())
    }

    // ── 设备管理 ──

    /// 获取用户的所有设备信息
    pub async fn get_devices(&self, login_id: &LoginId) -> AuthResult<Vec<DeviceSession>> {
        let keys = self
            .storage
            .keys_by_prefix(&device_prefix(login_id))
            .await?;

        let mut devices = Vec::new();
        for key in &keys {
            if let Some(device) = Self::parse_device_from_key(key, login_id) {
                if let Ok(Some(json)) = self.storage.get_string(key).await {
                    if let Ok(info) = serde_json::from_str::<DeviceInfo>(&json) {
                        devices.push(DeviceSession {
                            device,
                            login_time: info.login_time,
                            login_ip: info.login_ip,
                            user_agent: info.user_agent,
                        });
                    }
                }
            }
        }

        Ok(devices)
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
        assert!(!permission_matches("system:user", "system:user:list"));
        assert!(!permission_matches("system:user:list:detail", "system:user:list"));
    }

    #[test]
    fn single_segment() {
        assert!(permission_matches("admin", "admin"));
        assert!(!permission_matches("admin", "user"));
    }
}
