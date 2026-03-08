# 缓存监控页面 - 后端接口需求文档

> 前端页面：`/monitor/cache`
> 前端文件：`src/views/monitor/cache/index.vue` + `modules/cache-keys.vue`
> 现有API：`src/api/monitor.ts`

---

## 一、现有接口需要扩展的字段

### 1.1 `GET /api/monitor/cache/info` → `CacheInfoVo`

现有字段（**已实现，保持不变**）：

| 字段 | 类型 | 说明 | Redis 命令 |
|------|------|------|-----------|
| `version` | `string` | Redis 版本 | `INFO server` → `redis_version` |
| `mode` | `string` | 运行模式 (standalone/cluster/sentinel) | `INFO server` → `redis_mode` |
| `uptime` | `u64` | 运行时间（秒） | `INFO server` → `uptime_in_seconds` |
| `connectedClients` | `u64` | 连接客户端数 | `INFO clients` → `connected_clients` |
| `usedMemory` | `u64` | 已用内存（字节） | `INFO memory` → `used_memory` |
| `usedMemoryHuman` | `string` | 已用内存（可读） | `INFO memory` → `used_memory_human` |
| `usedMemoryPeakHuman` | `string` | 内存峰值（可读） | `INFO memory` → `used_memory_peak_human` |
| `maxmemoryHuman` | `string` | 最大内存限制（可读） | `INFO memory` → `maxmemory_human` |
| `totalKeys` | `u64` | 总键数 | `INFO keyspace` 累加 |
| `expiresKeys` | `u64` | 过期键数 | `INFO keyspace` 累加 |
| `keyspaceHits` | `u64` | 命中次数 | `INFO stats` → `keyspace_hits` |
| `keyspaceMisses` | `u64` | 未命中次数 | `INFO stats` → `keyspace_misses` |
| `hitRate` | `f64` | 命中率（%） | 计算: hits / (hits + misses) * 100 |
| `aofEnabled` | `bool` | AOF 是否开启 | `INFO persistence` → `aof_enabled` |
| `rdbLastSaveTime` | `u64` | RDB 最近保存时间戳（秒） | `INFO persistence` → `rdb_last_save_time` |
| `dbCount` | `u64` | 已使用 DB 数量 | `INFO keyspace` → db 数量 |

**需要新增的字段**：

| 字段 | 类型 | 说明 | Redis 命令 |
|------|------|------|-----------|
| `tcpPort` | `u16` | TCP 端口 | `INFO server` → `tcp_port` |
| `memFragmentationRatio` | `f64` | 内存碎片率 | `INFO memory` → `mem_fragmentation_ratio` |
| `instantaneousOpsPerSec` | `u64` | 每秒处理命令数 | `INFO stats` → `instantaneous_ops_per_sec` |
| `keyTypeDistribution` | `Vec<KeyTypeCount>` | 键类型分布 | 见下方说明 |
| `hitTrend` | `TrendData` | 命中/未命中趋势（12个时间点） | 需后端定时采集或计算 |
| `memoryTrend` | `TrendData` | 内存使用趋势（12个时间点） | 需后端定时采集或计算 |
| `qpsTrend` | `QpsTrendData` | QPS 趋势（7天） | 需后端定时采集或计算 |

---

### 1.2 新增子结构体

```rust
/// 键类型计数
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct KeyTypeCount {
    /// 类型名称: string, hash, list, set, zset
    pub name: String,
    /// 该类型的键数量
    pub value: u64,
}

/// 趋势数据（用于折线图）
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TrendData {
    /// 时间标签，如 ["08:00", "09:00", ...]
    pub labels: Vec<String>,
    /// 数据系列
    pub series: Vec<TrendSeries>,
}

/// 趋势数据系列
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TrendSeries {
    /// 系列名称，如 "命中"、"未命中"
    pub name: String,
    /// 数据值
    pub data: Vec<f64>,
}

/// QPS 趋势数据
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct QpsTrendData {
    /// 日期标签，如 ["周一", "周二", ...] 或 ["03-01", "03-02", ...]
    pub labels: Vec<String>,
    /// 每个时间点的平均 QPS
    pub data: Vec<u64>,
}
```

