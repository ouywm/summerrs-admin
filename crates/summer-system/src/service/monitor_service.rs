use std::collections::HashMap;

use summer_common::error::{ApiErrors, ApiResult};
use summer_model::dto::monitor::{CacheDeleteQuery, CacheKeysQuery};
use summer_model::vo::monitor::{
    CacheInfoVo, CacheKeyDetailVo, CacheKeyItem, CacheKeyValue, CacheKeysVo, CpuInfo, DiskInfo,
    HashField, KeyTypeCount, MemoryInfo, ProcessInfo, ServerInfoVo, StreamEntry, StreamField,
    SysInfo, ZSetMember,
};
use summer::plugin::Service;
use summer_redis::redis::{self, AsyncCommands};

// ═══════════════════════════════════════════════════════════════════════════════
// 服务监控 Service
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Clone, Service)]
pub struct ServerMonitorService;

impl ServerMonitorService {
    pub async fn get_server_info(&self) -> ApiResult<ServerInfoVo> {
        use sysinfo::{CpuRefreshKind, Disks, ProcessRefreshKind, RefreshKind, System};

        let mut sys = System::new_with_specifics(
            RefreshKind::nothing()
                .with_cpu(CpuRefreshKind::everything())
                .with_memory(sysinfo::MemoryRefreshKind::everything()),
        );

        // CPU 使用率需要二次采集（首次返回 0）
        tokio::time::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL).await;
        sys.refresh_cpu_usage();

        // 刷新当前进程信息
        let current_pid = sysinfo::get_current_pid().unwrap_or(sysinfo::Pid::from(0));
        sys.refresh_processes_specifics(
            sysinfo::ProcessesToUpdate::Some(&[current_pid]),
            true,
            ProcessRefreshKind::nothing().with_cpu().with_memory(),
        );

        // CPU
        let cpu = {
            let cpus = sys.cpus();
            let model_name = cpus
                .first()
                .map(|c| c.brand().to_string())
                .unwrap_or_default();
            let per_core_usage: Vec<f32> = cpus.iter().map(|c| c.cpu_usage()).collect();
            CpuInfo {
                physical_core_count: System::physical_core_count().unwrap_or(cpus.len()),
                logical_core_count: cpus.len(),
                usage: sys.global_cpu_usage(),
                model_name,
                per_core_usage,
            }
        };

        // 内存
        let total_mem = sys.total_memory();
        let used_mem = sys.used_memory();
        let available_mem = sys.available_memory();
        let memory = MemoryInfo {
            total: total_mem,
            used: used_mem,
            available: available_mem,
            usage: if total_mem > 0 {
                (used_mem as f64 / total_mem as f64) * 100.0
            } else {
                0.0
            },
            swap_total: sys.total_swap(),
            swap_used: sys.used_swap(),
        };

        // 磁盘（过滤虚拟文件系统）
        let disk_list = Disks::new_with_refreshed_list();
        let disks: Vec<DiskInfo> = disk_list
            .iter()
            .filter(|d| {
                let fs = d.file_system().to_string_lossy().to_string();
                !matches!(
                    fs.as_str(),
                    "tmpfs" | "devtmpfs" | "overlay" | "squashfs" | "devfs"
                )
            })
            .map(|d| {
                let total = d.total_space();
                let available = d.available_space();
                let used = total.saturating_sub(available);
                DiskInfo {
                    name: d.name().to_string_lossy().to_string(),
                    mount_point: d.mount_point().to_string_lossy().to_string(),
                    total,
                    used,
                    available,
                    usage: if total > 0 {
                        (used as f64 / total as f64) * 100.0
                    } else {
                        0.0
                    },
                    fs_type: d.file_system().to_string_lossy().to_string(),
                }
            })
            .collect();

        // 系统信息
        let sys_info = SysInfo {
            os_name: System::name().unwrap_or_default(),
            os_version: System::os_version().unwrap_or_default(),
            kernel_version: System::kernel_version().unwrap_or_default(),
            arch: System::cpu_arch(),
            host_name: System::host_name().unwrap_or_default(),
            uptime: System::uptime(),
        };

