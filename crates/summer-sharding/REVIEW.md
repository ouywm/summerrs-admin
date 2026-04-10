# summer-sharding 代码审查报告

> 审查日期: 2026-03-30

---

## 概览

`summer-sharding` 是一个功能全面的 Rust 分库分表中间件，对标 Java ShardingSphere，涵盖分片算法、SQL 路由/改写/执行/归并、多租户隔离、读写分离、CDC 数据迁移、Online DDL、数据加密脱敏等。整体架构设计清晰、模块划分合理，但在实现细节上存在若干 **Critical** 和 **Major** 级别的问题，主要集中在 **租户隔离安全性**、**2PC 事务正确性**、**SQL 改写完整性** 和 **DDL 切换数据一致性** 方面。

---

## Critical (5 个)

### 1. 租户隔离泄漏 — JOIN 查询未全表注入过滤条件

- **文件**: `tenant/rewrite.rs:45-55`
- **问题**: `inject_query_filter` 仅对 `FROM` 子句中的**第一张表**注入 `tenant_id` 过滤条件。多表 JOIN 时，后续表不会被过滤，导致**跨租户数据泄漏**。
- **影响**: 任何包含 JOIN 的查询都可能让租户 A 看到租户 B 的数据，这是 SaaS 产品中最严重的安全漏洞。
- **建议**: 遍历所有 `FROM` 和 `JOIN` 子句中的表，为每张非共享表注入 `AND {qualifier}.tenant_id = ?` 条件。

### 2. SQL 表名改写未递归进入子查询

- **文件**: `rewrite/table_rewrite.rs:107-125`
- **问题**: `rewrite_set_expr` 仅处理 `FROM` 子句，未递归进入 `WHERE`、`HAVING`、`SELECT` 列表中的子查询。
- **影响**: 包含子查询的 SQL（如 `WHERE id IN (SELECT id FROM logic_table ...)`）中逻辑表名不会被替换，导致查询在物理分片上执行失败。
- **建议**: 实现 AST 的深度遍历 visitor，对 `selection`、`having`、`projection` 中的子查询递归调用表名替换。

### 3. 2PC 事务 — 孤立 Prepared Transaction

- **文件**: `connector/transaction.rs:287-303`
- **问题**: `PREPARE TRANSACTION` 失败后的 `ROLLBACK PREPARED` 回滚错误被静默忽略（`let _ = ...`）。若回滚失败，Prepared Transaction 将**永久滞留在数据库中**，持有锁并阻止 VACUUM，最终导致 Transaction ID Wraparound 或磁盘耗尽。
- **建议**: 记录回滚失败事件到持久化日志，实现后台 recovery worker 定期清理孤立事务（`pg_prepared_xacts`）。

### 4. 占位符参数匹配错误

- **文件**: `connector/statement.rs:687-689`
- **问题**: 遇到 `?` 占位符时始终返回参数列表的**第一个值** `values.0.get(0)`。对于 `WHERE a = ? AND b = ?`，分片键 `b` 会错误地使用 `a` 的值。
- **影响**: 数据被路由到错误的分片，导致**数据错放和查询结果不正确**。
- **建议**: 跟踪占位符位置索引，按顺序匹配对应的参数值。

### 5. Online DDL 切换存在数据丢失窗口

- **文件**: `ddl/mod.rs:198-260`
- **问题**: CDC catch-up 循环在获取表锁**之前**终止。最后一次 `next_batch` 到 `LOCK TABLE` 之间提交的事务虽被 replication slot 捕获，但**不会**被应用到 ghost 表，导致切换后**静默丢失数据**。
- **建议**: 在 `LOCK TABLE` 之后再执行一轮 catch-up drain，确保所有残留变更被应用后再执行 rename swap。

---

## Major (14 个)

### 6. 2PC Commit 部分提交

- **文件**: `connector/transaction.rs:317-333`
- **问题**: 某个 `COMMIT PREPARED` 失败后继续提交其余分支，导致跨分片事务部分提交，违反原子性。
- **建议**: 引入持久化 WAL（Write-Ahead Log），记录事务决议，由 recovery worker 重试失败的 commit。

