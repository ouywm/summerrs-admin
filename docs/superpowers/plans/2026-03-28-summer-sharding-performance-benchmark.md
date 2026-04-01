# Summer Sharding Performance Benchmark Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 `summer-sharding` 建立一套基于真实 PostgreSQL 的对照性能基准，量化它相对原生 `sea-orm` 在透传、租户改写、分表路由三类场景中的额外损耗。

**Architecture:** 在 `crates/summer-sharding/benches` 下新增 `criterion` bench，内部自建 benchmark schema / 表 / 数据，并分别构造原生 `DatabaseConnection` 与多套 `ShardingConnection`。基准以真实数据库访问为核心，避免把连接初始化、HTTP 层、插件装配等噪音混入结果。

**Tech Stack:** Rust 2024, Criterion, Tokio, SeaORM, PostgreSQL, summer-sharding

---

### Task 1: 补齐 bench 入口与依赖

**Files:**
- Modify: `crates/summer-sharding/Cargo.toml`
- Create: `crates/summer-sharding/benches/sea_orm_overhead.rs`

- [ ] **Step 1: 给 `summer-sharding` 增加 bench 依赖**

在 `dev-dependencies` 里增加：
- `criterion = { workspace = true, features = ["async_tokio"] }`

增加 bench 声明：

```toml
[[bench]]
name = "sea_orm_overhead"
harness = false
```

- [ ] **Step 2: 先创建最小 bench 文件**

写一个最小 `criterion_group!` / `criterion_main!` 的空基准，确认 crate 可以被 `cargo bench -p summer-sharding --bench sea_orm_overhead --no-run` 识别。

- [ ] **Step 3: 运行验证命令**

Run:

```bash
cargo bench -p summer-sharding --bench sea_orm_overhead --no-run
```

Expected:
- bench target 可编译
- 无 bench 注册错误

- [ ] **Step 4: Commit**

```bash
git add crates/summer-sharding/Cargo.toml crates/summer-sharding/benches/sea_orm_overhead.rs
git commit -m "test: scaffold sharding performance benchmark"
```

### Task 2: 构建 benchmark 环境准备层

**Files:**
- Modify: `crates/summer-sharding/benches/sea_orm_overhead.rs`

- [ ] **Step 1: 写失败前置用例思路**

先在 bench 辅助代码中定义：
- benchmark 数据库 URL 解析
- schema 初始化
- 基准表创建
- 固定数据 seed

目标是让 bench 在首次运行时可自准备环境，而不是要求人工手动建表。

- [ ] **Step 2: 设计 bench 专用对象**

建议在 bench 文件内定义：

```rust
struct BenchmarkContext {
    raw: DatabaseConnection,
    passthrough: ShardingConnection,
    tenant: ShardingConnection,
    sharded: ShardingConnection,
}
```

再定义：

```rust
async fn prepare_benchmark_schema(...)
async fn seed_benchmark_data(...)
async fn build_benchmark_context(...) -> BenchmarkContext
```

- [ ] **Step 3: 准备 benchmark schema 与表**

创建：
- `bench_perf.raw_probe`
- `bench_perf.tenant_probe`
- `bench_perf.order_00`
- `bench_perf.order_01`
- `bench_perf.order_02`
- `bench_perf.order_03`

表字段建议：

```sql
id BIGINT PRIMARY KEY,
tenant_id VARCHAR(64),
payload VARCHAR(255) NOT NULL,
updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
```

- [ ] **Step 4: seed 固定数据**

要求：
- 固定 10_000 行基础数据
- `tenant_probe` 至少包含 2 个租户
- 4 张 order 物理表都要有数据

- [ ] **Step 5: 验证环境准备代码**

Run:

```bash
cargo bench -p summer-sharding --bench sea_orm_overhead --no-run
```

Expected:
- 编译通过
- bench 初始化函数可被调用

- [ ] **Step 6: Commit**

```bash
git add crates/summer-sharding/benches/sea_orm_overhead.rs
git commit -m "test: add sharding benchmark schema setup"
```

### Task 3: 实现透传对照基准

**Files:**
- Modify: `crates/summer-sharding/benches/sea_orm_overhead.rs`

- [ ] **Step 1: 写第一个最小对照基准**

先做一个最小场景：
- 原生 `DatabaseConnection`
- `ShardingConnection` 透传模式
- 操作只测 `select_by_id`

基准命名建议：

```rust
"passthrough/select_by_id/raw"
"passthrough/select_by_id/sharding"
```

- [ ] **Step 2: 验证“红灯”是否合理**

若初始化 / 查询 SQL / 表准备逻辑有误，先让 bench 编译并在首次执行时暴露真实错误，而不是继续堆功能。

- [ ] **Step 3: 扩展透传场景的四个操作**

补齐：
- `select_by_id`
- `select_limit_20`
- `insert_one`
- `update_by_id`

- [ ] **Step 4: 统一 warmup / sample size**

建议：