        // 当前进程
        let process = sys
            .process(current_pid)
            .map(|p| ProcessInfo {
                pid: current_pid.as_u32(),
                name: p.name().to_string_lossy().to_string(),
                memory: p.memory(),
                cpu_usage: p.cpu_usage(),
                uptime: p.run_time(),
                start_time: p.start_time(),
            })
            .unwrap_or(ProcessInfo {
                pid: current_pid.as_u32(),
                name: String::new(),
                memory: 0,
                cpu_usage: 0.0,
                uptime: 0,
                start_time: 0,
            });

        Ok(ServerInfoVo {
            cpu,
            memory,
            disks,
            sys: sys_info,
            process,
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 缓存监控 Service
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Clone, Service)]
pub struct CacheMonitorService {
    #[inject(component)]
    redis: summer_redis::Redis,
}

impl CacheMonitorService {
    pub async fn get_cache_info(&self) -> ApiResult<CacheInfoVo> {
        let mut conn = self.redis.clone();

        let info: String = redis::cmd("INFO")
            .query_async(&mut conn)
            .await
            .map_err(|e| ApiErrors::Internal(anyhow::anyhow!("Redis INFO 失败: {e}")))?;

        let map = parse_redis_info(&info);

        let keyspace_hits = parse_u64(&map, "keyspace_hits");
        let keyspace_misses = parse_u64(&map, "keyspace_misses");
        let total_hits = keyspace_hits + keyspace_misses;
        let hit_rate = if total_hits > 0 {
            (keyspace_hits as f64 / total_hits as f64) * 100.0
        } else {
            0.0
        };

        let (total_keys, expires_keys, db_count) = parse_keyspace(&map);

        let maxmemory = parse_u64(&map, "maxmemory");
        let maxmemory_human = if maxmemory == 0 {
            "无限制".to_string()
        } else {
            get_str(&map, "maxmemory_human")
        };

        // 键类型分布（采样统计）
        let key_type_distribution = self.get_key_type_distribution(&mut conn).await;

        Ok(CacheInfoVo {
            // 基础信息
            version: get_str(&map, "redis_version"),
            mode: get_str(&map, "redis_mode"),
            uptime: parse_u64(&map, "uptime_in_seconds"),
            tcp_port: parse_u64(&map, "tcp_port") as u16,
            connected_clients: parse_u64(&map, "connected_clients"),
            db_count,
            // 内存
            used_memory: parse_u64(&map, "used_memory"),
            used_memory_human: get_str(&map, "used_memory_human"),
            used_memory_peak_human: get_str(&map, "used_memory_peak_human"),
            maxmemory_human,
            mem_fragmentation_ratio: parse_f64(&map, "mem_fragmentation_ratio"),
            // 键空间
            total_keys,
            expires_keys,
            // 命中统计
            keyspace_hits,
            keyspace_misses,
            hit_rate,
            instantaneous_ops_per_sec: parse_u64(&map, "instantaneous_ops_per_sec"),
            // 持久化
            aof_enabled: parse_u64(&map, "aof_enabled") == 1,
            rdb_last_save_time: parse_u64(&map, "rdb_last_save_time"),
            // 图表数据
            key_type_distribution,
        })
    }