### 7. AES-GCM 非确定性加密用于等值查询

- **文件**: `rewrite/encrypt_rewrite.rs:353`
- **问题**: 当未配置辅助查询列时，用 AES-GCM（带随机 nonce）加密 WHERE 中的比较值，由于每次密文不同，等值查询**永远无法匹配**。
- **建议**: 强制要求配置 `assisted_query_column`（使用 SHA256 摘要）用于等值查询，或者使用确定性加密方案。

### 8. 绑定表后缀推导逻辑脆弱

- **文件**: `router/mod.rs:312-345`
- **问题**: 通过字符串截取推导绑定表后缀（`actual_table - logic_table = suffix`），假设所有绑定组内的表使用完全相同的后缀模式。对于命名不规则的表会导致**错误路由**。
- **建议**: 使用结构化的后缀匹配（如正则提取分片标识符），而非简单字符串减法。

### 9. Inline 算法路由到不存在的表

- **文件**: `algorithm/inline.rs:78`
- **问题**: 渲染出的目标表名不在 `available_targets` 中时，仍然返回该表名，绕过了可用表校验。
- **建议**: 若目标不在 `available_targets` 中则返回错误或空集。

### 10. Inline 算法渲染顺序错误

- **文件**: `algorithm/inline.rs:39-55`
- **问题**: 先替换 `${value}`，再在**原始**表达式中搜索 `${value % N}` 正则但替换**已渲染**的字符串，可能产生不一致结果。
- **建议**: 统一渲染逻辑：先处理 `${value % N}` 模式，再处理简单的 `${value}` 替换。

### 11. DefaultHasher 跨版本不稳定

- **文件**: `algorithm/hash_range.rs:18`
- **问题**: `std::collections::hash_map::DefaultHasher` 不保证跨 Rust 版本稳定。编译器升级后 hash 值可能变化，导致**现有数据路由失效**。
- **建议**: 使用 `xxhash`、`seahash` 或 `murmur3` 等保证跨平台稳定的 hash 算法。

### 12. SUM/COUNT 聚合精度丢失

- **文件**: `merge/group_by.rs:144-159`
- **问题**: 多分片 `SUM`、`COUNT` 归并时统一转为 `f64`，大整数（超过 2^53）会丢失精度。
- **建议**: 使用 `i128` 或 `BigInt` 做整数聚合，仅在原始类型为浮点时使用 `f64`。

### 13. 多分片写入被硬性阻止

- **文件**: `execute/scatter_gather.rs:22`
- **问题**: `ScatterGatherExecutor::execute` 对写操作委托给 `SimpleExecutor`，后者拒绝 `units.len() > 1`，无法执行跨分片的 UPDATE/DELETE。
- **建议**: 对于广播表或显式 broadcast hint 的写操作，允许多分片并发执行。

### 14. 数据源健康检查串行阻塞

- **文件**: `datasource/pool.rs:167-205`
- **问题**: 健康检查逐个串行 ping 所有数据源，单个慢响应会阻塞整个检查流程。
- **建议**: 使用 `tokio::join!` 或 `FuturesUnordered` 并行执行，设置超时。

### 15. 租户数据源同步加锁争用

- **文件**: `datasource/pool.rs:87-116`
- **问题**: 批量同步租户数据源时循环获取写锁，阻塞所有并发查询的读锁获取。
- **建议**: 先在锁外建立所有连接，然后一次性获取写锁批量更新。

### 16. INSERT...SELECT 缺少租户 ID 注入

- **文件**: `tenant/rewrite.rs:92-112`
- **问题**: 仅处理 `INSERT INTO ... VALUES`，不处理 `INSERT INTO ... SELECT ...`，后者的结果行不会携带 `tenant_id`。
- **建议**: 对 `INSERT ... SELECT` 的 SELECT 子句也注入租户过滤条件。

### 17. AES 密钥弱填充

