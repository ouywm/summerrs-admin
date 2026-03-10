use std::collections::HashMap;

use serde::Deserialize;
use summer::config::Configurable;

use crate::user_type::UserType;

fn default_token_name() -> String {
    "Authorization".to_string()
}

fn default_access_timeout() -> i64 {
    7200 // 2 小时
}

fn default_refresh_timeout() -> i64 {
    604800 // 7 天
}

fn default_true() -> bool {
    true
}

fn default_max_devices() -> usize {
    5
}

fn default_qr_timeout() -> i64 {
    300 // 5 分钟
}

fn default_token_prefix() -> Option<String> {
    Some("Bearer ".to_string())
}

fn default_jwt_issuer() -> String {
    "summer-admin".to_string()
}

fn default_jwt_audience() -> String {
    "summer-admin".to_string()
}

/// Token 风格：JWT 或 UUID（不透明 + Redis Session）
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenStyle {
    /// JWT 自包含 token（默认）
    #[default]
    Jwt,
    /// UUID 不透明 token + Redis 完整 Session
    Uuid,
}

/// JWT 签名算法
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
pub enum JwtAlgorithm {
    /// HMAC-SHA256（对称，使用 jwt_secret）
    #[default]
    HS256,
    /// HMAC-SHA384（对称，使用 jwt_secret）
    HS384,
    /// HMAC-SHA512（对称，使用 jwt_secret）
    HS512,
    /// RSA PKCS#1 v1.5 SHA-256（非对称，使用 jwt_private_key / jwt_public_key）
    RS256,
    /// RSA PKCS#1 v1.5 SHA-384
    RS384,
    /// RSA PKCS#1 v1.5 SHA-512
    RS512,
    /// ECDSA P-256 SHA-256（非对称，使用 jwt_private_key / jwt_public_key）
    ES256,
    /// ECDSA P-384 SHA-384
    ES384,
    /// EdDSA Ed25519（非对称，使用 jwt_private_key / jwt_public_key）
    EdDSA,
}

impl JwtAlgorithm {
    /// 是否为对称算法（HMAC 系列）
    pub fn is_symmetric(&self) -> bool {
        matches!(self, Self::HS256 | Self::HS384 | Self::HS512)
    }
}

#[derive(Debug, Clone, Deserialize, Configurable)]
#[config_prefix = "auth"]
pub struct AuthConfig {
    /// Token 风格：jwt 或 uuid，默认 jwt
    #[serde(default)]
    pub token_style: TokenStyle,

    /// Token 名称（Header 键名），默认 "Authorization"
    #[serde(default = "default_token_name")]
    pub token_name: String,

    /// Token 前缀（如 "Bearer "），设为 null / "" 则不使用前缀
    /// 默认 "Bearer "
    #[serde(default = "default_token_prefix")]
    pub token_prefix: Option<String>,

    /// Access Token 有效期（秒），默认 7200（2 小时）
    #[serde(default = "default_access_timeout")]
    pub access_timeout: i64,

    /// Refresh Token 有效期（秒），默认 604800（7 天）
    #[serde(default = "default_refresh_timeout")]
    pub refresh_timeout: i64,

    /// 是否允许同账号多设备并发登录，默认 true
    #[serde(default = "default_true")]
    pub concurrent_login: bool,

    /// 单用户最大设备数（0 = 不限），默认 5
    #[serde(default = "default_max_devices")]
    pub max_devices: usize,

    /// 是否从 Cookie 中读取 Token，默认 false
    /// TODO: 启用 Cookie 模式时需要实现 CSRF 防护
    /// 方案：登录时签发 CSRF token，前端在 Header 中携带，中间件双重校验
    #[serde(default)]
    pub is_read_cookie: bool,

    /// Cookie 名称（is_read_cookie = true 时使用），默认与 token_name 相同
    #[serde(default)]
    pub cookie_name: Option<String>,

    /// QR 码有效期（秒），默认 300（5 分钟）
    #[serde(default = "default_qr_timeout")]
    pub qr_code_timeout: i64,

    /// JWT 签名算法，默认 HS256
    /// 对称算法（HS256/HS384/HS512）使用 jwt_secret
    /// 非对称算法（RS256/ES256/EdDSA 等）使用 jwt_private_key + jwt_public_key
    #[serde(default)]
    pub jwt_algorithm: JwtAlgorithm,

    /// JWT 对称密钥（HMAC 算法时必须配置）
    #[serde(default)]
    pub jwt_secret: Option<String>,

