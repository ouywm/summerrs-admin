use serde::Deserialize;
use summer::config::Configurable;

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

/// Token 风格
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenStyle {
    /// 不透明令牌（UUID v4）— 依赖 Redis 反查键验证
    #[default]
    Uuid,
    /// JWT — 本地签名验证 + Redis 黑名单撤销
    Jwt,
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

    /// 请求时自动续期 access token 的 TTL，默认 false
    #[serde(default)]
    pub auto_renew: bool,

    /// 是否允许同账号多设备并发登录，默认 true
    #[serde(default = "default_true")]
    pub concurrent_login: bool,

    /// 同一设备重复登录时是否复用 Token，默认 false
    #[serde(default)]
    pub share_token: bool,

    /// 单用户最大设备数（0 = 不限），默认 5
    #[serde(default = "default_max_devices")]
    pub max_devices: usize,

    /// Token 风格，默认 uuid
    #[serde(default)]
    pub token_style: TokenStyle,

    /// 是否从 Cookie 中读取 Token，默认 false
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
}
