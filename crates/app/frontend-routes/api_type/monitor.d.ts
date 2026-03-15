declare namespace Api {
  /** 监控管理类型 */
  namespace Monitor {
    // ============================================================
    // 服务监控
    // ============================================================

    /** 服务器信息（对应后端 ServerInfoVo） */
    interface ServerInfoVo {
      cpu: CpuInfo
      memory: MemoryInfo
      disks: DiskInfo[]
      sys: SysInfo
      process: ProcessInfo
    }

    /** CPU 信息（对应后端 CpuInfo） */
    interface CpuInfo {
      /** 物理核心数 */
      physicalCoreCount: number
      /** 逻辑核心数 */
      logicalCoreCount: number
      /** CPU 总使用率（%） */
      usage: number
      /** CPU 型号 */
      modelName: string
      /** 每核使用率（%） */
      perCoreUsage: number[]
    }

    /** 内存信息（对应后端 MemoryInfo） */
    interface MemoryInfo {
      /** 总内存（字节） */
      total: number
      /** 已用内存（字节） */
      used: number
      /** 可用内存（字节） */
      available: number
      /** 内存使用率（%） */
      usage: number
      /** Swap 总量（字节） */
      swapTotal: number
      /** Swap 已用（字节） */
      swapUsed: number
    }

    /** 磁盘信息（对应后端 DiskInfo） */
    interface DiskInfo {
      /** 磁盘名称 */
      name: string
      /** 挂载点 */
      mountPoint: string
      /** 总空间（字节） */
      total: number
      /** 已用空间（字节） */
      used: number
      /** 可用空间（字节） */
      available: number
      /** 使用率（%） */
      usage: number
      /** 文件系统类型 */
      fsType: string
    }

    /** 系统信息（对应后端 SysInfo） */
    interface SysInfo {
      /** 操作系统名称 */
      osName: string
      /** 操作系统版本 */
      osVersion: string
      /** 内核版本 */
      kernelVersion: string
      /** 系统架构（x86_64 / aarch64） */
      arch: string
      /** 主机名 */
      hostName: string
      /** 系统运行时间（秒） */
      uptime: number
    }

    /** 进程信息（对应后端 ProcessInfo） */
    interface ProcessInfo {
      /** 当前进程 PID */
      pid: number
      /** 进程名称 */
      name: string
      /** 进程占用内存（字节） */
      memory: number
      /** 进程 CPU 使用率（%） */
      cpuUsage: number
      /** 进程运行时间（秒） */
      uptime: number
      /** 进程启动时间（UNIX 时间戳，秒） */
      startTime: number
    }

    // ============================================================
    // 缓存监控
    // ============================================================

    /** 缓存信息（对应后端 CacheInfoVo） */
    interface CacheInfoVo {
      // ─── 基础信息 ───
      version: string
      mode: string
      uptime: number
      /** TCP 端口 */
      tcpPort: number
      connectedClients: number
      /** 当前已用 DB 数量 */
      dbCount: number

      // ─── 内存 ───
      usedMemory: number
      usedMemoryHuman: string
      usedMemoryPeakHuman: string
      maxmemoryHuman: string
      /** 内存碎片率 */
      memFragmentationRatio: number

      // ─── 键空间 ───
      totalKeys: number
      expiresKeys: number

      // ─── 命中统计 ───
      keyspaceHits: number
      keyspaceMisses: number
      /** 命中率（%） */
      hitRate: number
      /** 每秒处理命令数 */
      instantaneousOpsPerSec: number

      // ─── 持久化 ───
      /** AOF 是否开启 */
      aofEnabled: boolean
      /** RDB 最近一次保存时间（UNIX 时间戳，秒；0 表示无） */
      rdbLastSaveTime: number

      // ─── 图表数据 ───
      /** 键类型分布 */
      keyTypeDistribution: KeyTypeCount[]
      /** 命中/未命中趋势（后端可选，可能为 null） */
      hitTrend: TrendData | null
      /** 内存使用趋势（后端可选，可能为 null） */
      memoryTrend: TrendData | null
      /** QPS 趋势（后端可选，可能为 null） */
      qpsTrend: QpsTrendData | null
    }

    /** 键类型计数（对应后端 KeyTypeCount） */
    interface KeyTypeCount {
      /** 类型名称: string, hash, list, set, zset */
      name: string
      /** 该类型的键数量 */
      value: number
    }

    /** 趋势数据（对应后端 TrendData） */
    interface TrendData {
      /** 时间标签，如 ["08:00", "09:00", ...] */
      labels: string[]
      /** 数据系列 */
      series: TrendSeries[]
    }

    /** 趋势数据系列（对应后端 TrendSeries） */
    interface TrendSeries {
      /** 系列名称，如 "命中"、"未命中" */
      name: string
      /** 数据值 */
      data: number[]
    }

    /** QPS 趋势数据（对应后端 QpsTrendData） */
    interface QpsTrendData {
      /** 日期标签 */
      labels: string[]
      /** 每个时间点的平均 QPS */
      data: number[]
    }

    // ─── 缓存键列表 ───

    /** 缓存键列表（对应后端 CacheKeysVo） */
    interface CacheKeysVo {
      keys: CacheKeyItem[]
      nextCursor: number
    }

    /** 缓存键项（对应后端 CacheKeyItem） */
    interface CacheKeyItem {
      key: string
      ttl: number
      keyType: string
      /** 序列化大小（可读格式，如 "256B", "1.2KB"） */
      size: string
      /** 内部编码（embstr/raw/int/listpack/quicklist/skiplist...） */
      encoding: string
    }

    // ─── 缓存键详情 ───

    /** 缓存键详情（对应后端 CacheKeyDetailVo） */
    interface CacheKeyDetailVo {
      /** 键名 */
      key: string
      /** 键类型: string, hash, list, set, zset, stream, vectorset */
      keyType: string
      /** 剩余过期时间（秒），-1 = 永不过期 */
      ttl: number
      /** 序列化大小（可读格式） */
      size: string
      /** 内部编码 */
      encoding: string
      /** 值内容（根据类型不同，结构不同） */
      value: CacheKeyValue
    }

    /** 缓存键值 - 带 type 鉴别标签（对应后端 CacheKeyValue 枚举） */
    type CacheKeyValue =
      | { type: 'string'; data: string }
      | { type: 'hash'; data: HashField[] }
      | { type: 'list'; data: string[]; total: number }
      | { type: 'set'; data: string[]; total: number }
      | { type: 'zset'; data: ZSetMember[]; total: number }
      | { type: 'stream'; data: StreamEntry[]; total: number }
      | { type: 'vectorSet'; data: string[]; total: number }

    /** Hash 字段（对应后端 HashField） */
    interface HashField {
      field: string
      value: string
    }

    /** ZSet 成员（对应后端 ZSetMember） */
    interface ZSetMember {
      member: string
      score: number
    }

    /** Stream 消息条目（对应后端 StreamEntry） */
    interface StreamEntry {
      /** 消息 ID（如 "1678886400000-0"） */
      id: string
      /** 消息字段键值对 */
      fields: StreamField[]
    }

    /** Stream 消息字段（对应后端 StreamField） */
    interface StreamField {
      field: string
      value: string
    }

    // ─── 查询参数 ───

    /** 缓存键列表查询参数（对应后端 CacheKeysQuery） */
    interface CacheKeysQuery {
      /** 匹配模式，默认 "*" */
      pattern?: string
      /** 游标，默认 0 */
      cursor?: number
      /** 每次扫描数量，默认 20 */
      count?: number
    }

    /** 缓存批量删除查询参数（对应后端 CacheDeleteQuery） */
    interface CacheDeleteQuery {
      pattern: string
    }
  }
}