- **文件**: `encrypt/aes.rs:23-26`
- **问题**: 短于 32 字节的密钥用零填充，大幅降低有效熵。
- **建议**: 使用 HKDF 或 PBKDF2 从短密钥派生 256 位密钥，或拒绝不合规密钥长度。

### 18. 数据脱敏 UTF-8 Panic

- **文件**: `masking/partial.rs:15-16`, `masking/phone.rs:11`
- **问题**: 使用字节索引切片 `&str`，遇到多字节字符（中文、emoji）会 **panic**。
- **建议**: 使用 `.chars()` 迭代器按字符索引操作，或使用 `str::char_indices`。

### 19. ClickHouse Sink 的 Update 实现不当

- **文件**: `cdc/clickhouse_sink.rs:160-172`
- **问题**: 用 `ALTER TABLE DELETE` + `INSERT` 实现 Update。ClickHouse 的 DELETE 是异步 mutation，开销极大且可能与 INSERT 产生竞态。
- **建议**: 使用 ClickHouse 的 `ReplacingMergeTree` 引擎特性，通过 INSERT 覆盖版本而非显式删除。

---

## Minor (14 个)

| # | 文件 | 问题 |
|---|------|------|
| 20 | `algorithm/time_range.rs:124-154` | `normalize_upper_bound` 假设上界为 inclusive，与 `<` 语义不一致 |
| 21 | `algorithm/time_range.rs:237-247` | `add_months` 用 while 循环，大步长时效率低（应用除法） |
| 22 | `algorithm/hash_mod.rs:35-43` | DateTime 类型回退为字符串 hash，与 hash_range 不一致 |
| 23 | `algorithm/complex.rs:77-81` | 未配置 time 算法时硬编码 Month/12 期默认值 |
| 24 | `router/mod.rs:353-364` | `QualifiedTableName::parse` 不支持带引号的标识符 |
| 25 | `router/table_router.rs:76-105` | `expand_numeric_pattern` 仅展开第一个 `${start..end}`，不支持多模式 |
| 26 | `router/rw_router.rs:98-110` | 加权负载均衡创建重复 Vec，高权重时内存浪费 |
| 27 | `merge/group_by.rs:42` | NULL 值序列化为 `"null"` 字符串，与实际字符串 `"null"` 冲突 |
| 28 | `merge/row.rs:10-15` | `ProxyRow` 列查找用线性遍历 + 大小写不敏感比较，效率低 |
| 29 | `masking/ip.rs:9` | 仅支持 IPv4 脱敏，IPv6 地址原样返回 |
| 30 | `masking/email.rs:8-10` | 无 `@` 符号的输入直接返回原文，未做兜底脱敏 |
| 31 | `cdc/pgoutput.rs:77` | 解析了 TRUNCATE 消息但未产生 CdcRecord，目标表不会被清空 |
| 32 | `ddl/ghost.rs:140-151` | Ghost 表名为确定性命名，并发 DDL 同表会冲突 |
| 33 | `shadow/condition.rs:67` | Shadow 列判断仅支持等值条件，不支持 `IN` 等复杂表达式 |

---

## 架构层面建议

1. **SQL AST 遍历需要 Visitor 模式**: 当前的表名替换、租户注入、加密改写都是手工遍历 AST 节点。建议实现统一的 `AstVisitor` trait，所有改写器作为 visitor 注册，避免遗漏子查询等深层节点。

2. **2PC 需要持久化事务日志**: 当前 2PC 没有 WAL，无法在进程崩溃后恢复。建议引入轻量级事务日志（可写文件或数据库表），记录事务决议状态。

3. **测试覆盖重点**: 建议优先为 Critical 问题编写回归测试：
   - 多表 JOIN 的租户隔离
   - 包含子查询的 SQL 改写
   - 多占位符参数的分片键提取
   - DDL 切换期间并发写入的数据一致性

4. **加密方案**: 对需要等值查询的加密列，应**强制**要求配置辅助查询列（摘要列），而非允许 fallback 到非确定性加密。

5. **稳定 Hash 算法**: `hash_range` 应迁移到保证跨版本稳定的 hash 实现，否则 Rust 编译器升级后现有数据路由会失效。