    /// 采样获取键类型分布
    async fn get_key_type_distribution(&self, conn: &mut summer_redis::Redis) -> Vec<KeyTypeCount> {
        let mut counts: HashMap<String, u64> = HashMap::new();
        let mut cursor: u64 = 0;
        let mut sampled = 0u64;
        const MAX_SAMPLE: u64 = 1000;

        loop {
            let result: Result<(u64, Vec<String>), _> = redis::cmd("SCAN")
                .arg(cursor)
                .arg("COUNT")
                .arg(200u64)
                .query_async(conn)
                .await;

            let (next_cursor, keys) = match result {
                Ok(r) => r,
                Err(_) => break,
            };

            for key in &keys {
                if let Ok(key_type) = redis::cmd("TYPE")
                    .arg(key)
                    .query_async::<String>(conn)
                    .await
                {
                    *counts.entry(key_type).or_insert(0) += 1;
                }
                sampled += 1;
                if sampled >= MAX_SAMPLE {
                    break;
                }
            }

            cursor = next_cursor;
            if cursor == 0 || sampled >= MAX_SAMPLE {
                break;
            }
        }

        // 按 value 降序排列
        let mut result: Vec<KeyTypeCount> = counts
            .into_iter()
            .filter(|(name, _)| name != "none")
            .map(|(name, value)| KeyTypeCount { name, value })
            .collect();
        result.sort_by(|a, b| b.value.cmp(&a.value));
        result
    }

    pub async fn get_cache_keys(&self, query: CacheKeysQuery) -> ApiResult<CacheKeysVo> {
        let mut conn = self.redis.clone();

        let (next_cursor, raw_keys): (u64, Vec<String>) = redis::cmd("SCAN")
            .arg(query.cursor)
            .arg("MATCH")
            .arg(&query.pattern)
            .arg("COUNT")
            .arg(query.count)
            .query_async(&mut conn)
            .await
            .map_err(|e| ApiErrors::Internal(anyhow::anyhow!("Redis SCAN 失败: {e}")))?;

        let mut keys = Vec::with_capacity(raw_keys.len());
        for key in &raw_keys {
            let key_type: String = redis::cmd("TYPE")
                .arg(key)
                .query_async(&mut conn)
                .await
                .unwrap_or_else(|_| "unknown".to_string());

            let ttl: i64 = conn.ttl(key).await.unwrap_or(-1);

            let mem_usage: i64 = redis::cmd("MEMORY")
                .arg("USAGE")
                .arg(key)
                .query_async(&mut conn)
                .await
                .unwrap_or(-1);
            let size = format_bytes(mem_usage);

            let encoding: String = redis::cmd("OBJECT")
                .arg("ENCODING")
                .arg(key)
                .query_async(&mut conn)
                .await
                .unwrap_or_else(|_| "unknown".to_string());

            keys.push(CacheKeyItem {
                key: key.clone(),
                ttl,
                key_type,
                size,
                encoding,
            });
        }

        Ok(CacheKeysVo { keys, next_cursor })
    }

