# Summer-Sharding 分库分表设计文档

> 参考 Java ShardingSphere、Go Gaea/Vitess 的成熟分片方案，为 Rust + SeaORM + PostgreSQL 打造的分库分表中间件。

---

## 一、背景与目标

### 1.1 现状

summerrs-admin 目前采用 **单库多 Schema** 架构：

| Schema | 职责 | 表数量 |
|--------|------|--------|
| `sys`  | 系统管理（用户、角色、菜单、配置、日志） | ~20 |
| `biz`  | 业务域（业务用户、客户） | ~5 |
| `ai`   | AI Gateway 全量表（渠道、令牌、请求、日志、组织、治理） | 80+ |

单库方案在业务初期足够，但随着 AI 请求量增长，`ai.log`、`ai.request`、`ai.request_execution`、`ai.trace_span` 等高写入表将成为瓶颈。同时，AI Gateway 作为 SaaS 服务，必须支持**多租户隔离**。

### 1.2 目标

| 优先级 | 目标 | 说明 |
|--------|------|------|
| P0 | **Schema 路由** | 透明地将 SQL 路由到 sys/biz/ai 对应的数据源 |
| P0 | **分表** | 对高写入表按时间或 hash 水平切分（如 `ai.log_202603`） |
| P0 | **多租户隔离** | 支持行级/表级/Schema级/库级四种租户隔离模式 |
| P1 | **读写分离** | 写走主库，读走从库 |
| P1 | **分布式 ID** | Snowflake / TSID 全局唯一 ID 生成 |
| P1 | **绑定表 & Hint 路由** | 同分片键表关联查询 + 强制路由 |
| P2 | **分库** | 跨物理库的水平拆分 |
| P2 | **跨分片查询** | 自动扇出查询 + 结果归并 |
| P2 | **数据加密 & 脱敏** | 列级透明加解密 |
| P3 | **分布式事务** | 柔性事务（SAGA / 本地消息表） |
| P3 | **弹性伸缩** | 在线扩缩分片 + 数据迁移 |
| P3 | **影子库** | 全链路压测 |

### 1.3 设计原则

1. **对业务代码零侵入** — 上层 SeaORM 代码不感知分片存在
2. **可插拔策略** — 分片算法、路由规则、ID 生成器均可自定义
3. **渐进式采用** — 可单独使用 Schema 路由而不启用分表
4. **SQL 兼容** — 基于 sqlparser 做 SQL 改写，而非字符串拼接
5. **多租户优先** — 租户隔离是一等公民，不是事后补丁

---

## 二、核心概念（对齐 ShardingSphere 术语）

```
┌─────────────────────────────────────────────────────────┐
│                    ShardingDataSource                     │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────┐  │
│  │ LogicTable   │  │ ShardingRule│  │ ShardingAlgorithm│ │
│  │  ai.log      │  │  time_range │  │  mod / range     │ │
│  └─────────────┘  └─────────────┘  └─────────────────┘  │
│                                                           │
│  ┌─────────────────────────────────────────────────────┐  │
│  │              SQL Pipeline                            │ │
│  │  Parse → Analyze → Route → Rewrite → Execute → Merge│ │
│  └─────────────────────────────────────────────────────┘  │
│                                                           │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐               │
│  │ DataSource│  │ DataSource│  │ DataSource│  ...         │
│  │  ds_sys   │  │  ds_ai_0 │  │  ds_ai_1 │               │
│  └──────────┘  └──────────┘  └──────────┘               │
└─────────────────────────────────────────────────────────┘
```

| 概念 | Java 对应 | 说明 |
|------|----------|------|
| **DataSourcePool** | ShardingSphereDataSource | 管理多个物理数据源连接池 |
| **LogicTable** | LogicTable | 逻辑表名，业务代码使用的表名 |
| **ActualTable** | ActualTable | 实际物理表名（如 `log_202603`） |
| **ShardingRule** | ShardingRule | 描述一张表的分片规则 |
| **ShardingStrategy** | ShardingStrategy | 分库策略 + 分表策略的组合 |
| **ShardingAlgorithm** | ShardingAlgorithm | 具体的分片计算逻辑（hash / range / time） |
| **ShardingKey** | ShardingColumn | 参与分片计算的列 |
| **KeyGenerator** | KeyGenerateAlgorithm | 分布式 ID 生成策略 |
| **SqlRouter** | SQLRouter | SQL 路由引擎 |
| **SqlRewriter** | SQLRewriteEngine | SQL 改写引擎 |
| **ResultMerger** | MergeEngine | 多分片结果归并引擎 |

---

## 三、架构设计

### 3.1 分层架构

```
                      ┌─────────────────────┐
                      │   SeaORM 业务代码    │  ← 零侵入
                      └─────────┬───────────┘
                                │
                      ┌─────────▼───────────┐
                      │  ShardingConnector   │  ← 实现 SeaORM ConnectionTrait
                      │  (代理层)            │
                      └─────────┬───────────┘
                                │
              ┌─────────────────┼─────────────────┐
              │                 │                   │
    ┌─────────▼──────┐ ┌───────▼────────┐ ┌───────▼────────┐
    │  SQL Parser     │ │  SQL Router     │ │  SQL Rewriter   │
    │  (sqlparser-rs) │ │  (路由引擎)     │ │  (改写引擎)     │
    └────────────────┘ └───────┬────────┘ └────────────────┘
                               │
                     ┌─────────▼───────────┐
                     │  Executor            │
                     │  (执行引擎)          │
                     └─────────┬───────────┘
                               │
              ┌────────────────┼────────────────┐
              │                │                  │
    ┌─────────▼──────┐ ┌──────▼───────┐ ┌───────▼──────┐
    │  DataSource 0   │ │ DataSource 1  │ │ DataSource N  │
    │  (主库-sys)     │ │ (主库-ai)     │ │ (从库-ai)     │
    └────────────────┘ └──────────────┘ └──────────────┘
                               │
                     ┌─────────▼───────────┐
                     │  ResultMerger       │
                     │  (结果归并)          │
                     └─────────────────────┘
```

### 3.2 模块划分

