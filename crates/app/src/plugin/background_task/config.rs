//! 后台任务队列配置

use serde::Deserialize;
use summer::config::Configurable;

/// 后台任务队列配置
///
/// 在 TOML 中配置示例：
/// ```toml
/// [background-task]
/// capacity = 4096
/// workers = 4
/// ```
#[derive(Debug, Deserialize, Configurable)]
#[config_prefix = "background-task"]
pub struct BackgroundTaskConfig {
    /// 队列容量（默认 4096，所有 worker 共享同一个队列）
    #[serde(default = "default_capacity")]
    pub capacity: usize,
    /// worker 数量（默认 4，即 4 个并发消费者）
    #[serde(default = "default_workers")]
    pub workers: usize,
}

pub(super) fn default_capacity() -> usize {
    4096
}

pub(super) fn default_workers() -> usize {
    4
}