```rust
group.sample_size(20);
group.warm_up_time(Duration::from_secs(2));
group.measurement_time(Duration::from_secs(8));
```

- [ ] **Step 5: 执行透传基准**

Run:

```bash
cargo bench -p summer-sharding --bench sea_orm_overhead -- passthrough
```

Expected:
- raw 与 sharding 都有结果
- 无连接初始化被重复计入的问题

- [ ] **Step 6: Commit**

```bash
git add crates/summer-sharding/benches/sea_orm_overhead.rs
git commit -m "test: add sharding passthrough overhead benchmarks"
```

### Task 4: 实现租户改写对照基准

**Files:**
- Modify: `crates/summer-sharding/benches/sea_orm_overhead.rs`

- [ ] **Step 1: 写失败前对照定义**

原生对照组：
- SQL 里显式写 `tenant_id`

sharding 组：
- 使用 `with_tenant(TenantContext::new(...))`
- 让 `summer-sharding` 自动做租户注入

- [ ] **Step 2: 实现租户场景配置**

最小配置要求：
- `tenant.enabled = true`
- `row_level.column_name = "tenant_id"`
- `strategy = "sql_rewrite"`

- [ ] **Step 3: 补齐四个操作**

基准命名建议：

```rust
"tenant_rewrite/select_by_id/raw"
"tenant_rewrite/select_by_id/sharding"
```

四个操作仍然是：
- `select_by_id`
- `select_limit_20`
- `insert_one`
- `update_by_id`

- [ ] **Step 4: 验证租户语义正确**

至少保证：
- raw 和 sharding 查到的数据属于同一租户
- 不会因为漏掉租户上下文而导致全表扫描

- [ ] **Step 5: 执行租户基准**

Run:

```bash
cargo bench -p summer-sharding --bench sea_orm_overhead -- tenant_rewrite
```

Expected:
- 可以稳定输出 raw 与 sharding 的对照结果

- [ ] **Step 6: Commit**

```bash
git add crates/summer-sharding/benches/sea_orm_overhead.rs
git commit -m "test: add sharding tenant rewrite overhead benchmarks"
```

### Task 5: 实现分表路由对照基准

**Files:**
- Modify: `crates/summer-sharding/benches/sea_orm_overhead.rs`

- [ ] **Step 1: 定义分表规则**

建议使用 hash/mod 4 分片：
- 逻辑表：`bench_perf.order`
- 物理表：`bench_perf.order_00..03`

原生对照：
- 直接访问目标物理表

sharding 对照：
- 访问逻辑表，由框架路由

- [ ] **Step 2: 封装“原生目标表解析”辅助函数**

定义一个本地 helper，用于 raw 组先按相同规则算出物理表。

这样能保证：
- 两边打到的是同一份真实数据
- 只比较“是否多了一层 sharding 路由开销”

- [ ] **Step 3: 实现四个操作**

同样覆盖：
- `select_by_id`
- `select_limit_20`
- `insert_one`
- `update_by_id`

说明：
- `select_limit_20` 建议先做“单分片可命中”的查询，避免第一轮混入 scatter-gather

- [ ] **Step 4: 执行分表基准**

Run:

```bash
cargo bench -p summer-sharding --bench sea_orm_overhead -- hash_route
```

Expected:
- raw 与 sharding 都能命中同一目标分片
- benchmark 数据稳定

- [ ] **Step 5: Commit**

```bash
git add crates/summer-sharding/benches/sea_orm_overhead.rs
git commit -m "test: add sharding route overhead benchmarks"
```

### Task 6: 补齐结果解释与运行文档

**Files:**
- Modify: `docs/superpowers/specs/2026-03-28-summer-sharding-performance-benchmark-design.md`
- Optionally modify: `crates/summer-sharding/benches/sea_orm_overhead.rs`

- [ ] **Step 1: 写运行说明**

至少包含：

```bash
cargo bench -p summer-sharding --bench sea_orm_overhead
cargo bench -p summer-sharding --bench sea_orm_overhead -- passthrough
cargo bench -p summer-sharding --bench sea_orm_overhead -- tenant_rewrite
cargo bench -p summer-sharding --bench sea_orm_overhead -- hash_route
```

- [ ] **Step 2: 写结果解读模板**

模板需要回答：
- 透传损耗多少
- 租户改写增量多少
- 分表路由增量多少
- 哪类操作最敏感

- [ ] **Step 3: 运行最终验证**

Run:

```bash
cargo bench -p summer-sharding --bench sea_orm_overhead --no-run
cargo test -p summer-sharding
```

Expected:
- bench 可编译
- 不影响现有单元测试

- [ ] **Step 4: Commit**

```bash
git add crates/summer-sharding/Cargo.toml crates/summer-sharding/benches/sea_orm_overhead.rs docs/superpowers/specs/2026-03-28-summer-sharding-performance-benchmark-design.md docs/superpowers/plans/2026-03-28-summer-sharding-performance-benchmark.md
git commit -m "test: add sharding performance benchmark plan"
```
