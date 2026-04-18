use serde::{Deserialize, Serialize};

/// 字段加密规则。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EncryptRuleConfig {
    /// 需要加密的逻辑表。
    pub table: String,
    /// 明文字段名。
    pub column: String,
    /// 密文字段名。
    pub cipher_column: String,
    /// 辅助查询字段名，例如用于等值匹配或模糊检索。
    #[serde(default)]
    pub assisted_query_column: Option<String>,
    /// 加密算法名称。
    #[serde(default)]
    pub algorithm: String,
    /// 存放密钥的环境变量名。
    #[serde(default)]
    pub key_env: String,
}

/// 加密模块配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EncryptConfig {
    /// 是否启用字段加密。
    #[serde(default)]
    pub enabled: bool,
    /// 加密规则列表。
    #[serde(default)]
    pub rules: Vec<EncryptRuleConfig>,
}