```
summer-sharding/
├── Cargo.toml
├── DESIGN.md                          ← 本文档
└── src/
    ├── lib.rs                         ← 公开 API 导出
    │
    ├── config/                        ← 配置层
    │   ├── mod.rs
    │   ├── rule.rs                    ← ShardingRuleConfig（TOML 配置反序列化）
    │   ├── datasource.rs             ← DataSourceConfig
    │   └── tenant.rs                 ← TenantConfig（多租户配置）
    │
    ├── algorithm/                     ← 分片算法
    │   ├── mod.rs                     ← ShardingAlgorithm trait
    │   ├── hash_mod.rs               ← 取模算法：value % shard_count
    │   ├── time_range.rs             ← 时间范围：按月/按日分表
    │   ├── hash_range.rs             ← 一致性哈希
    │   ├── tenant.rs                 ← 租户分片算法（按 tenant_id 路由到表/schema/库）
    │   └── complex.rs                ← 复合分片（租户 + 时间 二维分片）
    │
    ├── tenant/                        ← 多租户核心
    │   ├── mod.rs                     ← TenantContext, TenantIsolationLevel
    │   ├── context.rs                ← 租户上下文管理（task-local / middleware）
    │   ├── router.rs                 ← TenantRouter（根据 isolation_level 路由）
    │   ├── rewrite.rs                ← 租户级 SQL 改写（注入 tenant_id 条件）
    │   ├── rls.rs                    ← PostgreSQL RLS 策略管理
    │   ├── lifecycle.rs              ← 租户 onboard/offboard（创建/删除 schema/db）
    │   └── metadata.rs               ← 租户元数据存储（sys.tenant_config 表操作）
    │
    ├── keygen/                        ← 分布式 ID 生成
    │   ├── mod.rs                     ← KeyGenerator trait
    │   ├── snowflake.rs              ← Snowflake 算法
    │   └── tsid.rs                   ← TSID（Time-Sorted ID）
    │
    ├── router/                        ← SQL 路由
    │   ├── mod.rs                     ← SqlRouter trait
    │   ├── schema_router.rs          ← Schema 级路由（sys/biz/ai → 数据源）
    │   ├── table_router.rs           ← 表级路由（逻辑表 → 物理表）
    │   ├── rw_router.rs              ← 读写分离路由
    │   └── hint_router.rs            ← Hint 强制路由
    │
    ├── rewrite/                       ← SQL 改写
    │   ├── mod.rs                     ← SqlRewriter trait
    │   ├── table_rewrite.rs          ← 表名替换（log → log_202603）
    │   ├── schema_rewrite.rs         ← Schema 名注入
    │   ├── limit_rewrite.rs          ← LIMIT/OFFSET 膨胀（多分片查询）
    │   └── encrypt_rewrite.rs        ← 加密列改写（INSERT 加密 / SELECT 解密）
    │
    ├── execute/                       ← 执行引擎
    │   ├── mod.rs                     ← Executor trait
    │   ├── simple.rs                 ← 单分片执行
    │   └── scatter_gather.rs         ← 扇出执行 + 结果收集
    │
    ├── merge/                         ← 结果归并
    │   ├── mod.rs                     ← ResultMerger trait
    │   ├── order_by.rs               ← ORDER BY 归并（归并排序）
    │   ├── group_by.rs               ← GROUP BY 聚合归并
    │   ├── limit.rs                  ← LIMIT/OFFSET 归并
    │   └── stream.rs                 ← 流式归并迭代器
    │
    ├── datasource/                    ← 数据源管理
    │   ├── mod.rs                     ← DataSourcePool
    │   ├── pool.rs                   ← 连接池封装（基于 SeaORM DatabaseConnection）
    │   ├── health.rs                 ← 健康检查 / 故障转移
    │   └── discovery.rs              ← 数据源自动发现（主从拓扑检测）
    │
    ├── encrypt/                       ← 数据加密（列级透明加解密）
    │   ├── mod.rs                     ← EncryptAlgorithm trait
    │   ├── aes.rs                    ← AES 对称加密
    │   └── digest.rs                 ← 摘要/哈希（用于密文查询列）
    │
    ├── masking/                       ← 数据脱敏（读取时动态遮蔽）
    │   ├── mod.rs                     ← MaskingAlgorithm trait
    │   ├── phone.rs                  ← 手机号脱敏 138****1234
    │   ├── email.rs                  ← 邮箱脱敏 u***@example.com
    │   ├── ip.rs                     ← IP 脱敏 192.168.*.*
    │   └── partial.rs                ← 通用部分遮蔽
    │
    ├── connector/                     ← SeaORM 集成层
    │   ├── mod.rs
    │   ├── connection.rs             ← ShardingConnection：impl ConnectionTrait
    │   ├── transaction.rs            ← 分布式事务协调
    │   ├── statement.rs              ← Statement 拦截与转换
    │   └── hint.rs                   ← ShardingHint API
    │
    ├── audit/                         ← SQL 审计
    │   ├── mod.rs                     ← SqlAuditor trait
    │   └── log.rs                    ← 审计日志（慢查询、全扇出告警）
    │
    ├── migration/                     ← 分片生命周期管理
    │   ├── mod.rs
    │   ├── auto_table.rs             ← 自动建表（时间分表 pre-create）
    │   ├── archive.rs                ← 历史分片归档 / 清理
    │   └── resharding.rs             ← 在线扩缩分片（数据迁移）
    │
    ├── shadow/                        ← 影子库（全链路压测）
    │   ├── mod.rs                     ← ShadowRouter（压测流量检测 + 路由）
    │   └── condition.rs              ← 压测标记检测（Header / Column / Hint）
    │
    ├── ddl/                           ← Online DDL（不停机 Schema 变更）
    │   ├── mod.rs                     ← OnlineDdlEngine trait
    │   ├── ghost.rs                  ← Ghost Table 策略（影子表 + 原子切换）
    │   └── scheduler.rs              ← 多分片 DDL 并行编排
    │
    ├── cdc/                           ← CDC 数据迁移
    │   ├── mod.rs                     ← CdcSource / CdcSink trait
    │   ├── pg_source.rs              ← PostgreSQL 逻辑复制源
    │   ├── table_sink.rs             ← 目标表写入
    │   ├── transformer.rs            ← 行转换器（重分片键计算）
    │   └── pipeline.rs               ← 三阶段编排（Snapshot → Catch-up → Cutover）
    │
    └── error.rs                       ← 统一错误类型
```

---

## 四、核心 Trait 设计

### 4.1 ShardingAlgorithm — 分片算法

```rust
/// 分片算法 trait
/// 对标 ShardingSphere 的 ShardingAlgorithm
pub trait ShardingAlgorithm: Send + Sync + 'static {
    /// 精确分片：INSERT / 等值 WHERE 条件
    /// 输入分片键的值，返回目标分片名
    fn do_sharding(
        &self,
        available_targets: &[String],  // 可用的物理表/库名列表
        sharding_value: &ShardingValue, // 分片键值
    ) -> Vec<String>;                   // 命中的目标

    /// 范围分片：BETWEEN / >, < 条件
    fn do_range_sharding(
        &self,
        available_targets: &[String],
        lower: &ShardingValue,
        upper: &ShardingValue,
    ) -> Vec<String>;

    /// 算法类型标识
    fn algorithm_type(&self) -> &str;
}

/// 分片键值
pub enum ShardingValue {
    Int(i64),
    Str(String),
    DateTime(chrono::DateTime<chrono::FixedOffset>),
    Null,
}
```

### 4.2 ShardingRule — 分片规则配置

```rust
/// 一张逻辑表的完整分片规则
pub struct TableRule {
    /// 逻辑表名（业务代码中使用的表名）
    pub logic_table: String,
    /// 物理表表达式，如 "log_${202601..202612}"
    pub actual_tables: Vec<String>,
    /// 分库策略（可选）
    pub database_strategy: Option<ShardingStrategyConfig>,
    /// 分表策略
    pub table_strategy: ShardingStrategyConfig,
    /// 分布式 ID 生成策略（可选）
    pub key_generator: Option<KeyGeneratorConfig>,
}

/// 分片策略配置
pub struct ShardingStrategyConfig {
    /// 分片键列名
    pub sharding_column: String,
    /// 算法名称 → 对应注册的 ShardingAlgorithm 实例
    pub algorithm_name: String,
}
```

### 4.3 KeyGenerator — 分布式 ID

```rust
/// 分布式 ID 生成器
/// 对标 ShardingSphere 的 KeyGenerateAlgorithm
pub trait KeyGenerator: Send + Sync + 'static {
    /// 生成下一个全局唯一 ID
    fn next_id(&self) -> i64;
    /// 生成器类型
    fn generator_type(&self) -> &str;
}
```

### 4.4 ShardingConnection — SeaORM 代理连接

```rust
/// 分片代理连接，实现 SeaORM 的 ConnectionTrait
/// 业务代码感知不到分片，直接用这个连接操作即可
pub struct ShardingConnection {
    /// 路由规则
    rules: Arc<ShardingRuleConfig>,
    /// 物理数据源池
    pool: Arc<DataSourcePool>,
    /// SQL 路由器
    router: Arc<dyn SqlRouter>,
    /// SQL 改写器
    rewriter: Arc<dyn SqlRewriter>,
    /// 执行器
    executor: Arc<dyn Executor>,
    /// 归并器
    merger: Arc<dyn ResultMerger>,
}

/// 实现 SeaORM ConnectionTrait，对上层透明
#[async_trait]
impl ConnectionTrait for ShardingConnection {
    // 拦截所有 SQL 执行
    // 1. Parse：解析 SQL 提取表名、条件
    // 2. Route：根据规则确定目标分片
    // 3. Rewrite：改写 SQL 中的表名
    // 4. Execute：发送到目标数据源
    // 5. Merge：归并多分片结果
    async fn execute(&self, stmt: Statement) -> Result<ExecResult, DbErr> { ... }
    async fn query_one(&self, stmt: Statement) -> Result<Option<QueryResult>, DbErr> { ... }
    async fn query_all(&self, stmt: Statement) -> Result<Vec<QueryResult>, DbErr> { ... }
}
```

---

## 五、分片策略详解

### 5.1 时间范围分表（核心场景）

适用于 `ai.log`、`ai.request`、`ai.trace_span` 等按时间增长的大表。

```
逻辑表: ai.log
分片键: create_time
算法:   time_range(granularity = "month")

物理表:
  ai.log_202601
  ai.log_202602
  ai.log_202603  ← current
  ...

SQL 改写示例:
  原始: SELECT * FROM ai.log WHERE create_time >= '2026-02-01' AND create_time < '2026-04-01'
  改写: SELECT * FROM ai.log_202602 WHERE create_time >= '2026-02-01' AND create_time < '2026-03-01'
        UNION ALL
        SELECT * FROM ai.log_202603 WHERE create_time >= '2026-03-01' AND create_time < '2026-04-01'
```

**自动建表**：时间分表需要定时任务提前创建下个周期的物理表。

### 5.2 Hash 取模分表

适用于按用户维度查询的高并发表。