**键类型分布获取方式**：

遍历所有 db，对每个 key 使用 `TYPE` 命令，或者使用 `SCAN` 配合 `TYPE` 批量获取。如果键量较大，可考虑采样统计。

**趋势数据获取方式**：

推荐两种方案：
- **方案 A（简单）**：后端启动一个定时任务，每小时采集一次 `INFO stats` 中的 `keyspace_hits`、`keyspace_misses`、`used_memory`、`instantaneous_ops_per_sec`，存到内存环形缓冲区中。接口直接返回最近 12 个点。
- **方案 B（轻量）**：前端调用 `/cache/info` 后自行在内存中累积趋势数据。不需要后端额外存储。（当前模拟数据就是这个思路，但刷新后数据会丢失）

如果选择方案 A，trend 字段就放在 `CacheInfoVo` 里；如果选择方案 B，则 trend 相关字段可以不加，前端自行处理。

---

## 二、`CacheKeyItem` 需要扩展的字段

### 2.1 `GET /api/monitor/cache/keys` → `CacheKeysVo`

现有 `CacheKeyItem`：

| 字段 | 类型 | 说明 |
|------|------|------|
| `key` | `string` | 键名 |
| `ttl` | `i64` | 剩余过期时间（秒），-1 = 永不过期，-2 = 已过期 |
| `keyType` | `string` | 键类型 (string/hash/list/set/zset) |

**需要新增的字段**：

| 字段 | 类型 | 说明 | Redis 命令 |
|------|------|------|-----------|
| `size` | `string` | 序列化大小（可读格式，如 "256B", "1.2KB"） | `MEMORY USAGE <key>` → 转可读 |
| `encoding` | `string` | 内部编码（embstr/raw/int/listpack/quicklist/skiplist...） | `OBJECT ENCODING <key>` |

> 注意：`MEMORY USAGE` 在键量很大时可能有性能影响，建议在 SCAN 阶段就获取，或者仅在查看详情时获取。

---

## 三、新增接口：查看缓存键详情

### 3.1 `GET /api/monitor/cache/keys/:key/value` → `CacheKeyDetailVo`

**路径参数**：`key` — URL 编码的键名

**功能**：根据键的类型，返回对应结构的值

```rust
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
```

**各类型对应的 Redis 命令**：

| 键类型 | 获取值命令 | 获取总数命令 |
|--------|-----------|-------------|
| `string` | `GET <key>` | — |
| `hash` | `HSCAN <key> 0 COUNT 100` 或 `HGETALL`（小数据量时） | `HLEN <key>` |
| `list` | `LRANGE <key> 0 99` | `LLEN <key>` |
| `set` | `SSCAN <key> 0 COUNT 100` | `SCARD <key>` |
| `zset` | `ZREVRANGE <key> 0 99 WITHSCORES` | `ZCARD <key>` |

> 建议限制返回条数（如前 100 条），避免大 key 导致响应过大。

---

## 四、完整接口清单

| # | 方法 | 路径 | 说明 | 状态 |
|---|------|------|------|------|
| 1 | `GET` | `/api/monitor/cache/info` | 获取 Redis 总览信息 + 图表数据 | **已有，需扩展** |
| 2 | `GET` | `/api/monitor/cache/keys` | 获取缓存键列表（游标分页） | **已有，需扩展** |
| 3 | `GET` | `/api/monitor/cache/keys/:key/value` | 获取指定键的详情和值 | **新增** |
| 4 | `DELETE` | `/api/monitor/cache/keys/:key` | 删除指定缓存键 | **已有，不变** |
| 5 | `DELETE` | `/api/monitor/cache/keys?pattern=xxx` | 批量删除匹配的缓存键 | **已有，不变** |

