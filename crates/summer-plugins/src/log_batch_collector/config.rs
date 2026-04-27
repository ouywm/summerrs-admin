//! 日志批量收集器配置

use serde::Deserialize;
use summer::config::Configurable;

/// 批量收集器配置
///
/// ```toml
/// [log-batch]
/// batch_size = 50
/// flush_interval_ms = 500
/// capacity = 4096
/// ```
#[derive(Debug, Clone, Deserialize, Configurable)]
#[config_prefix = "log-batch"]
pub struct LogBatchConfig {
    /// 批量大小阈值（累积多少条后触发一次 INSERT，默认 50）
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
    /// 刷新间隔毫秒（即使不足 batch_size 也强制刷新，默认 500）
    #[serde(default = "default_flush_interval_ms")]
    pub flush_interval_ms: u64,
    /// 通道容量（默认 4096）
    #[serde(default = "default_capacity")]
    pub capacity: usize,
}

impl Default for LogBatchConfig {
    fn default() -> Self {
        Self {
            batch_size: default_batch_size(),
            flush_interval_ms: default_flush_interval_ms(),
            capacity: default_capacity(),
        }
    }
}

pub(super) fn default_batch_size() -> usize {
    50
}
pub(super) fn default_flush_interval_ms() -> u64 {
    500
}
pub(super) fn default_capacity() -> usize {
    4096
}
