use std::collections::HashMap;
use std::sync::Arc;

use base64::Engine;
use parking_lot::RwLock;
use uuid::Uuid;

use crate::bitmap::PermissionMap;
use crate::config::{AuthConfig, MultiAuthConfig, ResolvedTypeConfig, TokenStyle};
use crate::error::{AuthError, AuthResult};
use crate::session::model::{
    DeviceInfo, DeviceSession, UuidSessionData, UserProfile, ValidatedAccess,
};
use crate::storage::AuthStorage;
use crate::token::{TokenGenerator, TokenPair};
use crate::user_type::{DeviceType, LoginId, UserType};

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

fn uuid_access_key(uuid: &str) -> String {
    format!("auth:uuid:access:{uuid}")
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

// ── Token 格式检测 ──

fn is_jwt_format(token: &str) -> bool {
    token.split('.').count() == 3
}

/// 无验证 peek JWT payload 的 sub 字段，获取 UserType
fn peek_jwt_user_type(token: &str) -> Option<UserType> {
    let payload = token.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()?;
    let v: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    let sub = v.get("sub")?.as_str()?;
    LoginId::decode(sub).map(|id| id.user_type)
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
    /// 全局配置（token_name, token_prefix 等提取相关）
    pub(crate) global_config: AuthConfig,
    /// 每种用户类型的解析后配置
    pub(crate) type_configs: HashMap<UserType, ResolvedTypeConfig>,
    /// JWT 模式的 token 生成器（仅 token_style=Jwt 的类型）
    pub(crate) jwt_generators: HashMap<UserType, TokenGenerator>,
    pub(crate) perm_map: Arc<RwLock<Option<PermissionMap>>>,
}

impl SessionManager {
    /// 从 MultiAuthConfig 构造（新接口）
    pub fn from_multi_config(storage: Arc<dyn AuthStorage>, multi_config: MultiAuthConfig) -> Self {
        let type_configs = multi_config.resolve_all();
        let mut jwt_generators = HashMap::new();
        for (ut, resolved) in &type_configs {
            if resolved.token_style == TokenStyle::Jwt {
                let auth_config = resolved.to_auth_config(&multi_config.base);
                jwt_generators.insert(*ut, TokenGenerator::new(&auth_config));
            }
        }
        Self {
            storage,
            global_config: multi_config.base,
            type_configs,
            jwt_generators,
            perm_map: Arc::new(RwLock::new(None)),
        }
    }

    /// 从单一 AuthConfig 构造（向后兼容，所有类型使用相同配置）
    pub fn new(storage: Arc<dyn AuthStorage>, config: AuthConfig) -> Self {
        let multi_config = MultiAuthConfig {
            base: config,
            admin: None,
            business: None,
            customer: None,
        };
        Self::from_multi_config(storage, multi_config)
    }

    pub fn config(&self) -> &AuthConfig {
        &self.global_config
    }

    /// 获取指定用户类型的 resolved 配置
    fn type_config(&self, ut: &UserType) -> &ResolvedTypeConfig {
        &self.type_configs[ut]
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
        refresh_timeout: i64,
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
            .set_string(&device_key(login_id, device), &json, refresh_timeout)
            .await?;
        Ok(())
    }

    /// 清理单个设备（删除 device key + 对应 refresh key）
    async fn cleanup_device(&self, login_id: &LoginId, device: &DeviceType) {
        let dk = device_key(login_id, device);
        if let Ok(Some(json)) = self.storage.get_string(&dk).await {
            if let Ok(info) = serde_json::from_str::<DeviceInfo>(&json) {
                let _ = self.storage.delete(&refresh_key(&info.rid)).await;
                // UUID 模式：rid 同时是 uuid access key 的标识
                let _ = self.storage.delete(&uuid_access_key(&info.rid)).await;
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
        config: &ResolvedTypeConfig,
    ) -> AuthResult<()> {
        if !config.concurrent_login {
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
        if config.max_devices > 0 {
            let keys = self
                .storage
                .keys_by_prefix(&device_prefix(login_id))
                .await?;
            if keys.len() >= config.max_devices {
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
                let to_remove = devices_with_time.len() + 1 - config.max_devices;
                for (key, info) in devices_with_time.iter().take(to_remove) {
                    let _ = self.storage.delete(&refresh_key(&info.rid)).await;
                    let _ = self.storage.delete(&uuid_access_key(&info.rid)).await;
                    let _ = self.storage.delete(key).await;
                }
            }
        }

        Ok(())
    }

    // ── deny check 共享逻辑 ──

    /// deny check: 比较 iat 与 deny_ts，返回 Ok(()) 或错误
    async fn check_deny(&self, login_id: &LoginId, iat: i64) -> AuthResult<()> {
        if let Some(deny_value) = self.storage.get_string(&deny_key(login_id)).await? {
            match deny_value.as_str() {
                "banned" => return Err(AuthError::AccountBanned),
                v if v.starts_with("refresh:") => {
                    if let Ok(deny_ts) = v[8..].parse::<i64>() {
                        if iat <= deny_ts {
                            return Err(AuthError::RefreshRequired);
                        }
                    } else {
                        tracing::warn!(
                            "畸形 deny 值: {}, login_id: {}",
                            deny_value,
                            login_id.encode()
                        );
                        return Err(AuthError::InvalidToken);
                    }
                }
                _ => {
                    tracing::warn!(
                        "未知 deny 值: {}, login_id: {}",
                        deny_value,
                        login_id.encode()
                    );
                    return Err(AuthError::InvalidToken);
                }
            }
        }
        Ok(())
    }

    /// deny check for refresh: banned → error, 其他 → allow
    async fn check_deny_for_refresh(&self, login_id: &LoginId) -> AuthResult<()> {
        if let Some(deny_value) = self.storage.get_string(&deny_key(login_id)).await? {
            if deny_value == "banned" {
                return Err(AuthError::AccountBanned);
            }
        }
        Ok(())
    }

    // ── 登录 ──

    pub async fn login(&self, params: LoginParams) -> AuthResult<TokenPair> {
        let ut = &params.login_id.user_type;
        let config = self.type_config(ut);

        // 处理设备策略
        self.handle_device_policy(&params.login_id, &params.device, config)
            .await?;

        match config.token_style {
            TokenStyle::Jwt => self.jwt_login(params, config).await,
            TokenStyle::Uuid => self.uuid_login(params, config).await,
        }
    }

    /// JWT 模式登录
    async fn jwt_login(
        &self,
        params: LoginParams,
        config: &ResolvedTypeConfig,
    ) -> AuthResult<TokenPair> {
        let ut = &params.login_id.user_type;
        let tg = self
            .jwt_generators
            .get(ut)
            .expect("JWT generator missing for JWT-mode user type");

        // 编码权限位图（如果有映射表）
        let pb = self.perm_map.read().as_ref().and_then(|map| {
            crate::bitmap::encode(params.profile.permissions(), map)
        });

        // 生成 Access JWT
        let (access_token, _) = tg.generate_access(
            &params.login_id,
            &params.device,
            &params.profile,
            pb.as_deref(),
            config.access_timeout,
        )?;

        // 生成 Refresh JWT
        let (refresh_token, refresh_claims) =
            tg.generate_refresh(&params.login_id, config.refresh_timeout)?;

        // 存储 refresh key
        let refresh_value = format!(
            "{}:{}",
            params.login_id.encode(),
            params.device.as_str()
        );
        self.storage
            .set_string(
                &refresh_key(&refresh_claims.rid),
                &refresh_value,
                config.refresh_timeout,
            )
            .await?;

        // 存储 device info
        self.store_device_info(
            &params.login_id,
            &params.device,
            &refresh_claims.rid,
            &params.login_ip,
            &params.user_agent,
            config.refresh_timeout,
        )
        .await?;

        Ok(TokenPair {
            access_token,
            refresh_token,
            expires_in: config.access_timeout,
        })
    }

    /// UUID 模式登录
    async fn uuid_login(
        &self,
        params: LoginParams,
        config: &ResolvedTypeConfig,
    ) -> AuthResult<TokenPair> {
        let access_uuid = Uuid::new_v4().to_string();
        let refresh_uuid = Uuid::new_v4().to_string();

        // 构造 UUID session data
        let session_data = UuidSessionData {
            login_id: params.login_id.encode(),
            device: params.device.as_str().to_string(),
            iat: chrono::Local::now().timestamp(),
            user_name: params.profile.user_name().to_string(),
            nick_name: params.profile.nick_name().to_string(),
            roles: params.profile.roles().to_vec(),
            permissions: params.profile.permissions().to_vec(),
        };

        let session_json = serde_json::to_string(&session_data)
            .map_err(|e| AuthError::Internal(e.to_string()))?;

        // 存储 access session
        self.storage
            .set_string(
                &uuid_access_key(&access_uuid),
                &session_json,
                config.access_timeout,
            )
            .await?;

        // 存储 refresh key
        let refresh_value = format!(
            "{}:{}",
            params.login_id.encode(),
            params.device.as_str()
        );
        self.storage
            .set_string(
                &refresh_key(&refresh_uuid),
                &refresh_value,
                config.refresh_timeout,
            )
            .await?;

        // 存储 device info（rid 使用 access_uuid，用于 cleanup 时删除 access session）
        self.store_device_info(
            &params.login_id,
            &params.device,
            &access_uuid,
            &params.login_ip,
            &params.user_agent,
            config.refresh_timeout,
        )
        .await?;

        Ok(TokenPair {
            access_token: access_uuid,
            refresh_token: refresh_uuid,
            expires_in: config.access_timeout,
        })
    }

    // ── 登出 ──

    pub async fn logout(&self, login_id: &LoginId, device: &DeviceType) -> AuthResult<()> {
        let config = self.type_config(&login_id.user_type);

        // 清理设备
        self.cleanup_device(login_id, device).await;

        // 设置 deny key（让该用户已签发的 access token 失效）
        let deny_value = format!("refresh:{}", chrono::Local::now().timestamp());
        self.storage
            .set_string(
                &deny_key(login_id),
                &deny_value,
                config.access_timeout,
            )
            .await?;

        Ok(())
    }

    pub async fn logout_all(&self, login_id: &LoginId) -> AuthResult<()> {
        let config = self.type_config(&login_id.user_type);

        // 扫描删除所有设备
        let keys = self
            .storage
            .keys_by_prefix(&device_prefix(login_id))
            .await?;
        for key in &keys {
            if let Ok(Some(json)) = self.storage.get_string(key).await {
                if let Ok(info) = serde_json::from_str::<DeviceInfo>(&json) {
                    let _ = self.storage.delete(&refresh_key(&info.rid)).await;
                    let _ = self.storage.delete(&uuid_access_key(&info.rid)).await;
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
                config.access_timeout,
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
        if is_jwt_format(refresh_token) {
            self.jwt_refresh(refresh_token, profile).await
        } else {
            self.uuid_refresh(refresh_token, profile).await
        }
    }

    /// JWT 模式刷新
    async fn jwt_refresh(
        &self,
        refresh_token: &str,
        profile: &UserProfile,
    ) -> AuthResult<TokenPair> {
        // 1. peek UserType
        let ut = peek_jwt_user_type(refresh_token).ok_or(AuthError::InvalidRefreshToken)?;
        let tg = self
            .jwt_generators
            .get(&ut)
            .ok_or(AuthError::InvalidRefreshToken)?;
        let config = self.type_config(&ut);

        // 2. 解码 Refresh JWT
        let refresh_claims = tg.jwt().decode_refresh(refresh_token)?;
        let login_id =
            LoginId::decode(&refresh_claims.sub).ok_or(AuthError::InvalidRefreshToken)?;

        // 3. 检查 Redis 中 refresh key 是否存在
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

        // 4. 检查 deny key
        self.check_deny_for_refresh(&login_id).await?;

        // 5. 删除旧的 refresh key（轮转）
        let _ = self.storage.delete(&rk).await;

        // 6. 编码权限位图（如果有映射表）
        let pb = self.perm_map.read().as_ref().and_then(|map| {
            crate::bitmap::encode(profile.permissions(), map)
        });

        // 7. 生成新的 Access JWT（包含最新 profile）
        let (new_access_token, _) = tg.generate_access(
            &login_id,
            &device,
            profile,
            pb.as_deref(),
            config.access_timeout,
        )?;

        // 8. 生成新的 Refresh JWT
        let (new_refresh_token, new_refresh_claims) =
            tg.generate_refresh(&login_id, config.refresh_timeout)?;

        // 9. 存储新的 refresh key
        let new_refresh_value = format!("{}:{}", login_id.encode(), device.as_str());
        self.storage
            .set_string(
                &refresh_key(&new_refresh_claims.rid),
                &new_refresh_value,
                config.refresh_timeout,
            )
            .await?;

        // 10. 更新 device info
        let dk = device_key(&login_id, &device);
        if let Ok(Some(json)) = self.storage.get_string(&dk).await {
            if let Ok(mut info) = serde_json::from_str::<DeviceInfo>(&json) {
                info.rid = new_refresh_claims.rid;
                if let Ok(new_json) = serde_json::to_string(&info) {
                    let _ = self
                        .storage
                        .set_string(&dk, &new_json, config.refresh_timeout)
                        .await;
                }
            }
        }

        Ok(TokenPair {
            access_token: new_access_token,
            refresh_token: new_refresh_token,
            expires_in: config.access_timeout,
        })
    }

    /// UUID 模式刷新
    async fn uuid_refresh(
        &self,
        refresh_token: &str,
        profile: &UserProfile,
    ) -> AuthResult<TokenPair> {
        // 1. Redis GET refresh key
        let rk = refresh_key(refresh_token);
        let refresh_value = self
            .storage
            .get_string(&rk)
            .await?
            .ok_or(AuthError::InvalidRefreshToken)?;

        // 解析 login_id:device
        let (login_id_str, device_str) = refresh_value
            .rsplit_once(':')
            .ok_or(AuthError::InvalidRefreshToken)?;
        let login_id = LoginId::decode(login_id_str).ok_or(AuthError::InvalidRefreshToken)?;
        let device = DeviceType::from(device_str);
        let config = self.type_config(&login_id.user_type);

        // 2. deny check
        self.check_deny_for_refresh(&login_id).await?;

        // 3. 删除旧 refresh key
        let _ = self.storage.delete(&rk).await;

        // 4. 生成新的 UUID 对
        let new_access_uuid = Uuid::new_v4().to_string();
        let new_refresh_uuid = Uuid::new_v4().to_string();

        // 5. 构造新的 session data
        let session_data = UuidSessionData {
            login_id: login_id.encode(),
            device: device.as_str().to_string(),
            iat: chrono::Local::now().timestamp(),
            user_name: profile.user_name().to_string(),
            nick_name: profile.nick_name().to_string(),
            roles: profile.roles().to_vec(),
            permissions: profile.permissions().to_vec(),
        };
        let session_json = serde_json::to_string(&session_data)
            .map_err(|e| AuthError::Internal(e.to_string()))?;

        // 6. 存储新的 access session
        self.storage
            .set_string(
                &uuid_access_key(&new_access_uuid),
                &session_json,
                config.access_timeout,
            )
            .await?;

        // 7. 存储新的 refresh key
        let new_refresh_value = format!("{}:{}", login_id.encode(), device.as_str());
        self.storage
            .set_string(
                &refresh_key(&new_refresh_uuid),
                &new_refresh_value,
                config.refresh_timeout,
            )
            .await?;

        // 8. 删除旧的 access session + 更新 device info
        let dk = device_key(&login_id, &device);
        if let Ok(Some(json)) = self.storage.get_string(&dk).await {
            if let Ok(mut info) = serde_json::from_str::<DeviceInfo>(&json) {
                // 删除旧的 uuid access key
                let _ = self.storage.delete(&uuid_access_key(&info.rid)).await;
                // 更新 device info 指向新的 access uuid
                info.rid = new_access_uuid.clone();
                if let Ok(new_json) = serde_json::to_string(&info) {
                    let _ = self
                        .storage
                        .set_string(&dk, &new_json, config.refresh_timeout)
                        .await;
                }
            }
        }

        Ok(TokenPair {
            access_token: new_access_uuid,
            refresh_token: new_refresh_uuid,
            expires_in: config.access_timeout,
        })
    }

    // ── Token 验证 ──

    /// 验证 Access Token（JWT 或 UUID）+ deny check → 返回 ValidatedAccess
    pub async fn validate_token(&self, access_token: &str) -> AuthResult<ValidatedAccess> {
        if is_jwt_format(access_token) {
            self.jwt_validate(access_token).await
        } else {
            self.uuid_validate(access_token).await
        }
    }

    /// JWT 模式验证
    async fn jwt_validate(&self, access_token: &str) -> AuthResult<ValidatedAccess> {
        // 1. peek UserType
        let ut = peek_jwt_user_type(access_token).ok_or(AuthError::InvalidToken)?;
        let tg = self
            .jwt_generators
            .get(&ut)
            .ok_or(AuthError::InvalidToken)?;

        // 2. JWT 验证签名 + exp（本地，零 IO）
        let claims = tg.jwt().decode_access(access_token)?;
        let login_id = LoginId::decode(&claims.sub).ok_or(AuthError::InvalidToken)?;

        // 3. deny check
        self.check_deny(&login_id, claims.iat).await?;

        // 4. 解码权限：bitmap 优先，降级为 permissions 数组
        let permissions = if let Some(ref pb) = claims.pb {
            self.perm_map
                .read()
                .as_ref()
                .map(|map| crate::bitmap::decode(pb, map))
                .unwrap_or_else(|| claims.permissions.clone())
        } else {
            claims.permissions.clone()
        };

        // 5. 从 JWT claims 构造 ValidatedAccess
        Ok(ValidatedAccess {
            login_id,
            device: DeviceType::from(claims.dev.as_str()),
            user_name: claims.user_name,
            nick_name: claims.nick_name,
            roles: claims.roles,
            permissions,
        })
    }

    /// UUID 模式验证
    async fn uuid_validate(&self, access_token: &str) -> AuthResult<ValidatedAccess> {
        // 1. Redis GET uuid access key
        let session_json = self
            .storage
            .get_string(&uuid_access_key(access_token))
            .await?
            .ok_or(AuthError::InvalidToken)?;

        // 2. 反序列化 session
        let session: UuidSessionData = serde_json::from_str(&session_json)
            .map_err(|e| AuthError::Internal(format!("UUID session 反序列化失败: {e}")))?;

        let login_id = LoginId::decode(&session.login_id).ok_or(AuthError::InvalidToken)?;

        // 3. deny check（比较 iat vs deny_ts）
        self.check_deny(&login_id, session.iat).await?;

        // 4. 构造 ValidatedAccess
        Ok(ValidatedAccess {
            login_id,
            device: DeviceType::from(session.device.as_str()),
            user_name: session.user_name,
            nick_name: session.nick_name,
            roles: session.roles,
            permissions: session.permissions,
        })
    }

    /// 解析 refresh token → 返回 LoginId（async，JWT 模式本地解码，UUID 模式查 Redis）
    pub async fn parse_refresh_token(&self, token: &str) -> AuthResult<LoginId> {
        if is_jwt_format(token) {
            // JWT 模式：peek UserType → 取对应 generator → decode
            let ut = peek_jwt_user_type(token).ok_or(AuthError::InvalidRefreshToken)?;
            let tg = self
                .jwt_generators
                .get(&ut)
                .ok_or(AuthError::InvalidRefreshToken)?;
            let claims = tg.jwt().decode_refresh(token)?;
            LoginId::decode(&claims.sub).ok_or(AuthError::InvalidRefreshToken)
        } else {
            // UUID 模式：Redis GET refresh key → 解析 login_id
            let rk = refresh_key(token);
            let refresh_value = self
                .storage
                .get_string(&rk)
                .await?
                .ok_or(AuthError::InvalidRefreshToken)?;
            let (login_id_str, _) = refresh_value
                .rsplit_once(':')
                .ok_or(AuthError::InvalidRefreshToken)?;
            LoginId::decode(login_id_str).ok_or(AuthError::InvalidRefreshToken)
        }
    }

    // ── 封禁/解封 ──

    /// 封禁用户（deny=banned + 清理所有设备/refresh）
    pub async fn ban_user(&self, login_id: &LoginId) -> AuthResult<()> {
        let config = self.type_config(&login_id.user_type);

        // 清理所有设备
        let keys = self
            .storage
            .keys_by_prefix(&device_prefix(login_id))
            .await?;
        for key in &keys {
            if let Ok(Some(json)) = self.storage.get_string(key).await {
                if let Ok(info) = serde_json::from_str::<DeviceInfo>(&json) {
                    let _ = self.storage.delete(&refresh_key(&info.rid)).await;
                    let _ = self.storage.delete(&uuid_access_key(&info.rid)).await;
                }
            }
            let _ = self.storage.delete(key).await;
        }

        // 设置永久封禁（用 refresh_timeout 作为 TTL，足够长）
        self.storage
            .set_string(
                &deny_key(login_id),
                "banned",
                config.refresh_timeout,
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
    pub async fn force_refresh(&self, login_id: &LoginId) -> AuthResult<()> {
        let config = self.type_config(&login_id.user_type);
        let deny_value = format!("refresh:{}", chrono::Local::now().timestamp());
        self.storage
            .set_string(
                &deny_key(login_id),
                &deny_value,
                config.access_timeout,
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
    use super::*;

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
        assert!(!permission_matches(
            "system:user:list:detail",
            "system:user:list"
        ));
    }

    #[test]
    fn single_segment() {
        assert!(permission_matches("admin", "admin"));
        assert!(!permission_matches("admin", "user"));
    }

    #[test]
    fn is_jwt_format_detection() {
        assert!(is_jwt_format("header.payload.signature"));
        assert!(is_jwt_format("a.b.c"));
        assert!(!is_jwt_format("550e8400-e29b-41d4-a716-446655440000"));
        assert!(!is_jwt_format("not-a-jwt"));
        assert!(!is_jwt_format("two.parts"));
        assert!(!is_jwt_format("four.parts.here.wow"));
    }

    #[test]
    fn peek_jwt_user_type_correct() {
        // 构造一个有效的 JWT payload
        use base64::engine::general_purpose::URL_SAFE_NO_PAD;
        let payload_json = r#"{"sub":"admin:42","typ":"access"}"#;
        let payload_b64 = URL_SAFE_NO_PAD.encode(payload_json.as_bytes());
        let fake_jwt = format!("header.{payload_b64}.signature");

        let ut = peek_jwt_user_type(&fake_jwt);
        assert_eq!(ut, Some(UserType::Admin));

        let payload_json2 = r#"{"sub":"user:1"}"#;
        let payload_b64_2 = URL_SAFE_NO_PAD.encode(payload_json2.as_bytes());
        let fake_jwt2 = format!("header.{payload_b64_2}.signature");
        assert_eq!(peek_jwt_user_type(&fake_jwt2), Some(UserType::Customer));
    }
}
