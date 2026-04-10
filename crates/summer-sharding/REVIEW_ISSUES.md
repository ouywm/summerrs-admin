# summer-sharding 代码深度审查报告

> **审查日期**: 2026年3月30日
> **目标库**: `summer-sharding` (Rust + SeaORM + PostgreSQL 分库分表中间件)

本报告汇总了对 `summer-sharding` 核心代码库的深度审查结果，涵盖了分布式事务、性能瓶颈、数据一致性及架构缺陷等方面。

---

## 1. 分布式事务与一致性风险

### 1.1 SAGA 协调器缺乏持久化 (P0)
- **位置**: `src/connector/transaction.rs` (`SagaCoordinator`)
- **问题**: 当前的 SAGA 协调器完全运行在内存中。如果进程在执行一组 SAGA 步骤期间崩溃，后续的补偿逻辑（Compensate）将无法自动触发。
- **影响**: 可能导致分布式环境下各个分片数据处于中间态，无法达到最终一致性。
- **建议**: 将 SAGA 的执行状态（State Machine）持久化到数据库或 Redis 中，并由后台任务负责崩溃后的恢复和补偿流程。

### 1.2 Lookup Index 更新非原子性 (P1)
- **位置**: `src/connector/connection.rs` (`execute_with_raw`)
- **问题**: `sync_lookup_table` 是在主表 `executor.execute` 成功之后调用的。这两个操作分属两次独立的数据库交互。
- **影响**: 如果主表写成功但 `sync_lookup_table` 因网络或程序崩溃而失败，Lookup 索引表将与物理表不一致。
- **建议**:
    1. 在 `ShardingTransaction` 内部，应确保 Lookup 表的更新与主表更新处于同一个物理事务中（如果它们在同一个分片上）。
    2. 对于跨分片场景，建议利用 `cdc` 模块通过逻辑复制监听 WAL 来异步但可靠地更新 Lookup 表，而非在 Web 请求链路中同步更新。

---

## 2. 性能与资源消耗

### 2.1 深分页内存溢出风险 (P1)
- **位置**: `src/merge/limit.rs` (`apply`), `src/merge/mod.rs`
- **问题**: 在处理多分片 `SELECT` 时，系统将每个分片返回的 `LIMIT + OFFSET` 条数据全部 `collect` 到内存中的 `Vec` 里，再在 `DefaultResultMerger` 中进行排序和截断。
- **影响**: 当 `OFFSET` 较大（如万级以上）且分片较多时，内存占用会剧增，极易触发 OOM，且浪费大量网络带宽。
- **建议**: 实现 **流式归并 (Streaming Merge)**。每个分片返回有序的 Stream，`ResultMerger` 采用堆排序（Heap Merge）实时弹出最小值，从而只需要在内存中维护 `N`（分片数）条数据。

### 2.2 SQL 解析与 AST 克隆开销 (P2)
- **位置**: `src/rewrite/mod.rs` (`DefaultSqlRewriter`)
- **问题**: 在改写过程中，代码频繁调用 `analysis.ast.clone()` 为每个分片生成 SQL。
- **影响**: `sqlparser` 的 AST 对象相当庞大，频繁克隆会显著增加 CPU 压力，尤其在高并发点查场景下。
- **建议**: 引入 **SQL 路由缓存**。对于同一类 SQL（Prepare Statement 模式），缓存其解析好的路由模板，改写时仅替换参数和表名，避免全量 AST 克隆和重新渲染。

---

## 3. 执行引擎与错误处理

### 3.1 错误聚合不透明 (P2)
- **位置**: `src/execute/scatter_gather.rs` (`ScatterGatherExecutor`)
- **问题**: 使用 `try_join_all` 处理并发执行。
- **影响**: 如果 10 个分片中有 3 个报错，调用方只能接收到第一个报错的信息，其他分片的失败原因被完全忽略，不利于精细化的系统监控和故障排查。
- **建议**: 自定义 Join 逻辑，收集所有分片的 `Result`，并将它们聚合成一个复合错误对象返回。

### 3.2 缺乏执行超时控制 (P2)
- **位置**: `src/execute/`
- **问题**: 代码中未见到显式的执行层超时（Timeout）编排，完全依赖底层连接池或驱动的超时。
- **影响**: 在扇出（Fanout）查询场景下，一个慢分片会拖累整个请求的响应时间。

---

## 4. 架构约束与局限性

### 4.1 SQL 语法兼容性限制
- **问题**: 基于 `sqlparser` 意味着许多 PostgreSQL 特有语法（如 `WITH RECURSIVE`、复杂的 `JSONB` 操作、自定义类型）可能无法被正确解析或改写。
- **建议**: 增加 SQL 审计告警，对于无法解析或包含不支持语法的 SQL 自动退化为全扇出查询或报错。

### 4.2 绑定表 (Binding Groups) 的强假设
- **位置**: `src/router/mod.rs` (`apply_binding_group`)
- **问题**: 目前假设绑定表的所有实际物理表后缀完全一致。
- **影响**: 这种强耦合限制了分片策略的灵活性，一旦某个表的扩容步长与其他表不一致，绑定关系将破裂。

---

## 5. 改进建议清单

1. [ ] **引入持久化 SAGA**: 确保分布式事务在进程重启后可恢复。
2. [ ] **实现流式归并排序**: 解决深分页 OOM 问题。
3. [ ] **构建预解析缓存**: 降低 SQL 解析和 AST 操作的 CPU 开销。
4. [ ] **完善 2PC 恢复机制**: 目前 `TwoPhaseShardingTransaction` 缺乏崩溃后的自动回滚/提交扫描。
5. [ ] **增加 SQL 审计与慢查询日志**: 自动记录扇出数量过多的查询。