```
逻辑表: ai.token
分片键: user_id
算法:   hash_mod(count = 4)

物理表:
  ai.token_0  (user_id % 4 == 0)
  ai.token_1  (user_id % 4 == 1)
  ai.token_2  (user_id % 4 == 2)
  ai.token_3  (user_id % 4 == 3)
```

### 5.3 Schema 路由（已有需求）

当前项目已经有 sys/biz/ai 三个 Schema，这是最基础的"分库"形式。

```
路由规则:
  sys.* → ds_sys (或同库不同 schema)
  biz.* → ds_biz
  ai.*  → ds_ai

未来扩展: ai.* 可进一步拆到独立物理库
```

### 5.4 读写分离

```
写: ai.log INSERT/UPDATE/DELETE → ds_ai_primary
读: ai.log SELECT              → ds_ai_replica_0 / ds_ai_replica_1 (轮询/权重)

事务内读写:
  开启事务后，所有读写均走主库（避免主从延迟导致脏读）
```

### 5.5 多租户分片（重点章节）

多租户是 SaaS 产品的核心需求。我们提供**四种隔离级别**，可按租户规模混合使用。

#### Level 1：共享表 + 租户列（Row-Level Isolation）

**原理**：所有租户数据在同一张表，通过 `tenant_id` 列区分。

```
┌─────────────────────────────────────┐
│           ai.log (共享表)            │
│                                      │
│  id │ tenant_id │ data │ create_time │
│  1  │ T-001     │ ...  │ ...         │
│  2  │ T-002     │ ...  │ ...         │
│  3  │ T-001     │ ...  │ ...         │
│  4  │ T-003     │ ...  │ ...         │
└─────────────────────────────────────┘

所有 SQL 自动注入: WHERE tenant_id = ?
  原始: SELECT * FROM ai.log WHERE status = 1
  改写: SELECT * FROM ai.log WHERE status = 1 AND tenant_id = 'T-001'
```

**实现方式**：
- **方案 A — SQL 改写注入**：ShardingConnection 在 SQL Pipeline 的 Rewrite 阶段自动追加 `AND tenant_id = ?`
- **方案 B — PostgreSQL RLS**：利用数据库原生 Row-Level Security，通过 `SET app.current_tenant = 'T-001'` 设置会话变量

```sql
-- PostgreSQL RLS 策略
ALTER TABLE ai.log ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON ai.log
  USING (tenant_id = current_setting('app.current_tenant'));
```

**适用场景**：小型租户、租户数量多（数千~数万）、数据量可控
**优点**：实现简单、资源利用率高、维护成本低
**缺点**：单表数据量有上限、租户间无物理隔离、一个慢查询影响所有租户

---

#### Level 2：租户独立表（Table-Level Isolation）

**原理**：每个租户一张独立表，通过 tenant_id 路由。

```
┌────────────────┐  ┌────────────────┐  ┌────────────────┐
│ ai.log_t001    │  │ ai.log_t002    │  │ ai.log_t003    │
│ (租户 T-001)   │  │ (租户 T-002)   │  │ (租户 T-003)   │
│                │  │                │  │                │
│ id │ data      │  │ id │ data      │  │ id │ data      │
│ 1  │ ...       │  │ 1  │ ...       │  │ 1  │ ...       │
│ 3  │ ...       │  │ 2  │ ...       │  │ 4  │ ...       │
└────────────────┘  └────────────────┘  └────────────────┘

路由:
  tenant_id = T-001 → ai.log_t001
  tenant_id = T-002 → ai.log_t002

SQL 改写:
  原始: INSERT INTO ai.log (data) VALUES ('hello')
  改写: INSERT INTO ai.log_t001 (data) VALUES ('hello')  -- 从上下文获取 tenant_id
```

**适用场景**：中型租户、需要一定隔离性、单租户数据量较大
**优点**：物理隔离、可独立维护索引、可独立备份/归档
**缺点**：DDL 变更需要批量操作所有租户表、表数量随租户线性增长

---

#### Level 3：租户独立 Schema（Schema-Level Isolation）

**原理**：每个租户分配一个独立的 PostgreSQL Schema。

```
┌─────────────────────────────────────────────────┐
│              同一个物理数据库                      │
│                                                   │
│  ┌─────────────┐  ┌─────────────┐  ┌──────────┐  │
│  │ tenant_001  │  │ tenant_002  │  │ tenant_N │  │
│  │   .log      │  │   .log      │  │   .log   │  │
│  │   .request  │  │   .request  │  │   .req.. │  │
│  │   .token    │  │   .token    │  │   .token │  │
│  └─────────────┘  └─────────────┘  └──────────┘  │
│                                                   │
│  ┌─────────────────────────────────┐              │
│  │ shared (共享 Schema)            │              │
│  │   .model_config  .vendor        │              │
│  │   .channel       .routing_rule  │              │
│  └─────────────────────────────────┘              │
└─────────────────────────────────────────────────┘

路由:
  TenantContext(T-001) → search_path = tenant_001, shared
  TenantContext(T-002) → search_path = tenant_002, shared

SQL 改写:
  原始: SELECT * FROM log WHERE id = 1
  改写: SET search_path TO tenant_001, shared; SELECT * FROM log WHERE id = 1
  或者: SELECT * FROM tenant_001.log WHERE id = 1
```

**适用场景**：中大型租户、需要强隔离、合规要求
**优点**：
- 完整的命名空间隔离
- 共享 Schema 可放全局配置表（model_config、vendor 等）
- PostgreSQL `search_path` 原生支持，应用层 SQL 无需改表名
- 可独立做 pg_dump 备份
**缺点**：Schema 数量有实际上限（建议 < 1000）、DDL 需遍历所有 Schema

---

#### Level 4：租户独立数据库（Database-Level Isolation）

**原理**：VIP 大租户分配独立的物理数据库实例。

```
┌──────────────┐  ┌──────────────┐  ┌──────────────┐
│  db_default   │  │  db_t_vip01  │  │  db_t_vip02  │
│  (共享库)     │  │  (VIP 租户)   │  │  (VIP 租户)   │
│              │  │              │  │              │
│ 小租户共用    │  │ 独享全部表    │  │ 独享全部表    │
│ T-001~T-999  │  │ VIP-001      │  │ VIP-002      │
└──────────────┘  └──────────────┘  └──────────────┘

路由:
  TenantContext(VIP-001) → db_t_vip01 数据源
  TenantContext(T-001)   → db_default 数据源 + Schema/Table 隔离

连接管理:
  DataSourcePool 中动态管理多个 DatabaseConnection
  租户 → 数据源映射存储在 sys.tenant_datasource_mapping 表中
```

**适用场景**：VIP/企业级租户、数据主权要求、独立 SLA
**优点**：完全物理隔离、独立扩缩容、可部署在不同地域
**缺点**：资源成本最高、运维复杂度最高

---

#### 混合模式（推荐）

实际 SaaS 产品中，**不同规模的租户用不同隔离级别**：

```
┌─────────────────────────────────────────────┐
│              TenantRouter                    │
│                                              │
│  运行时从 sys.tenant_datasource 加载:        │
│                                              │
│  免费用户 ──→ Level 1 (共享表 + tenant_id)   │
│  付费用户 ──→ Level 2 (独立表) 或 Level 3    │
│  企业用户 ──→ Level 3 (独立 Schema)          │
│  VIP 企业 ──→ Level 4 (独立数据库)           │
│                                              │
│  租户数据源表:                                │
│  ┌──────────────────────────────────────┐    │
│  │ sys.tenant_datasource                │    │
│  │                                      │    │
│  │ id │ tenant_id │ tier │ isolation    │    │
│  │ 1  │ T-001     │ free │ shared_row   │    │
│  │ 2  │ T-PRO     │ pro  │ sep_table    │    │
│  │ 3  │ T-ENT     │ ent  │ sep_schema   │    │
│  │ 4  │ T-VIP     │ vip  │ sep_database │    │
│  │                                      │    │
│  │ schema_name  │ db_uri │ db_max_conns │    │
│  │ NULL         │ NULL   │ NULL         │    │
│  │ NULL         │ NULL   │ NULL         │    │
│  │ tenant_ent01 │ NULL   │ NULL         │    │
│  │ NULL         │ pg://..│ 20           │    │
│  └──────────────────────────────────────┘    │
│                                              │
│  数据源管理:                                  │
│  - 启动时全量加载 → 为 sep_database 租户      │
│    动态创建 DatabaseConnection               │
│  - PG LISTEN/NOTIFY 监听变更 → 热加载        │
│  - onboard: 建 schema/表 → 写入行 → 通知     │
│  - offboard: 标记 inactive → 延迟清理        │
└─────────────────────────────────────────────┘
```

