use schemars::JsonSchema;
use serde::Deserialize;

/// 缓存键列表查询参数
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CacheKeysQuery {
    #[serde(default = "default_pattern")]
    pub pattern: String,
    #[serde(default)]
    pub cursor: u64,
    #[serde(default = "default_count")]
    pub count: u64,
}

fn default_pattern() -> String {
    "*".to_string()
}

fn default_count() -> u64 {
    20
}

/// 缓存批量删除查询参数
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CacheDeleteQuery {
    pub pattern: String,
}
