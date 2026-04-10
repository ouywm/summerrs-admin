# Summer Sharding 性能基准设计

> 目标：量化 `summer-sharding` 相比原生 `sea-orm` 的额外性能损耗，并把损耗拆解到“透传、租户改写、分表路由”三个层次。

## 背景

当前 `summer-sharding` 的功能和 E2E 测试已经比较完整，但还缺少一个明确的性能口径。现在最重要的问题不是“它能不能用”，而是：

1. 在完全不分表、不改写租户条件时，`summer-sharding` 仅作为一层代理，额外开销有多少。
2. 打开行级租户注入后，SQL parse / rewrite 会带来多少增量损耗。
3. 打开真实分表规则后，SQL 解析、路由、表名改写、执行链路整体会额外花多少时间。

这个基准的目标是回答以上三个问题，并且给出与原生 `sea-orm` 的可重复对比数据。

## 设计目标

- 同一台机器、同一个 PostgreSQL、同一份数据、同一条 SQL，对比 `DatabaseConnection` 与 `ShardingConnection`。
- 不使用 mock，不只测内存逻辑，要覆盖真实 SQLx + 网络 IO + PostgreSQL 执行链路。
- 第一轮优先回答“相对损耗多少”，而不是做复杂压测平台。
- 输出既能看绝对耗时，也能看相对损耗比例。

## 非目标

- 这次不做跨机压测。
- 这次不做 HTTP 层 benchmark，只测数据库访问层。
- 这次不做混合业务场景回放。
- 这次不追求极端高并发吞吐，只先建立稳定、可信、可重复的对照基准。

## 方案选型

### 方案 A：Criterion + 真实 PostgreSQL 对照基准

做法：
- 在 `crates/summer-sharding/benches` 下新增 `criterion` 基准。
- 基准内部自动准备测试 schema / 表 / 数据。
- 分别构造原生 `sea-orm::DatabaseConnection` 和 `summer_sharding::ShardingConnection`。
- 用同一批 SQL 在三个场景下对比。

优点：
- 和仓库现有 bench 风格一致。
- 有 warmup / sample size / report 支持。
- 容易本地复现，也容易持续迭代。

缺点：
- `criterion` 原生不会直接输出“损耗百分比”，需要我们在 bench 命名和结果解释上补足。

### 方案 B：自定义微基准 harness

做法：
- 自己写循环、计时、统计 p50/p95 和 overhead ratio。

优点：
- 输出完全可控。
- 更容易直接打印“+12% / +35%”。

缺点：
- 统计和稳定性不如 `criterion`。
- 容易做成一次性脚本，后续不易维护。

### 方案 C：HTTP 接口级压测

做法：
- 启应用后，用 `wrk` / `oha` 直接压接口。

优点：
- 更接近真实业务入口。

缺点：
- 会混入 Web、中间件、序列化、鉴权等因素。
- 无法单独回答 `summer-sharding` 相比原生 `sea-orm` 的数据库层额外成本。

## 选型结论

采用方案 A，保留后续补一个轻量“结果汇总脚本”的空间。

原因：
- 当前最想回答的是数据库访问层的净增损耗。
- `criterion + real postgres` 能兼顾真实性、可维护性和可重复性。
- 先把对照基准做扎实，再决定是否补充更高层的 HTTP 压测。

## 基准范围

第一轮基准覆盖三类场景、四种操作。

### 场景 1：透传开销

定义：
- 不配置分表规则。
- 不开启租户上下文。
- `summer-sharding` 只作为默认数据源代理。

目的：
- 测量“仅接入 `summer-sharding`”的最低额外成本。

对比：
- `sea-orm raw`
- `summer-sharding passthrough`

### 场景 2：行级租户改写开销

定义：
- 启用租户能力。
- 使用共享表 + `tenant_id` 行级隔离。
- 查询与写入都带租户上下文。

目的：
- 测量 SQL 解析和 `tenant_id` 注入的额外成本。

对比：
- `sea-orm raw + 显式 tenant_id 条件`
- `summer-sharding + with_tenant(...)`

说明：
- 原生对照组里，`tenant_id` 条件直接写在 SQL 里，避免把“租户过滤逻辑”本身也算成框架损耗。

### 场景 3：真实分表路由开销

定义：
- 配置一条真实 hash/mod 分表规则。
- 逻辑表映射到 4 张物理表。
- 查询和写入都通过逻辑表执行。

目的：
- 测量 SQL 解析、路由、表名改写和执行的完整额外成本。

对比：
- `sea-orm raw` 直接访问目标物理表
- `summer-sharding` 访问逻辑表

说明：
- 原生对照组不做逻辑表路由，而是直接命中预先计算好的物理表，这是最公平的“下界比较”。

## 操作矩阵

每个场景都跑以下四个操作：

1. 主键点查
   - `SELECT id, ... WHERE id = ?`
2. 小结果集查询
   - `SELECT ... ORDER BY id LIMIT 20`
3. 单行插入
   - `INSERT INTO ...`
4. 单行更新
   - `UPDATE ... WHERE id = ?`

原因：
- 这四个操作覆盖读写主路径。
- 点查最容易看纯额外开销。
- 小结果集查询能看到 route / rewrite 与结果映射的组合成本。
- 插入和更新可以看写路径损耗。

## 环境设计

### 数据库

使用真实 PostgreSQL。

优先方案：
- 独立 bench 库，例如 `summerrs_admin_bench`