#### 租户上下文传递

```rust
/// 租户上下文 — 通过 tower middleware / axum Extension 注入
#[derive(Clone)]
pub struct TenantContext {
    pub tenant_id: String,
    pub isolation_level: TenantIsolationLevel,
    /// 当 isolation_level 为 Database 时，指向专属数据源
    pub datasource_override: Option<String>,
    /// 当 isolation_level 为 Schema 时，指向专属 schema
    pub schema_override: Option<String>,
}

pub enum TenantIsolationLevel {
    /// Level 1: 共享表，通过 tenant_id 列过滤
    SharedRow,
    /// Level 2: 租户独立表（log → log_t001）
    SeparateTable,
    /// Level 3: 租户独立 Schema（tenant_001.log）
    SeparateSchema,
    /// Level 4: 租户独立数据库
    SeparateDatabase,
}

/// ShardingConnection 在执行时获取当前租户上下文
/// 当前实现推荐显式绑定，而不是隐式 task-local
let tenant_bound = sharding.with_tenant_context(TenantContext {
    tenant_id: "T-001".to_string(),
    isolation_level: TenantIsolationLevel::SharedRow,
    datasource_override: None,
    schema_override: None,
});
```

#### 租户 + 时间复合分片

大租户的表也可能需要按时间分表，形成**二维分片**：

```
维度 1: tenant_id → 选择 Schema 或数据源
维度 2: create_time → 在该 Schema 内按月分表

示例 (VIP 租户 + 时间分表):
  tenant_vip01.log_202601
  tenant_vip01.log_202602
  tenant_vip01.log_202603

示例 (共享表 + 时间分表):
  ai.log_202603  (WHERE tenant_id = 'T-001')
```

对标 ShardingSphere 的 **复合分片策略 (ComplexShardingStrategy)**。

---

### 5.6 绑定表（Binding Tables）

对标 ShardingSphere 的 BindingTableRule。当多张表使用**相同分片键**时，它们的分片结果必然一致，JOIN 时无需跨分片。

```
绑定组: [ai.request, ai.request_execution, ai.trace_span]
分片键: create_time (共享)

保证:
  request_202603 和 request_execution_202603 在同一个分片
  → JOIN 在单分片内完成，不扇出

SQL:
  SELECT r.*, e.status
  FROM ai.request r
  JOIN ai.request_execution e ON r.id = e.request_id
  WHERE r.create_time >= '2026-03-01'

改写后:
  SELECT r.*, e.status
  FROM ai.request_202603 r
  JOIN ai.request_execution_202603 e ON r.id = e.request_id
  WHERE r.create_time >= '2026-03-01'
  -- 单分片执行，无扇出
```

### 5.7 Hint 强制路由

有时分片键不在 SQL 条件中，需要通过代码显式指定路由目标。对标 ShardingSphere 的 HintShardingStrategy。

```rust
/// Hint 路由 — 业务代码显式指定目标
sharding_conn
    .with_hint(ShardingHint::Table("ai.log_202601"))
    .query_all(stmt)
    .await?;

/// 或者指定分片键值
sharding_conn
    .with_hint(ShardingHint::Value("create_time", ShardingValue::DateTime(ts)))
    .query_all(stmt)
    .await?;

/// 广播到所有分片
sharding_conn
    .with_hint(ShardingHint::Broadcast)
    .execute(ddl_stmt)
    .await?;
```

---

## 六、TOML 配置设计

```toml
# 分片配置示例 (config/sharding.toml)

# ═══════════════════════════════════════
#  数据源定义（仅配置基础设施级别的数据源）
#  租户专属数据源从 sys.tenant_datasource 表动态加载，不在此配置
# ═══════════════════════════════════════
[datasources.ds_default]
uri = "${DATABASE_URL}"
schema = "public"
role = "primary"  # primary / replica

[datasources.ds_ai_replica]
uri = "${DATABASE_AI_REPLICA_URL}"
schema = "ai"
role = "replica"
weight = 10

# ═══════════════════════════════════════
#  多租户配置
#  注意：租户的具体隔离级别和数据源连接信息
#  存储在 sys.tenant_datasource 表中，运行时动态加载
#  此处仅配置全局默认行为和提取策略
# ═══════════════════════════════════════
[tenant]
enabled = true
# 租户 ID 来源：header / jwt_claim / query_param / context
tenant_id_source = "jwt_claim"
tenant_id_field = "tenant_id"
# 默认隔离级别（租户表中未配置时的兜底）
default_isolation = "shared_row"
# 共享表（所有租户可见，不做租户过滤）
shared_tables = ["ai.model_config", "ai.vendor", "ai.channel"]

# 行级隔离（Level 1）的列名配置
[tenant.row_level]
column_name = "tenant_id"
# 使用 PostgreSQL RLS 还是 SQL 改写注入
strategy = "sql_rewrite"      # sql_rewrite / rls

# 租户数据源表结构（框架自动读取，无需手动配置每个租户）
# ┌──────────────────────────────────────────────────────────────┐
# │ sys.tenant_datasource                                        │
# │                                                              │
# │ id │ tenant_id │ tier       │ isolation_level │ status       │
# │    │           │            │                 │              │
# │ 1  │ T-FREE-01 │ free       │ shared_row      │ active       │
# │ 2  │ T-PRO-01  │ pro        │ separate_table  │ active       │
# │ 3  │ T-ENT-01  │ enterprise │ separate_schema │ active       │
# │ 4  │ T-VIP-01  │ vip        │ separate_db     │ active       │
# │                                                              │
# │ schema_name │ db_uri                    │ db_max_conns       │
# │                                                              │
# │ NULL        │ NULL                      │ NULL (用默认库)     │
# │ NULL        │ NULL                      │ NULL (用默认库)     │
# │ tenant_ent01│ NULL                      │ NULL (用默认库)     │
# │ NULL        │ postgres://vip01:xxx@host │ 20                 │
# └──────────────────────────────────────────────────────────────┘
#
# 运行时行为:
#   1. 启动时加载 sys.tenant_datasource 全量数据到内存
#   2. 为 separate_db 租户动态创建 DatabaseConnection 并加入 DataSourcePool
#   3. 通过 LISTEN/NOTIFY 或定时轮询监听租户表变更，热加载
#   4. 新租户 onboard 时自动：创建 schema/表 → 写入 tenant_datasource → 热加载

# ═══════════════════════════════════════
#  分片规则
# ═══════════════════════════════════════
[[sharding.tables]]
logic_table = "ai.log"
actual_tables = "ai.log_${yyyyMM}"   # 按月动态展开
sharding_column = "create_time"
algorithm = "time_range"

  [sharding.tables.algorithm_props]
  granularity = "month"           # month | week | day
  pre_create_months = 3           # 提前创建 3 个月的表
  retention_months = 24           # 保留 24 个月

  [sharding.tables.key_generator]
  type = "snowflake"
  worker_id = 1

[[sharding.tables]]
logic_table = "ai.request"
actual_tables = "ai.request_${yyyyMM}"
sharding_column = "create_time"
algorithm = "time_range"

  [sharding.tables.algorithm_props]
  granularity = "month"

[[sharding.tables]]
logic_table = "ai.trace_span"
actual_tables = "ai.trace_span_${yyyyMM}"
sharding_column = "created_at"
algorithm = "time_range"

  [sharding.tables.algorithm_props]
  granularity = "month"

# 绑定表组（共享分片键的表，JOIN 不跨分片）
[[sharding.binding_groups]]
tables = ["ai.request", "ai.request_execution", "ai.trace_span"]
sharding_column = "create_time"

# ═══════════════════════════════════════
#  读写分离
# ═══════════════════════════════════════
[read_write_splitting]
enabled = true

  [[read_write_splitting.rules]]
  name = "ai_rw"
  primary = "ds_default"
  replicas = ["ds_ai_replica"]
  load_balance = "round_robin"    # round_robin | random | weight

# ═══════════════════════════════════════
#  数据加密（列级透明加解密）
# ═══════════════════════════════════════
[encrypt]
enabled = false

  [[encrypt.rules]]
  table = "ai.token"
  column = "key_hash"
  cipher_column = "key_hash_cipher"     # 密文存储列
  assisted_query_column = "key_hash_eq" # 辅助等值查询列（存摘要）
  algorithm = "aes_256_gcm"
  key_env = "ENCRYPT_KEY"               # 密钥从环境变量读取

# ═══════════════════════════════════════
#  SQL 审计
# ═══════════════════════════════════════
[audit]
enabled = true
slow_query_threshold_ms = 500    # 慢查询阈值
log_full_scatter = true          # 全扇出查询告警
log_no_sharding_key = true       # 无分片键查询告警

# ═══════════════════════════════════════
#  全局配置
# ═══════════════════════════════════════
[sharding.global]
# 广播表（不分片，每个数据源全量同步）
broadcast_tables = ["ai.model_config", "ai.vendor", "ai.channel"]
# 默认数据源
default_datasource = "ds_default"
```

