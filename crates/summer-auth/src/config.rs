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

fn default_jwt_issuer() -> String {
    "summer-admin".to_string()
}

fn default_jwt_audience() -> String {
    "summer-admin".to_string()
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
