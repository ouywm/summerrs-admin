use serde::{Deserialize, Serialize};

fn default_mask_char() -> String {
    "*".to_string()
}

/// 脱敏规则配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MaskingRuleConfig {
    /// 需要脱敏的逻辑表。
    pub table: String,
    /// 需要脱敏的字段名。
    pub column: String,
    /// 脱敏算法名称。
    pub algorithm: String,
    /// 保留前缀字符数。
    #[serde(default)]
    pub show_first: usize,
    /// 保留后缀字符数。
    #[serde(default)]
    pub show_last: usize,
    /// 用于填充的脱敏字符。
    #[serde(default = "default_mask_char")]
    pub mask_char: String,
}

/// 脱敏模块配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MaskingConfig {
    /// 是否启用脱敏。
    #[serde(default)]
    pub enabled: bool,
    /// 脱敏规则列表。
    #[serde(default)]
    pub rules: Vec<MaskingRuleConfig>,
}