---

## 七、SQL Pipeline 详细流程

```
业务代码:  Entity::find().filter(Column::CreateTime.gte(ts)).all(&sharding_conn)
                |
                v
┌─────────────────────────────────────────────┐
│  1. Statement Intercept                      │
│     从 SeaORM Statement 提取原始 SQL + 参数  │
└──────────────────────┬──────────────────────┘
                       v
┌─────────────────────────────────────────────┐
│  2. SQL Parse (sqlparser-rs)                 │
│     解析为 AST → 提取:                       │
│     - 表名列表 (FROM / JOIN / INSERT INTO)   │
│     - WHERE 条件中的分片键值                  │
│     - ORDER BY / GROUP BY / LIMIT            │
│     - SQL 类型 (SELECT/INSERT/UPDATE/DELETE)  │
└──────────────────────┬──────────────────────┘
                       v
┌─────────────────────────────────────────────┐
│  3. Route                                    │
│     根据表名 → 查找 ShardingRule             │
│     根据分片键值 → ShardingAlgorithm 计算    │
│     输出: RouteResult {                      │
│       targets: [(datasource, actual_table)]  │
│     }                                        │
│     无分片规则 → 透传到默认数据源              │
└──────────────────────┬──────────────────────┘
                       v
┌─────────────────────────────────────────────┐
│  4. Rewrite                                  │
│     遍历 AST，将逻辑表名替换为物理表名        │
│     ai.log → ai.log_202603                   │
│     处理 LIMIT 改写:                         │
│       多分片时 LIMIT 10 OFFSET 20            │
│       → 每个分片 LIMIT 30 OFFSET 0           │
│       (在归并层做真正的排序截断)               │
└──────────────────────┬──────────────────────┘
                       v
┌─────────────────────────────────────────────┐
│  5. Execute                                  │
│     单分片 → 直接执行                        │
│     多分片 → 并发扇出 (tokio::join!)         │
│              同 datasource 可复用连接         │
└──────────────────────┬──────────────────────┘
                       v
┌─────────────────────────────────────────────┐
│  6. Merge                                    │
│     单分片 → 直接返回                        │
│     多分片:                                  │
│       - 有 ORDER BY → 归并排序               │
│       - 有 GROUP BY → 聚合合并               │
│       - 有 LIMIT    → 截断                   │
│       - COUNT/SUM   → 累加                   │
│       - AVG         → SUM/COUNT 重算         │
│       - 无特殊      → 直接拼接               │
└─────────────────────────────────────────────┘
```

---

## 八、与 SeaORM 的集成方式

### 8.1 方案：ConnectionTrait 代理（推荐）

```rust
// 启动时构建 ShardingConnection
let sharding_config = ShardingConfig::from_toml("config/sharding.toml")?;
let sharding_conn = ShardingConnection::build(sharding_config).await?;

// 业务代码完全不变，只是换了连接对象
let logs = ai_log::Entity::find()
    .filter(ai_log::Column::CreateTime.between(start, end))
    .order_by_desc(ai_log::Column::CreateTime)
    .paginate(&sharding_conn, 20)
    .fetch_page(0)
    .await?;

// INSERT 自动路由到正确的分表
let new_log = ai_log::ActiveModel { ... };
new_log.insert(&sharding_conn).await?;
```

### 8.2 Entity 定义无需变化

```rust
// 实体定义保持不变，逻辑表名仍然是 "log"
#[derive(DeriveEntityModel)]
#[sea_orm(schema_name = "ai", table_name = "log")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub create_time: DateTimeWithTimeZone,
    // ...
}
// ShardingConnection 内部会把 ai.log → ai.log_202603
```

---

## 九、分布式 ID 方案

### 9.1 Snowflake（64-bit）

```
┌──────────┬───────────┬──────────┬────────────┐
│ 1 bit    │ 41 bits   │ 10 bits  │ 12 bits    │
│ 符号(0)  │ 毫秒时间戳 │ 机器ID   │ 序列号     │
│          │ (69年)     │ (1024台) │ (4096/ms)  │
└──────────┴───────────┴──────────┴────────────┘
```

- 时间有序，适合 B-Tree 索引
- 每毫秒每节点可生成 4096 个 ID
- 需要配置 worker_id（通过环境变量或 Redis 协调）

### 9.2 TSID（推荐）

```
┌──────────┬───────────┬──────────────────────┐
│ 1 bit    │ 42 bits   │ 21 bits              │
│ 符号(0)  │ 毫秒时间戳 │ 随机/计数(2M/ms)    │
└──────────┴───────────┴──────────────────────┘
```

- 比 Snowflake 更简单，无需配置 worker_id
- 同样时间有序
- 适合单机或少量节点部署

---

## 十、本项目的推荐分片策略

根据当前表结构和业务场景：

### 10.1 需要分表的表（P0）

| 逻辑表 | 分片键 | 算法 | 粒度 | 原因 |
|--------|--------|------|------|------|
| `ai.log` | `create_time` | time_range | 月 | 核心消费日志，写入量最大 |
| `ai.request` | `create_time` | time_range | 月 | 每次 AI 请求产生一条 |
| `ai.request_execution` | `create_time` | time_range | 月 | 每次请求可能多次重试 |
| `ai.trace_span` | `created_at` | time_range | 月 | 追踪链路，量级与 request 相当 |
| `ai.audit_log` | `created_at` | time_range | 月 | 审计日志只增不改 |
| `sys.operation_log` | `create_time` | time_range | 月 | 操作日志持续增长 |
| `sys.login_log` | `create_time` | time_range | 月 | 登录日志 |

### 10.2 广播表（不分片，全量同步）

| 表 | 原因 |
|------|------|
| `ai.model_config` | 配置表，变更少，需全局可见 |
| `ai.vendor` | 供应商字典表 |
| `ai.channel` | 渠道数量有限 |
| `ai.channel_account` | 账号池数量有限 |
| `ai.routing_rule` | 路由规则 |
| `ai.guardrail_config` | 治理配置 |
| `sys.menu` | 菜单配置 |
| `sys.config` | 系统配置 |
| `sys.dict_type` / `sys.dict_data` | 字典 |

### 10.3 不需要分片的表

其余所有 sys/biz 表数据量可预见地保持在合理范围，不需要分片。

---

## 十一、实现路线图

### Phase 1 — 核心框架 + Schema 路由 + 多租户行级隔离（3 周）

- [ ] 定义核心 trait（ShardingAlgorithm, SqlRouter, SqlRewriter, KeyGenerator）
- [ ] 实现 TOML 配置解析（ShardingConfig，含租户配置）
- [ ] 实现 DataSourcePool（管理多个 SeaORM DatabaseConnection）
- [ ] 实现 Schema 路由（sys/biz/ai → 对应数据源）
- [ ] 实现 TenantContext + tenant_id 自动注入（Level 1: SharedRow）
- [ ] 实现 sys.tenant_datasource 表读取 + PG LISTEN/NOTIFY 热加载
- [ ] 实现 ShardingConnection（impl ConnectionTrait）
- [ ] 集成测试：现有业务通过 ShardingConnection + 租户过滤正常工作

### Phase 2 — 时间分表 + SQL 改写 + 绑定表（3 周）

- [ ] 基于 sqlparser 实现 SQL 解析器（提取表名、WHERE 条件、分片键值）
- [ ] 实现 time_range 分片算法
- [ ] 实现 SQL 改写引擎（表名替换 + LIMIT 膨胀）
- [ ] 实现绑定表规则（同分片键表 JOIN 不扇出）
- [ ] 实现 Hint 强制路由 API
- [ ] 实现自动建表定时任务（pre-create）
- [ ] 对 ai.log 表启用时间分表，验证 INSERT / SELECT / JOIN

### Phase 3 — 多分片查询 + 结果归并 + Lookup Index（3 周）

- [ ] 实现扇出执行器（并发查询多个分片）
- [ ] 实现结果归并器（ORDER BY / GROUP BY / LIMIT / 聚合）
- [ ] 实现 Lookup Index（辅助查找表，解决非分片键查询）
- [ ] 实现 SQL 审计（慢查询、全扇出告警）
- [ ] 集成分页查询测试

### Phase 4 — 多租户高级模式 + 读写分离 + ID 生成（3 周）

