use serde::{Deserialize, Serialize};

/// 登录成功后返回的 Token 对
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenPair {
    /// 访问令牌（短期，如 2 小时）
    pub access_token: String,
    /// 刷新令牌（长期，如 7 天）
    pub refresh_token: String,
    /// access_token 过期时间（秒）
    pub expires_in: i64,
}