退路：
- 若未单独提供，则使用环境变量指定现有本地 PostgreSQL，并在独立 schema 下建表。

建议环境变量：

- `SUMMER_SHARDING_BENCH_DATABASE_URL`

默认不直接写死生产开发库名，避免误污染已有数据。

### Schema 与表

独立 schema：
- `bench_perf`

表设计：

- `bench_perf.raw_probe`
  - 用于透传场景
- `bench_perf.tenant_probe`
  - 用于行级租户场景
  - 含 `tenant_id`
- `bench_perf.order_00`
- `bench_perf.order_01`
- `bench_perf.order_02`
- `bench_perf.order_03`
  - 用于 hash/mod 分表场景

字段保持简单：

- `id BIGINT PRIMARY KEY`
- `tenant_id VARCHAR(64)`，仅租户表需要
- `payload VARCHAR(255)` 或 `JSONB`
- `updated_at TIMESTAMP`

第一轮建议 `payload` 先用 `VARCHAR(255)`，避免 JSON 反序列化噪音过大。

## 数据准备

每张表预置固定规模数据，避免基准过程中不断扩表导致统计漂移。

建议：
- 点查 / 更新基准：预置 10_000 行
- 小结果集查询：保证有足够连续数据
- 插入基准：使用一个单独 id 范围，避免主键冲突

数据准备策略：
- bench 启动时执行 `CREATE SCHEMA IF NOT EXISTS`
- `TRUNCATE` 或 `DELETE`
- 批量插入固定数据
- 每次 benchmark group 之间复位必要数据

## 配置设计

bench 内构造三套连接对象：

1. 原生 `DatabaseConnection`
2. `ShardingConnection` 透传配置
3. `ShardingConnection` 租户配置
4. `ShardingConnection` 分表配置

配置方式：
- 优先使用 `ShardingConfig::from_test_str(...)`
- 直接在 bench 里构造最小可用配置

这样可以保证：
- 基准自包含
- 不依赖应用层 `App`
- 不被插件装配噪声干扰

## 公平性控制

为了让结果可信，需要控制以下变量：

### 1. 相同 SQL 语义

- 原生和 sharding 都跑等价 SQL。
- 不能让原生查一张表、sharding 查四张表后再说“sharding 慢”。

### 2. 连接预热

- 正式计时前先跑 warmup。
- 确保连接池、SQLx statement cache、PostgreSQL buffer cache 已经被激活。

### 3. 避免建连成本混入

- 连接对象在 benchmark 组外初始化。
- 基准内只测查询/写入，不测 `connect`。

### 4. 避免数据漂移

- 对插入和更新使用独立数据范围。
- 如有必要，在每轮 benchmark 之间重置表状态。

### 5. 避免日志与审计噪音

- 透传和路由 benchmark 默认关闭 audit / slow query log / fanout metrics 等可选开销。
- 如果后续要测“全功能模式”，再单独补一个场景。

## 指标设计

主要看三类信息：

1. 绝对耗时
   - ns / us / ms
2. 吞吐
   - iter/s
3. 相对损耗
   - `overhead = (sharding_time - sea_orm_time) / sea_orm_time`

第一轮输出重点：

- 透传损耗
- 租户改写增量损耗
- 分表路由增量损耗

预期展示方式：

- `passthrough.select_by_id`
- `tenant_rewrite.select_by_id`
- `hash_route.select_by_id`
- 同理覆盖 `limit_20 / insert / update`

## 结果解释口径

最终需要能回答下面这些问题：

1. 只接入 `summer-sharding`，最低开销是多少？
2. 行级租户改写比纯透传多多少？
3. 真实分表路由比纯透传多多少？
4. 哪些操作最敏感？
   - 点查更敏感，还是小范围查询更敏感？
   - 写入路径是否明显比读取路径更重？

## 风险与限制

### 风险 1：本地 Docker / PostgreSQL 抖动

解决：
- benchmark 使用独立库或独立 schema
- 运行前尽量避免其他重负载任务

### 风险 2：Criterion 结果不直接给 overhead ratio

解决：
- 先在命名上对齐场景
- 第一轮先读 raw result
- 如有必要，再补一个结果汇总脚本

### 风险 3：原生 SeaORM 与 ShardingConnection 的调用形式不同

解决：
- 第一轮主要使用 `query_all_raw / execute_unprepared` 对齐底层能力
- 后续如有需要，再补 Entity API 对照

## 第一轮交付物

1. `summer-sharding` 独立 bench
2. 自动准备 benchmark schema / 表 / 数据
3. 三类场景、四种操作的对照基准
4. 可直接执行的 bench 命令
5. 一份结果解释说明

## 预计输出示例

```text
passthrough/select_by_id/sea_orm         35.2 us
passthrough/select_by_id/sharding        41.8 us   (+18.8%)

tenant_rewrite/select_by_id/sea_orm      37.4 us
tenant_rewrite/select_by_id/sharding     49.1 us   (+31.3%)

hash_route/select_by_id/sea_orm          36.0 us
hash_route/select_by_id/sharding         58.7 us   (+63.1%)
```

## 结论

这套设计的核心价值不是“证明 sharding 一定更快或更慢”，而是把损耗拆开：

- 透传代理层损耗
- 租户改写损耗
- 分表路由损耗

只要第一轮基准能稳定回答这三件事，后面无论是继续优化 SQL parse、减少 rewrite 成本，还是评估是否需要 statement cache / fast path，都会有明确基线。