- [ ] 实现 Level 2: 租户独立表（SeparateTable）
- [ ] 实现 Level 3: 租户独立 Schema（SeparateSchema）
- [ ] 实现 Level 4: 租户独立数据库（SeparateDatabase，从 tenant_datasource 动态连接）
- [ ] 实现混合模式（不同租户不同隔离级别）
- [ ] 实现租户 onboard/offboard 生命周期管理
- [ ] 实现读写分离路由器 + 事务内强制主库
- [ ] 实现 Snowflake / TSID ID 生成器

### Phase 5 — 数据安全（2 周）

- [ ] 数据加密（AES-256-GCM 列级透明加解密 + 辅助查询列）
- [ ] 数据脱敏（手机号 / 邮箱 / IP / 自定义规则）
- [ ] 脱敏权限控制（管理员跳过脱敏）

### Phase 6 — Online DDL + CDC 数据迁移（3 周）

- [ ] Online DDL（Ghost Table 策略 + 多分片并行编排）
- [ ] CDC Source（PostgreSQL 逻辑复制）
- [ ] CDC Sink（目标表批量写入）
- [ ] CDC Pipeline 三阶段编排（Snapshot → Catch-up → Cutover）
- [ ] 租户迁移场景（Level 1 → Level 3 数据搬迁）
- [ ] 分片扩容场景（重新 hash 分布）

### Phase 7 — 生产加固（持续）

- [ ] 影子库（全链路压测 — 压测流量路由到影子表/库）
- [ ] 复合分片（租户 + 时间二维分片）
- [ ] 数据源自动发现（主从拓扑检测）
- [ ] 历史分片数据归档 / 冷热分离到 ClickHouse
- [ ] 健康检查 + 故障转移
- [ ] 监控指标（分片命中率、查询扇出数、慢查询）
- [ ] 弹性伸缩（在线扩缩分片）
- [ ] 柔性事务（SAGA）

---

## 十二、Java / Go 分库分表方案深度对比

### 12.1 Java — Apache ShardingSphere（业界标杆）

ShardingSphere 是 Apache 顶级项目，功能最全面的分库分表方案。

**部署形态**：
- **ShardingSphere-JDBC**：嵌入式，替换 JDBC DataSource → 对标我们的 `ShardingConnection`
- **ShardingSphere-Proxy**：独立代理进程，兼容 MySQL/PG 协议 → 我们暂不做

**完整特性列表 vs 我们的覆盖度**：

| 特性分类 | ShardingSphere 功能 | 我们的覆盖 | 状态 |
|---------|---------------------|-----------|------|
| **数据分片** | 分库分表 | 分表 + Schema 路由 | 已设计 |
| | 分片算法（Inline/Standard/Complex/Hint） | hash_mod / time_range / complex / hint | 已设计 |
| | 绑定表 (Binding Table) | binding_groups | 已设计 |
| | 广播表 (Broadcast Table) | broadcast_tables | 已设计 |
| | 行表达式 (Groovy Inline) | TOML 配置 | 用 TOML 替代 |
| **多租户** | 未内置（需自行扩展） | **四级隔离模式** | **我们更强** |
| **读写分离** | Primary-Replica + 自动发现 | primary-replica | 已设计 |
| | 数据库发现 (Database Discovery) | discovery.rs | 已设计 |
| | 读写一致性（事务内走主） | 事务内强制主库 | 已设计 |
| **分布式事务** | XA / SAGA / Seata | SAGA（P3） | 后续 |
| | 本地事务 | SeaORM 原生事务 | 已有 |
| **数据加密** | 透明列加密 + 辅助查询列 | encrypt/ 模块 | 已设计 |
| **数据脱敏** | 动态脱敏规则 | masking/ 模块 | 已设计 |
| **影子库** | 全链路压测 | shadow/ 模块 | 已设计 |
| **SQL 审计** | 审计日志 + 拦截 | audit/ 模块 | 已设计 |
| **SQL 联邦** | 跨库复杂查询（Calcite） | scatter_gather | 部分覆盖 |
| **弹性伸缩** | 在线扩缩分片 + 数据迁移 | resharding.rs + cdc/ | 已设计 |
| **配置中心** | ZooKeeper / Nacos / etcd | 文件 + API reload | 简化版 |
| **分布式治理** | 集群协调、锁、选主 | — | **不需要（嵌入式）** |
| **Pipeline** | CDC 数据迁移 | cdc/ 模块 | 已设计 |

**ShardingSphere 的核心 SPI 体系**：
```java
// Java 通过 SPI 做插件化
public interface ShardingAlgorithm extends TypedSPI { }
public interface KeyGenerateAlgorithm extends TypedSPI { }
public interface EncryptAlgorithm extends TypedSPI { }
public interface DatabaseDiscoveryType extends TypedSPI { }
public interface SQLAuditor extends TypedSPI { }
```
→ 我们用 Rust trait 对标，编译时安全 + 零成本分发

---

### 12.2 Go — Vitess（Google/YouTube 出品）

Vitess 是 CNCF 毕业项目，YouTube 数据层核心。架构与 ShardingSphere 差异很大。

**架构**：
```
客户端 → vtgate (路由代理) → vttablet (表管理) → MySQL
```

**核心概念**：

| Vitess 概念 | 说明 | 我们的对应 |
|------------|------|-----------|
| **VSchema** | 虚拟 Schema 定义，描述表如何分片 | ShardingRuleConfig |
| **Vindex** | 虚拟索引，决定行属于哪个分片 | ShardingAlgorithm |
| **Primary Vindex** | 主分片键（唯一决定分片） | sharding_column |
| **Secondary Vindex / Lookup** | 辅助索引（非分片键查询的二级索引表） | **缺失** |
| **Sequence Table** | 全局自增 ID 表 | KeyGenerator |
| **VReplication** | 在线数据迁移 / 重分片 | resharding.rs |
| **Online DDL** | 不停机 DDL 变更 | ddl/ 模块 |
| **Tablet** | 数据分片的运行时实例 | DataSource |

**Vitess 的 Lookup Vindex（我们缺失的重要特性）**：

当需要按**非分片键**查询时，Vitess 自动维护一张**查找表**：

```
场景: ai.log 按 create_time 分表，但需要按 trace_id 查询

Lookup 表: ai.log_trace_id_lookup
┌──────────────┬─────────────┐
│ trace_id     │ shard_key   │  (create_time 的值，用于路由)
├──────────────┼─────────────┤
│ tr-abc-123   │ 2026-03-15  │
│ tr-def-456   │ 2026-02-20  │
└──────────────┴─────────────┘

查询流程:
1. SELECT * FROM ai.log WHERE trace_id = 'tr-abc-123'
2. 先查 lookup 表 → 得到 shard_key = 2026-03-15
3. 路由到 ai.log_202603
4. 在该分片内执行原始查询
```

→ 这是我们文档中**缺失的重要特性**，后面补充。

---

### 12.3 Go — Gaea（小米开源）

Gaea 比 Vitess 轻量很多，更接近 ShardingSphere-Proxy。

**特性**：

| 特性 | Gaea | 我们的覆盖 |
|------|------|-----------|
| SQL 路由 | 基于 yacc SQL 解析 | sqlparser-rs |
| 分库分表 | hash / range / date | hash_mod / time_range |
| 读写分离 | 支持 | 支持 |
| SQL 指纹 | SQL 模板匹配 | — |
| SQL 黑白名单 | 拦截危险 SQL | audit/ 部分覆盖 |
| 慢查询日志 | 内置 | audit/ 慢查询告警 |
| 连接池管理 | 后端连接复用 | DataSourcePool |
| 多租户 | 不支持 | **我们更强** |

---

### 12.4 Go — go-sharding / GORM Sharding

GORM 生态的嵌入式分片插件，最接近我们的定位。

```go
// GORM Sharding 的使用方式 — 对标我们的 ShardingConnection
db.Use(sharding.Register(sharding.Config{
    ShardingKey:         "create_time",
    NumberOfShards:      12,
    ShardingAlgorithm:   sharding.MonthSharding,
    PrimaryKeyGenerator: sharding.PKSnowflake,
}))

// 业务代码无感知 — 与我们的设计一致
db.Create(&Log{...})
db.Where("create_time > ?", time.Now()).Find(&logs)
```

特点：轻量、嵌入式、只支持分表不支持分库。
我们的方案覆盖度远超 GORM Sharding。

---

### 12.5 完整差距分析（Gap Analysis）

经过上述对比，我们的 DESIGN.md **原先缺失的关键特性**：

