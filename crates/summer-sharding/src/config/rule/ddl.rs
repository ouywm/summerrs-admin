use serde::{Deserialize, Serialize};

const fn default_online_ddl_concurrency() -> usize {
    3
}

const fn default_online_ddl_batch_size() -> usize {
    10_000
}

const fn default_online_ddl_cutover_lock_timeout_ms() -> u64 {
    5_000
}

const fn default_online_ddl_cleanup_delay_hours() -> u64 {
    24
}

/// 在线 DDL 配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OnlineDdlConfig {
    /// 是否启用在线 DDL。
    #[serde(default)]
    pub enabled: bool,
    /// 并发执行的分片任务数。
    #[serde(default = "default_online_ddl_concurrency")]
    pub concurrency: usize,
    /// 单批次处理的数据量。
    #[serde(default = "default_online_ddl_batch_size")]
    pub batch_size: usize,
    /// cutover 阶段获取锁的超时时间，单位毫秒。
    #[serde(default = "default_online_ddl_cutover_lock_timeout_ms")]
    pub cutover_lock_timeout_ms: u64,
    /// 清理旧表或临时资源的延迟时间，单位小时。
    #[serde(default = "default_online_ddl_cleanup_delay_hours")]
    pub cleanup_delay_hours: u64,
}

impl Default for OnlineDdlConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            concurrency: default_online_ddl_concurrency(),
            batch_size: default_online_ddl_batch_size(),
            cutover_lock_timeout_ms: default_online_ddl_cutover_lock_timeout_ms(),
            cleanup_delay_hours: default_online_ddl_cleanup_delay_hours(),
        }
    }
}