    /// JWT 私钥文件路径（非对称算法时必须配置，PEM 格式）
    #[serde(default)]
    pub jwt_private_key: Option<String>,

    /// JWT 公钥文件路径（非对称算法时必须配置，PEM 格式）
    #[serde(default)]
    pub jwt_public_key: Option<String>,

    /// JWT 签发者标识（iss），默认 "summer-admin"
    #[serde(default = "default_jwt_issuer")]
    pub jwt_issuer: String,

    /// JWT 受众标识（aud），默认 "summer-admin"
    #[serde(default = "default_jwt_audience")]
    pub jwt_audience: String,
}

/// 按用户类型可覆盖的认证配置（所有字段 Option，None 表示使用全局默认）
///
/// 注意：`token_name`、`token_prefix`、`is_read_cookie`、`cookie_name`、`qr_code_timeout`
/// 不可覆盖——token 提取发生在用户类型已知之前，必须全局统一。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct AuthConfigOverride {
    pub token_style: Option<TokenStyle>,
    pub access_timeout: Option<i64>,
    pub refresh_timeout: Option<i64>,
    pub concurrent_login: Option<bool>,
    pub max_devices: Option<usize>,
    pub jwt_algorithm: Option<JwtAlgorithm>,
    pub jwt_secret: Option<String>,
    pub jwt_private_key: Option<String>,
    pub jwt_public_key: Option<String>,
    pub jwt_issuer: Option<String>,
    pub jwt_audience: Option<String>,
}

/// 合并后的用户类型完整配置（非 Option）
#[derive(Debug, Clone)]
pub struct ResolvedTypeConfig {
    pub token_style: TokenStyle,
    pub access_timeout: i64,
    pub refresh_timeout: i64,
    pub concurrent_login: bool,
    pub max_devices: usize,
    pub jwt_algorithm: JwtAlgorithm,
    pub jwt_secret: Option<String>,
    pub jwt_private_key: Option<String>,
    pub jwt_public_key: Option<String>,
    pub jwt_issuer: String,
    pub jwt_audience: String,
}

impl ResolvedTypeConfig {
    /// 将全局配置和可选的覆盖配置合并为完整配置
    pub fn merge(base: &AuthConfig, ovr: Option<&AuthConfigOverride>) -> Self {
        match ovr {
            None => Self {
                token_style: base.token_style,
                access_timeout: base.access_timeout,
                refresh_timeout: base.refresh_timeout,
                concurrent_login: base.concurrent_login,
                max_devices: base.max_devices,
                jwt_algorithm: base.jwt_algorithm,
                jwt_secret: base.jwt_secret.clone(),
                jwt_private_key: base.jwt_private_key.clone(),
                jwt_public_key: base.jwt_public_key.clone(),
                jwt_issuer: base.jwt_issuer.clone(),
                jwt_audience: base.jwt_audience.clone(),
            },
            Some(ovr) => Self {
                token_style: ovr.token_style.unwrap_or(base.token_style),
                access_timeout: ovr.access_timeout.unwrap_or(base.access_timeout),
                refresh_timeout: ovr.refresh_timeout.unwrap_or(base.refresh_timeout),
                concurrent_login: ovr.concurrent_login.unwrap_or(base.concurrent_login),
                max_devices: ovr.max_devices.unwrap_or(base.max_devices),
                jwt_algorithm: ovr.jwt_algorithm.unwrap_or(base.jwt_algorithm),
                jwt_secret: ovr.jwt_secret.clone().or_else(|| base.jwt_secret.clone()),
                jwt_private_key: ovr
                    .jwt_private_key
                    .clone()
                    .or_else(|| base.jwt_private_key.clone()),
                jwt_public_key: ovr
                    .jwt_public_key
                    .clone()
                    .or_else(|| base.jwt_public_key.clone()),
                jwt_issuer: ovr
                    .jwt_issuer
                    .clone()
                    .unwrap_or_else(|| base.jwt_issuer.clone()),
                jwt_audience: ovr
                    .jwt_audience
                    .clone()
                    .unwrap_or_else(|| base.jwt_audience.clone()),
            },
        }
    }