| # | 缺失特性 | 来源 | 优先级 | 说明 |
|---|---------|------|-------|------|
| 1 | **多租户四级隔离** | SaaS 需求 | P0 | ✅ 已设计 |
| 2 | **绑定表** | ShardingSphere | P1 | ✅ 已设计 |
| 3 | **Hint 强制路由** | ShardingSphere | P1 | ✅ 已设计 |
| 4 | **复合分片（租户+时间）** | ShardingSphere Complex | P1 | ✅ 已设计 |
| 5 | **数据加密** | ShardingSphere | P2 | ✅ 已设计 |
| 6 | **SQL 审计** | ShardingSphere + Gaea | P2 | ✅ 已设计 |
| 7 | **Lookup Index（辅助查找表）** | Vitess Vindex | P2 | ✅ 已设计 |
| 8 | **数据源自动发现** | ShardingSphere | P2 | ✅ 已设计 |
| 9 | **数据脱敏** | ShardingSphere | P2 | ✅ 已设计 |
| 10 | **影子库（全链路压测）** | ShardingSphere | P2 | ✅ 已设计 |
| 11 | **Online DDL** | Vitess | P2 | ✅ 已设计 |
| 12 | **CDC 数据迁移** | ShardingSphere Pipeline | P2 | ✅ 已设计 |

---

## 十三、补充特性：Lookup Index（辅助查找索引）

参考 Vitess 的 Lookup Vindex，解决**非分片键查询**的路由问题。

### 问题

```
ai.log 按 create_time 分表
但经常需要按 trace_id / request_id / user_id 查询
没有分片键 → 必须全扇出 → 性能差
```

### 方案

为高频非分片键查询维护一张 Lookup 表：

```
┌──────────────────────────────────────────────────────────┐
│  ai.log_lookup_trace_id (Lookup 表，不分片)               │
│                                                           │
│  trace_id (PK) │ shard_key (create_time 值)              │
│  tr-abc-123    │ 2026-03-15T10:00:00Z                    │
│  tr-def-456    │ 2026-02-20T14:30:00Z                    │
└──────────────────────────────────────────────────────────┘

INSERT 流程:
  1. INSERT INTO ai.log (trace_id, create_time, ...) VALUES (...)
  2. 同时写入 Lookup: INSERT INTO ai.log_lookup_trace_id (trace_id, shard_key) VALUES (...)

SELECT 流程:
  1. SELECT * FROM ai.log WHERE trace_id = 'tr-abc-123'
  2. 先查 Lookup → shard_key = 2026-03-15
  3. 路由到 ai.log_202603
  4. 执行: SELECT * FROM ai.log_202603 WHERE trace_id = 'tr-abc-123'
```

### 配置

```toml
[[sharding.lookup_indexes]]
logic_table = "ai.log"
lookup_column = "trace_id"
lookup_table = "ai.log_lookup_trace_id"
sharding_column = "create_time"     # 存储分片键值，用于路由

[[sharding.lookup_indexes]]
logic_table = "ai.log"
lookup_column = "request_id"
lookup_table = "ai.log_lookup_request_id"
sharding_column = "create_time"
```

### Trait 设计

```rust
/// Lookup Index — 辅助查找索引
pub trait LookupIndex: Send + Sync + 'static {
    /// INSERT 时同步写入 lookup 表
    async fn on_insert(
        &self,
        db: &dyn ConnectionTrait,
        lookup_value: &ShardingValue,  // trace_id 的值
        shard_key_value: &ShardingValue, // create_time 的值
    ) -> Result<(), DbErr>;

    /// SELECT 时先查 lookup 得到 shard_key
    async fn resolve(
        &self,
        db: &dyn ConnectionTrait,
        lookup_value: &ShardingValue,
    ) -> Result<Option<ShardingValue>, DbErr>;
}
```

---

## 十四、数据脱敏（Data Masking）

参考 ShardingSphere 的 DataMaskRule，对敏感字段在**查询返回时动态脱敏**，写入时不变。

### 与数据加密的区别

| | 数据加密 (encrypt/) | 数据脱敏 (masking/) |
|--|---|---|
| **时机** | 写入时加密，读取时解密 | 存储明文，读取时动态遮蔽 |
| **目的** | 防数据库泄露 | 防应用层越权查看 |
| **可逆** | 可逆（有密钥） | 不可逆（信息丢失） |
| **性能** | 写入有开销 | 读取有微量开销 |

### 脱敏规则

```toml
[masking]
enabled = true

  [[masking.rules]]
  table = "sys.user"
  column = "phone"
  algorithm = "phone"          # 138****1234

  [[masking.rules]]
  table = "sys.user"
  column = "email"
  algorithm = "email"          # u***@example.com

  [[masking.rules]]
  table = "ai.token"
  column = "key_prefix"
  algorithm = "partial"        # sk-abc***
  show_first = 6
  show_last = 0

  [[masking.rules]]
  table = "ai.log"
  column = "client_ip"
  algorithm = "ip"             # 192.168.*.*
```

### Trait 设计

```rust
/// 脱敏算法
pub trait MaskingAlgorithm: Send + Sync + 'static {
    /// 对原始值进行脱敏
    fn mask(&self, value: &str) -> String;
    /// 算法名称
    fn algorithm_type(&self) -> &str;
}

/// 内置算法
pub struct PhoneMasking;     // 138****1234
pub struct EmailMasking;     // u***@example.com
pub struct IpMasking;        // 192.168.*.*
pub struct PartialMasking {  // 自定义保留前N后M
    pub show_first: usize,
    pub show_last: usize,
    pub mask_char: char,
}
```

### 执行时机

脱敏在 SQL Pipeline 的 **Merge 阶段之后**、**返回给业务代码之前**：

```
Execute → Merge → Masking → 返回
```

可通过角色/权限跳过脱敏（管理员看完整数据）：

```rust
sharding_conn
    .with_hint(ShardingHint::SkipMasking)
    .query_all(stmt)
    .await?;
```

---

## 十五、Online DDL（不停机 Schema 变更）

参考 Vitess Online DDL 和 GitHub 的 `gh-ost`，在分片环境下执行 DDL 不锁表、不停服。

### 问题

分片表 `ai.log_202601` ~ `ai.log_202612` 共 12 张表，要加一列：

```sql
-- 传统方式：逐表 ALTER，每次锁表
ALTER TABLE ai.log_202601 ADD COLUMN new_col VARCHAR(64);
ALTER TABLE ai.log_202602 ADD COLUMN new_col VARCHAR(64);
...  -- 12 次锁表，每次可能几秒到几分钟
```

### 方案：Ghost Table + CDC 同步

```
┌───────────────────────────────────────────────────────────────┐
│  Online DDL 流程（每个分片表）                                  │
│                                                                │
│  1. 创建影子表:                                                │
│     CREATE TABLE ai._log_202603_ghost (LIKE ai.log_202603)    │
│     ALTER TABLE ai._log_202603_ghost ADD COLUMN new_col ...   │
│                                                                │
│  2. 全量复制: COPY 存量数据到影子表（分批，不锁表）             │
│     INSERT INTO _ghost SELECT * FROM log_202603                │
│     WHERE id BETWEEN ? AND ? -- 分批 10000 行                 │
│                                                                │
│  3. 增量同步: 通过 PG logical replication 捕获                 │
│     INSERT/UPDATE/DELETE 增量，实时应用到影子表                 │
│                                                                │
│  4. 追平后原子切换:                                            │
│     BEGIN;                                                     │
│     LOCK TABLE ai.log_202603 IN ACCESS EXCLUSIVE MODE;         │
│     -- 应用最后一批增量                                         │
│     ALTER TABLE ai.log_202603 RENAME TO _log_202603_old;       │
│     ALTER TABLE ai._log_202603_ghost RENAME TO log_202603;     │
│     COMMIT;  -- 锁表时间 < 1 秒                               │
│                                                                │
│  5. 清理: DROP TABLE _log_202603_old (异步延迟)               │
└───────────────────────────────────────────────────────────────┘
```

### 批量编排

分片表需要对每个物理表执行上述流程，框架负责**并行编排**：

```rust
/// Online DDL 任务
pub struct OnlineDdlTask {
    pub ddl: String,               // ALTER TABLE ai.log ADD COLUMN ...
    pub actual_tables: Vec<String>, // [log_202601, ..., log_202612]
    pub concurrency: usize,         // 默认 3
    pub batch_size: usize,          // 默认 10000
    pub status: DdlTaskStatus,      // Pending → CopyData → CatchUp → CutOver → Done
}

pub trait OnlineDdlEngine: Send + Sync + 'static {
    async fn submit(&self, task: OnlineDdlTask) -> Result<DdlTaskId, ShardingError>;
    async fn progress(&self, id: DdlTaskId) -> Result<DdlProgress, ShardingError>;
    async fn cancel(&self, id: DdlTaskId) -> Result<(), ShardingError>;
}
```