    pub async fn get_cache_key_detail(&self, key: &str) -> ApiResult<CacheKeyDetailVo> {
        let mut conn = self.redis.clone();

        // 检查键是否存在
        let exists: bool = conn
            .exists(key)
            .await
            .map_err(|e| ApiErrors::Internal(anyhow::anyhow!("Redis EXISTS 失败: {e}")))?;
        if !exists {
            return Err(ApiErrors::NotFound(format!("缓存键 '{key}' 不存在")));
        }

        let key_type: String = redis::cmd("TYPE")
            .arg(key)
            .query_async(&mut conn)
            .await
            .map_err(|e| ApiErrors::Internal(anyhow::anyhow!("Redis TYPE 失败: {e}")))?;

        let ttl: i64 = conn.ttl(key).await.unwrap_or(-1);

        let mem_usage: i64 = redis::cmd("MEMORY")
            .arg("USAGE")
            .arg(key)
            .query_async(&mut conn)
            .await
            .unwrap_or(-1);
        let size = format_bytes(mem_usage);

        let encoding: String = redis::cmd("OBJECT")
            .arg("ENCODING")
            .arg(key)
            .query_async(&mut conn)
            .await
            .unwrap_or_else(|_| "unknown".to_string());

        let value = match key_type.as_str() {
            "string" => {
                let data: String = conn.get(key).await.unwrap_or_default();
                CacheKeyValue::String { data }
            }
            "hash" => {
                let total: u64 = redis::cmd("HLEN")
                    .arg(key)
                    .query_async(&mut conn)
                    .await
                    .unwrap_or(0);
                // 小数据量用 HGETALL，大数据量用 HSCAN
                let fields: Vec<(String, String)> = if total <= 100 {
                    redis::cmd("HGETALL")
                        .arg(key)
                        .query_async(&mut conn)
                        .await
                        .unwrap_or_default()
                } else {
                    let (_, pairs): (u64, Vec<(String, String)>) = redis::cmd("HSCAN")
                        .arg(key)
                        .arg(0u64)
                        .arg("COUNT")
                        .arg(100u64)
                        .query_async(&mut conn)
                        .await
                        .unwrap_or((0, vec![]));
                    pairs
                };
                let data = fields
                    .into_iter()
                    .map(|(field, value)| HashField { field, value })
                    .collect();
                CacheKeyValue::Hash { data }
            }
            "list" => {
                let total: u64 = redis::cmd("LLEN")
                    .arg(key)
                    .query_async(&mut conn)
                    .await
                    .unwrap_or(0);
                let data: Vec<String> = redis::cmd("LRANGE")
                    .arg(key)
                    .arg(0i64)
                    .arg(99i64)
                    .query_async(&mut conn)
                    .await
                    .unwrap_or_default();
                CacheKeyValue::List { data, total }
            }
            "set" => {
                let total: u64 = redis::cmd("SCARD")
                    .arg(key)
                    .query_async(&mut conn)
                    .await
                    .unwrap_or(0);
                let (_, data): (u64, Vec<String>) = redis::cmd("SSCAN")
                    .arg(key)
                    .arg(0u64)
                    .arg("COUNT")
                    .arg(100u64)
                    .query_async(&mut conn)
                    .await
                    .unwrap_or((0, vec![]));
                CacheKeyValue::Set { data, total }
            }
            "zset" => {
                let total: u64 = redis::cmd("ZCARD")
                    .arg(key)
                    .query_async(&mut conn)
                    .await
                    .unwrap_or(0);
                let raw: Vec<(String, f64)> = redis::cmd("ZREVRANGE")
                    .arg(key)
                    .arg(0i64)
                    .arg(99i64)
                    .arg("WITHSCORES")
                    .query_async(&mut conn)
                    .await
                    .unwrap_or_default();
                let data = raw
                    .into_iter()
                    .map(|(member, score)| ZSetMember { member, score })
                    .collect();
                CacheKeyValue::Zset { data, total }
            }
            "stream" => {
                let total: u64 = redis::cmd("XLEN")
                    .arg(key)
                    .query_async(&mut conn)
                    .await
                    .unwrap_or(0);
                // XREVRANGE key + - COUNT 100 → 最新 100 条，按 ID 降序
                let raw: Vec<(String, Vec<(String, String)>)> = redis::cmd("XREVRANGE")
                    .arg(key)
                    .arg("+")
                    .arg("-")
                    .arg("COUNT")
                    .arg(100u64)
                    .query_async(&mut conn)
                    .await
                    .unwrap_or_default();
                let data = raw
                    .into_iter()
                    .map(|(id, pairs)| StreamEntry {
                        id,
                        fields: pairs
                            .into_iter()
                            .map(|(field, value)| StreamField { field, value })
                            .collect(),
                    })
                    .collect();
                CacheKeyValue::Stream { data, total }
            }
            "vectorset" => {
                let total: u64 = redis::cmd("VCARD")
                    .arg(key)
                    .query_async(&mut conn)
                    .await
                    .unwrap_or(0);
                // VRANDMEMBER key count → 随机获取最多 100 个成员名称
                let data: Vec<String> = redis::cmd("VRANDMEMBER")
                    .arg(key)
                    .arg(100i64)
                    .query_async(&mut conn)
                    .await
                    .unwrap_or_default();
                CacheKeyValue::VectorSet { data, total }
            }
            _ => CacheKeyValue::String {
                data: format!("不支持的键类型: {key_type}"),
            },
        };

        Ok(CacheKeyDetailVo {
            key: key.to_string(),
            key_type,
            ttl,
            size,
            encoding,
            value,
        })
    }

