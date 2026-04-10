use schemars::JsonSchema;
use serde::Serialize;
use summer_common::serde_utils::{percent_f32, percent_f64};

// ─── 服务监控 VO ────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfoVo {
    pub cpu: CpuInfo,
    pub memory: MemoryInfo,
    pub disks: Vec<DiskInfo>,
    pub sys: SysInfo,
    pub process: ProcessInfo,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CpuInfo {
    /// 物理核心数
    pub physical_core_count: usize,
    /// 逻辑核心数
    pub logical_core_count: usize,
    /// CPU 总使用率（%）
    #[serde(serialize_with = "percent_f32::serialize")]
    pub usage: f32,
    /// CPU 型号
    pub model_name: String,
    /// 每核使用率（%）
    pub per_core_usage: Vec<f32>,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MemoryInfo {
    /// 总内存（字节）
    pub total: u64,
    /// 已用内存（字节）
    pub used: u64,
    /// 可用内存（字节）
    pub available: u64,
    /// 内存使用率（%）
    #[serde(serialize_with = "percent_f64::serialize")]
    pub usage: f64,
    /// Swap 总量（字节）
    pub swap_total: u64,
    /// Swap 已用（字节）
    pub swap_used: u64,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DiskInfo {
    /// 磁盘名称
    pub name: String,
    /// 挂载点
    pub mount_point: String,
    /// 总空间（字节）
    pub total: u64,
    /// 已用空间（字节）
    pub used: u64,
    /// 可用空间（字节）
    pub available: u64,
    /// 使用率（%）
    #[serde(serialize_with = "percent_f64::serialize")]
    pub usage: f64,
    /// 文件系统类型
    pub fs_type: String,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SysInfo {
    /// 操作系统名称
    pub os_name: String,
    /// 操作系统版本
    pub os_version: String,
    /// 内核版本
    pub kernel_version: String,
    /// 系统架构（x86_64 / aarch64）
    pub arch: String,
    /// 主机名
    pub host_name: String,
    /// 系统运行时间（秒）
    pub uptime: u64,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProcessInfo {
    /// 当前进程 PID
    pub pid: u32,
    /// 进程名称
    pub name: String,
    /// 进程占用内存（字节）
    pub memory: u64,
    /// 进程 CPU 使用率（%）
    #[serde(serialize_with = "percent_f32::serialize")]
    pub cpu_usage: f32,
    /// 进程运行时间（秒）
    pub uptime: u64,
    /// 进程启动时间（UNIX 时间戳，秒）
    pub start_time: u64,
}

// ─── 缓存监控 VO ────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CacheInfoVo {
    // ─── 基础信息 ───
    pub version: String,
    pub mode: String,
    pub uptime: u64,
    pub tcp_port: u16,
    pub connected_clients: u64,
    pub db_count: u64,

    // ─── 内存 ───
    pub used_memory: u64,
    pub used_memory_human: String,
    pub used_memory_peak_human: String,
    pub maxmemory_human: String,
    pub mem_fragmentation_ratio: f64,

    // ─── 键空间 ───
    pub total_keys: u64,
    pub expires_keys: u64,

    // ─── 命中统计 ───
    pub keyspace_hits: u64,
    pub keyspace_misses: u64,
    #[serde(serialize_with = "percent_f64::serialize")]
    pub hit_rate: f64,
    pub instantaneous_ops_per_sec: u64,

    // ─── 持久化 ───
    pub aof_enabled: bool,
    pub rdb_last_save_time: u64,

    // ─── 图表数据 ───
    pub key_type_distribution: Vec<KeyTypeCount>,
}

/// 键类型计数
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct KeyTypeCount {
    /// 类型名称: string, hash, list, set, zset
    pub name: String,
    /// 该类型的键数量
    pub value: u64,
}

// ─── 缓存键列表 VO ──────────────────────────────────────────────────────────

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CacheKeysVo {
    pub keys: Vec<CacheKeyItem>,
    pub next_cursor: u64,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CacheKeyItem {
    pub key: String,
    pub ttl: i64,
    pub key_type: String,
    /// 序列化大小（可读格式，如 "256B", "1.2KB"）
    pub size: String,
    /// 内部编码（embstr/raw/int/listpack/quicklist/skiplist...）
    pub encoding: String,
}

// ─── 缓存键详情 VO ──────────────────────────────────────────────────────────

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CacheKeyDetailVo {
    /// 键名
    pub key: String,
    /// 键类型: string, hash, list, set, zset
    pub key_type: String,
    /// 剩余过期时间（秒），-1 = 永不过期
    pub ttl: i64,
    /// 序列化大小（可读格式）
    pub size: String,
    /// 内部编码
    pub encoding: String,
    /// 值内容（根据类型不同，结构不同）
    pub value: CacheKeyValue,
}

/// 缓存键值（按类型区分）
#[derive(Debug, Serialize, JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum CacheKeyValue {
    /// string 类型 → 直接返回字符串
    String { data: String },
    /// hash 类型 → 返回字段列表
    Hash { data: Vec<HashField> },
    /// list 类型 → 返回元素列表（前 100 条）
    List { data: Vec<String>, total: u64 },
    /// set 类型 → 返回成员列表（前 100 条）
    Set { data: Vec<String>, total: u64 },
    /// zset 类型 → 返回成员+分数列表（前 100 条，按分数降序）
    Zset { data: Vec<ZSetMember>, total: u64 },
    /// stream 类型 → 返回消息列表（最新 100 条，按 ID 降序）
    Stream { data: Vec<StreamEntry>, total: u64 },
    /// vectorset 类型 → 返回随机成员列表（Redis 8.0+）
    VectorSet { data: Vec<String>, total: u64 },
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HashField {
    pub field: String,
    pub value: String,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ZSetMember {
    pub member: String,
    pub score: f64,
}

/// Stream 消息条目
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StreamEntry {
    /// 消息 ID（如 "1678886400000-0"）
    pub id: String,
    /// 消息字段键值对
    pub fields: Vec<StreamField>,
}

/// Stream 消息字段
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StreamField {
    pub field: String,
    pub value: String,
}