### 配置

```toml
[online_ddl]
enabled = true
concurrency = 3              # 同时变更的分片数
batch_size = 10000           # 全量复制每批行数
cutover_lock_timeout_ms = 5000  # 切换时最大锁等待
cleanup_delay_hours = 24     # 旧表清理延迟
```

---

## 十六、影子库（Shadow Database — 全链路压测）

参考 ShardingSphere Shadow，在生产环境执行压测流量时，将压测数据路由到影子表/影子库，**不污染真实数据**。

### 原理

```
┌─────────────────────────────────────────────────────────┐
│                       请求入口                           │
│                         │                                │
│              ┌──────────▼──────────┐                     │
│              │  Shadow Router       │                    │
│              │  检测压测标记:       │                     │
│              │  - Header: X-Shadow  │                    │
│              │  - Column: is_shadow │                    │
│              └────┬──────────┬─────┘                     │
│                   │          │                            │
│            正常流量│          │压测流量                    │
│                   │          │                            │
│          ┌────────▼───┐ ┌───▼─────────┐                  │
│          │ ai.log     │ │ ai.log_shadow│ (影子表)         │
│          │ (真实数据)  │ │ (压测数据)   │                  │
│          └────────────┘ └─────────────┘                  │
└─────────────────────────────────────────────────────────┘
```

### 配置

```toml
[shadow]
enabled = false                 # 仅压测时开启
shadow_suffix = "_shadow"

  [shadow.table_mode]
  enabled = true
  tables = ["ai.log", "ai.request", "ai.trace_span"]

  [shadow.database_mode]
  enabled = false
  datasource = "ds_shadow"

  [[shadow.conditions]]
  type = "header"
  key = "X-Shadow"
  value = "true"

  [[shadow.conditions]]
  type = "column"
  column = "is_shadow"
  value = "1"
```

---

## 十七、CDC 数据迁移（Change Data Capture）

参考 ShardingSphere Pipeline 和 Debezium，实现**在线数据迁移、重分片、跨库同步**。

### 场景

| 场景 | 说明 |
|------|------|
| **分片扩容** | 从 4 分片扩到 8 分片，存量数据重新分布 |
| **租户升级** | 租户从 Level 1（共享表）迁移到 Level 3（独立 Schema） |
| **冷热分离** | 旧数据从热库迁移到 ClickHouse / 对象存储 |
| **灾备同步** | 主集群 → 灾备集群实时同步 |

### 架构

```
┌──────────────────────────────────────────────────────────┐
│                   CDC Pipeline                            │
│                                                           │
│  ┌─────────┐    ┌────────────┐    ┌──────────────┐       │
│  │ Source   │───→│ Transformer│───→│ Sink          │      │
│  │ PG WAL  │    │ 分片键重算  │    │ 目标表写入    │      │
│  └─────────┘    └────────────┘    └──────────────┘       │
│                                                           │
│  三阶段:                                                  │
│  1. Snapshot: 全量分批读取                                │
│  2. Catch-up: 消费 WAL 增量                               │
│  3. Cutover: 追平 → 切换路由                              │
└──────────────────────────────────────────────────────────┘
```

### PostgreSQL 逻辑复制集成

```sql
-- 创建逻辑复制槽
SELECT pg_create_logical_replication_slot('summer_cdc_slot', 'pgoutput');

-- 创建发布
CREATE PUBLICATION summer_cdc_pub FOR TABLE ai.log_202601, ai.log_202602;
```

### Trait 设计

```rust
pub trait CdcSource: Send + Sync + 'static {
    async fn snapshot(&self, table: &str, offset: i64, limit: i64)
        -> Result<Vec<Row>, ShardingError>;
    async fn subscribe(&self) -> Result<CdcStream, ShardingError>;
}

pub trait CdcSink: Send + Sync + 'static {
    async fn write_batch(&self, rows: &[Row]) -> Result<(), ShardingError>;
    async fn apply_change(&self, change: CdcChange) -> Result<(), ShardingError>;
}

pub trait RowTransformer: Send + Sync + 'static {
    fn transform(&self, row: Row) -> Result<Row, ShardingError>;
}
```

### 配置

```toml
[cdc]
enabled = false

  [[cdc.tasks]]
  name = "expand_log_shards"
  source_tables = ["ai.log_202601", "ai.log_202602"]
  sink_tables = ["ai.log_0", "ai.log_1", "ai.log_2", "ai.log_3"]
  transformer = "rehash"
  batch_size = 5000

  [[cdc.tasks]]
  name = "migrate_tenant_ent01"
  source_tables = ["ai.log"]
  source_filter = "tenant_id = 'T-ENT-01'"
  sink_schema = "tenant_ent01"

  [[cdc.tasks]]
  name = "archive_old_logs"
  source_tables = ["ai.log_202401"]
  sink_type = "clickhouse"
  sink_uri = "${CLICKHOUSE_URL}"
  delete_after_migrate = true
```

---

## 十八、Rust 方案的独特优势

| 维度 | Java ShardingSphere | Go Vitess/Gaea | Rust summer-sharding |
|------|--------------------|----|-----|
| **分发机制** | JDK SPI + 反射 | interface 动态分发 | trait 静态分发（零成本） |
| **内存安全** | GC 管理 | GC 管理 | 所有权系统（无 GC 暂停） |
| **并发模型** | 线程池 + Future | goroutine | tokio async（无栈协程） |
| **SQL 解析** | ANTLR（重量级） | yacc / parser | sqlparser-rs（纯 Rust，轻量） |
| **部署** | JVM + classpath 或独立 Proxy | 独立 Proxy 集群 | 编译到业务二进制，无额外进程 |
| **启动时间** | 秒级（JVM 预热） | 毫秒级 | 毫秒级 |
| **类型安全** | 运行时异常 | 运行时 panic | 编译时检查 |
| **包大小** | 几十 MB（含依赖） | 几 MB | 增量 < 1 MB |

---

## 十九、FAQ

**Q: 为什么不用 PostgreSQL 原生分区（PARTITION BY）？**
A: 原生分区适合单机场景，但不支持跨库。summer-sharding 的目标是兼容未来分库需求。不过两者可以互补 — 在单库阶段可以同时使用 PG 分区，在需要分库时由 sharding 层接管路由。

**Q: 分表后如何做跨表 JOIN？**
A: 跨分片 JOIN 代价高昂。推荐：
1. 广播表 JOIN 分片表 → 广播表在每个分片都有全量副本
2. 关联查询拆为多次单表查询 → 应用层组装
3. 同分片键的表可以 JOIN（绑定表 / co-locate 策略）

**Q: 分表后如何做全局 COUNT？**
A: 归并器支持 SUM(COUNT) 聚合。每个分片执行 COUNT 后，归并器自动累加。

**Q: 如何处理没有分片键的查询？**
A: 无分片键 → 扇出到所有分片 → 归并结果。建议：
1. WHERE 条件中尽量带上分片键
2. 对高频非分片键查询配置 Lookup Index
3. 审计模块会告警全扇出查询，帮助发现性能问题

**Q: 租户从 Level 1 升级到 Level 3 需要数据迁移吗？**
A: 需要。流程：
1. 创建租户独立 Schema（tenant_lifecycle::onboard）
2. 从共享表导出该租户数据（带 tenant_id 过滤）
3. 导入到独立 Schema 的对应表
4. 更新 sys.tenant_config 的 isolation_level
5. 删除共享表中的该租户数据
6. ShardingConnection 热加载新配置 → 后续请求自动路由到新 Schema

**Q: 多租户 + 时间分表怎么组合？**
A: 这就是复合分片。以 VIP 租户为例：
- 维度 1：tenant_id=VIP-001 → 数据源 ds_vip_001
- 维度 2：create_time → 在该数据源内按月分表 log_202603
- 路由顺序：先确定数据源，再确定分表

**Q: ShardingSphere 有 Proxy 模式，我们为什么不做？**
A: Proxy 模式引入额外网络跳转和部署复杂度。Rust 的嵌入式方案（编译进业务二进制）性能更优、运维更简单。如果未来有异构语言接入需求，可以考虑基于 tokio 实现兼容 PG wire protocol 的 Proxy。

**Q: 和 Citus（PostgreSQL 原生分片扩展）有什么区别？**
A: Citus 在数据库层做分片，对应用透明但需要 PG 扩展支持。summer-sharding 在应用层做分片，优势是：
1. 不依赖特定数据库扩展
2. 支持跨异构数据源（未来可以混合 PG + ClickHouse）
3. 多租户隔离策略更灵活