---

## 五、前端页面各区域与接口字段的对应关系

### 5.1 顶部统计卡片（4 个）

| 卡片 | 字段 |
|------|------|
| 总键数 | `totalKeys`, `expiresKeys` |
| 命中次数 | `keyspaceHits` |
| 内存使用 | `usedMemoryHuman`, `usedMemoryPeakHuman` |
| 连接客户端 | `connectedClients` |

### 5.2 命中率趋势折线图

| 数据 | 字段 |
|------|------|
| X 轴标签 | `hitTrend.labels` |
| 命中线 | `hitTrend.series[0].data`（name="命中"） |
| 未命中线 | `hitTrend.series[1].data`（name="未命中"） |
| 右上角命中率 | `hitRate` |

### 5.3 内存使用趋势折线图

| 数据 | 字段 |
|------|------|
| X 轴标签 | `memoryTrend.labels` |
| 内存值 | `memoryTrend.series[0].data` |
| 右上角标签 | `usedMemoryHuman` / `maxmemoryHuman` |

### 5.4 命中率环形图

| 数据 | 字段 |
|------|------|
| 命中数 | `keyspaceHits` |
| 未命中数 | `keyspaceMisses` |
| 命中率 | `hitRate` |

### 5.5 键类型分布饼图

| 数据 | 字段 |
|------|------|
| 每种类型名称+数量 | `keyTypeDistribution[].name`, `keyTypeDistribution[].value` |

### 5.6 每秒命令执行数柱状图

| 数据 | 字段 |
|------|------|
| X 轴标签 | `qpsTrend.labels` |
| 柱状图数据 | `qpsTrend.data` |

### 5.7 Redis 信息面板

| 展示项 | 字段 |
|--------|------|
| Redis 版本 | `version` |
| 运行模式 | `mode` |
| 运行时间 | `uptime`（前端格式化为 "X 天 X 时 X 分"） |
| 连接客户端数 | `connectedClients` |
| 已用 DB 数量 | `dbCount` |
| TCP 端口 | `tcpPort` |

### 5.8 内存信息面板

| 展示项 | 字段 |
|--------|------|
| 已用内存 | `usedMemoryHuman` |
| 内存峰值 | `usedMemoryPeakHuman` |
| 最大内存限制 | `maxmemoryHuman`（空或"0B"时显示"无限制"） |
| 内存碎片率 | `memFragmentationRatio` |
| 总键数 | `totalKeys` |
| 过期键数 | `expiresKeys` |

### 5.9 持久化 & 统计面板

| 展示项 | 字段 |
|--------|------|
| AOF 持久化 | `aofEnabled` |
| RDB 最近保存 | `rdbLastSaveTime`（前端转日期显示，0 显示"无"） |
| 命中次数 | `keyspaceHits` |
| 未命中次数 | `keyspaceMisses` |
| 命中率 | `hitRate` |
| 每秒处理命令 | `instantaneousOpsPerSec` |

### 5.10 缓存键列表

| 列 | `CacheKeyItem` 字段 |
|----|---------------------|
| 键名 | `key` |
| TTL | `ttl` |
| 类型 | `keyType` |
| 大小 | `size` |

### 5.11 缓存键详情弹窗

| 展示项 | `CacheKeyDetailVo` 字段 |
|--------|------------------------|
| 键名 | `key` |
| 类型 | `keyType` |
| 大小 | `size` |
| TTL | `ttl` |
| 编码 | `encoding` |
| 值内容 | `value`（按 type 分发展示） |

---

## 六、完整的 `CacheInfoVo` 结构体（最终版）

```rust
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
    pub hit_trend: Option<TrendData>,
    pub memory_trend: Option<TrendData>,
    pub qps_trend: Option<QpsTrendData>,
}
```

> `hit_trend`、`memory_trend`、`qps_trend` 设为 `Option`。如果后端暂不实现定时采集，返回 `null` 即可，前端会自行处理空状态。