    /// 将 ResolvedTypeConfig 回填为 AuthConfig（供 TokenGenerator::new 使用）
    pub fn to_auth_config(&self, base: &AuthConfig) -> AuthConfig {
        AuthConfig {
            token_style: self.token_style,
            token_name: base.token_name.clone(),
            token_prefix: base.token_prefix.clone(),
            access_timeout: self.access_timeout,
            refresh_timeout: self.refresh_timeout,
            concurrent_login: self.concurrent_login,
            max_devices: self.max_devices,
            is_read_cookie: base.is_read_cookie,
            cookie_name: base.cookie_name.clone(),
            qr_code_timeout: base.qr_code_timeout,
            jwt_algorithm: self.jwt_algorithm,
            jwt_secret: self.jwt_secret.clone(),
            jwt_private_key: self.jwt_private_key.clone(),
            jwt_public_key: self.jwt_public_key.clone(),
            jwt_issuer: self.jwt_issuer.clone(),
            jwt_audience: self.jwt_audience.clone(),
        }
    }
}

/// 多用户类型认证配置（全局默认 + 按类型覆盖）
///
/// TOML 示例:
/// ```toml
/// [auth]
/// token_style = "jwt"
/// access_timeout = 7200
///
/// [auth.customer]
/// token_style = "uuid"
/// access_timeout = 3600
/// ```
#[derive(Debug, Clone, Deserialize, Configurable)]
#[config_prefix = "auth"]
pub struct MultiAuthConfig {
    #[serde(flatten)]
    pub base: AuthConfig,

    #[serde(default)]
    pub admin: Option<AuthConfigOverride>,

    #[serde(default)]
    pub business: Option<AuthConfigOverride>,

    #[serde(default)]
    pub customer: Option<AuthConfigOverride>,
}

impl MultiAuthConfig {
    /// 解析指定用户类型的完整配置
    pub fn resolve(&self, ut: &UserType) -> ResolvedTypeConfig {
        let ovr = match ut {
            UserType::Admin => self.admin.as_ref(),
            UserType::Business => self.business.as_ref(),
            UserType::Customer => self.customer.as_ref(),
        };
        ResolvedTypeConfig::merge(&self.base, ovr)
    }

    /// 解析所有用户类型的配置
    pub fn resolve_all(&self) -> HashMap<UserType, ResolvedTypeConfig> {
        UserType::all()
            .iter()
            .map(|ut| (*ut, self.resolve(ut)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_config() -> AuthConfig {
        serde_json::from_str(
            r#"{
                "token_name": "Authorization",
                "token_style": "jwt",
                "access_timeout": 7200,
                "refresh_timeout": 604800,
                "jwt_secret": "test-secret"
            }"#,
        )
        .unwrap()
    }

    #[test]
    fn resolve_no_override() {
        let base = base_config();
        let resolved = ResolvedTypeConfig::merge(&base, None);
        assert_eq!(resolved.token_style, TokenStyle::Jwt);
        assert_eq!(resolved.access_timeout, 7200);
        assert_eq!(resolved.refresh_timeout, 604800);
    }

    #[test]
    fn resolve_with_partial_override() {
        let base = base_config();
        let ovr = AuthConfigOverride {
            access_timeout: Some(3600),
            refresh_timeout: Some(86400),
            ..Default::default()
        };
        let resolved = ResolvedTypeConfig::merge(&base, Some(&ovr));
        assert_eq!(resolved.token_style, TokenStyle::Jwt); // 未覆盖 → 使用 base
        assert_eq!(resolved.access_timeout, 3600); // 覆盖
        assert_eq!(resolved.refresh_timeout, 86400); // 覆盖
    }

    #[test]
    fn resolve_token_style_override() {
        let base = base_config();
        let ovr = AuthConfigOverride {
            token_style: Some(TokenStyle::Uuid),
            ..Default::default()
        };
        let resolved = ResolvedTypeConfig::merge(&base, Some(&ovr));
        assert_eq!(resolved.token_style, TokenStyle::Uuid);
        assert_eq!(resolved.access_timeout, 7200); // 未覆盖
    }

    #[test]
    fn multi_auth_config_resolve_all() {
        let config: MultiAuthConfig = serde_json::from_str(
            r#"{
                "token_name": "Authorization",
                "access_timeout": 7200,
                "refresh_timeout": 604800,
                "jwt_secret": "test-secret",
                "customer": {
                    "token_style": "uuid",
                    "access_timeout": 3600
                }
            }"#,
        )
        .unwrap();

        let resolved = config.resolve_all();

        // Admin: 使用全局默认
        let admin = &resolved[&UserType::Admin];
        assert_eq!(admin.token_style, TokenStyle::Jwt);
        assert_eq!(admin.access_timeout, 7200);

        // Customer: 覆盖
        let customer = &resolved[&UserType::Customer];
        assert_eq!(customer.token_style, TokenStyle::Uuid);
        assert_eq!(customer.access_timeout, 3600);
        assert_eq!(customer.refresh_timeout, 604800); // 未覆盖
    }
}