    pub async fn delete_cache_key(&self, key: &str) -> ApiResult<()> {
        let mut conn = self.redis.clone();

        let deleted: i64 = conn
            .del(key)
            .await
            .map_err(|e| ApiErrors::Internal(anyhow::anyhow!("Redis DEL 失败: {e}")))?;

        if deleted > 0 {
            Ok(())
        } else {
            Err(ApiErrors::NotFound(format!("缓存键 '{key}' 不存在")))
        }
    }

    pub async fn delete_cache_keys_by_pattern(&self, query: CacheDeleteQuery) -> ApiResult<u64> {
        if query.pattern == "*" {
            return Err(ApiErrors::BadRequest(
                "不允许删除全部缓存键，请指定更精确的 pattern".to_string(),
            ));
        }

        let mut conn = self.redis.clone();
        let mut cursor: u64 = 0;
        let mut total_deleted: u64 = 0;

        loop {
            let (next_cursor, keys): (u64, Vec<String>) = redis::cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg(&query.pattern)
                .arg("COUNT")
                .arg(100u64)
                .query_async(&mut conn)
                .await
                .map_err(|e| ApiErrors::Internal(anyhow::anyhow!("Redis SCAN 失败: {e}")))?;

            if !keys.is_empty() {
                let deleted: u64 = redis::cmd("DEL")
                    .arg(&keys)
                    .query_async(&mut conn)
                    .await
                    .map_err(|e| ApiErrors::Internal(anyhow::anyhow!("Redis DEL 失败: {e}")))?;
                total_deleted += deleted;
            }

            cursor = next_cursor;
            if cursor == 0 {
                break;
            }
        }

        Ok(total_deleted)
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 辅助函数
// ═══════════════════════════════════════════════════════════════════════════════

/// 解析 Redis INFO 输出为 key-value 映射
fn parse_redis_info(info: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in info.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once(':') {
            map.insert(k.to_string(), v.to_string());
        }
    }
    map
}

fn get_str(map: &HashMap<String, String>, key: &str) -> String {
    map.get(key).cloned().unwrap_or_default()
}

fn parse_u64(map: &HashMap<String, String>, key: &str) -> u64 {
    map.get(key).and_then(|v| v.parse().ok()).unwrap_or(0)
}

fn parse_f64(map: &HashMap<String, String>, key: &str) -> f64 {
    map.get(key).and_then(|v| v.parse().ok()).unwrap_or(0.0)
}

/// 解析 keyspace 信息，返回 (total_keys, expires_keys, db_count)
fn parse_keyspace(map: &HashMap<String, String>) -> (u64, u64, u64) {
    let mut total_keys = 0u64;
    let mut expires_keys = 0u64;
    let mut db_count = 0u64;
    for (k, v) in map {
        if k.starts_with("db") && k[2..].chars().all(|c| c.is_ascii_digit()) {
            db_count += 1;
            for part in v.split(',') {
                if let Some(val) = part.strip_prefix("keys=") {
                    total_keys += val.parse::<u64>().unwrap_or(0);
                } else if let Some(val) = part.strip_prefix("expires=") {
                    expires_keys += val.parse::<u64>().unwrap_or(0);
                }
            }
        }
    }
    (total_keys, expires_keys, db_count)
}

/// 将字节数格式化为可读字符串
fn format_bytes(bytes: i64) -> String {
    if bytes < 0 {
        return "unknown".to_string();
    }
    let bytes = bytes as f64;
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;

    if bytes < KB {
        format!("{bytes}B")
    } else if bytes < MB {
        format!("{:.1}KB", bytes / KB)
    } else if bytes < GB {
        format!("{:.1}MB", bytes / MB)
    } else {
        format!("{:.1}GB", bytes / GB)
    }
}
