use serde::{Deserialize, Serialize};

const fn default_slow_query_threshold_ms() -> u64 {
    500
}

/// 审计配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditConfig {
    /// 是否启用审计。
    #[serde(default)]
    pub enabled: bool,
    /// 慢查询阈值，单位毫秒。
    #[serde(default = "default_slow_query_threshold_ms")]
    pub slow_query_threshold_ms: u64,
    /// 是否记录全散射查询。
    #[serde(default)]
    pub log_full_scatter: bool,
    /// 是否记录缺失分片键的 SQL。
    #[serde(default)]
    pub log_no_sharding_key: bool,
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            slow_query_threshold_ms: default_slow_query_threshold_ms(),
            log_full_scatter: false,
            log_no_sharding_key: false,
        }
    }
}
