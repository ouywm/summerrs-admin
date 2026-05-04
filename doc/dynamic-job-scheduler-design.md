# 动态任务调度系统设计

> 状态：单实例可用 — P1（基础动态化）+ P2（阻塞/重试/超时/Misfire）+ P3.1（依赖触发）+ P3.2（Unique 去重）+ P3.4（Rhai 脚本）已上线。
> 部署形态：**单实例**。如需多实例水平扩展，建议直接迁移到 xxl-job Rust 版本（`ratch-job`），不在本调度器内做分布式协调。
> 目标：取代 `#[cron]` 硬编码模式，让单进程内的所有定时任务可以网页可配置、可观测。

---

## 一、背景与目标

### 当前痛点（迁出前）

现有任务通过 `summer_job` patch crate 提供的 `#[cron("...")]` 宏在编译期注册到 inventory，启动时由 `auto_jobs()` 收集后塞进调度器：

```rust
#[cron("0 0 * * * *")]
async fn s3_multipart_cleanup(...) { ... }

#[cron("0 */10 * * * *")]
async fn socket_session_gc(...) { ... }
```

带来以下不便：

| 维度 | 现状 |
|---|---|
| cron 改动 | 改源码重发，不能热更新 |
| 启用 / 停用 | 注释代码 |
| 手动触发 | 不支持 |
| 执行日志 | 仅 tracing 输出，无法查询历史 |
| 任务参数 | 函数硬编码，不能从外部传 |
| 失败处理 | 无重试、无告警、无超时 |

### 目标（本调度器）

**内嵌式单实例任务调度** —— 不引入独立调度服务进程：

- 任务定义存 DB，网页 CRUD，秒级生效
- 任务执行状态机 + 全量执行日志可查
- 失败重试 / 超时杀任务 / 阻塞策略 / Misfire 补跑
- 任务依赖触发（A 跑完按状态触发 B）
- 幂等去重（按参数 hash 防重复触发）
- 脚本任务（Rhai，不重新编译就能加任务）

### 非目标（明确划线）

下面这些场景不在本调度器目标内，避免架构滑坡：

- ❌ 多实例分布式协调（选主 / 心跳 / 路由 / 分片广播） —— 用 xxl-job rust 版（`ratch-job`）
- ❌ 工作流 DAG（多 upstream join 等待）
- ❌ 跨进程任务队列（用 `summer-apalis` / `apalis`）

### 设计哲学

延续本项目"插件组合到单二进制"的核心定位：所有调度能力作为 `summer-job-dynamic` crate 集成进现有 app，进程内运行，不依赖外部 broker（除了已有的 Redis 和 Postgres）。

---

## 二、为什么不用 ratch-job 等独立调度服务

`ratch-job` 是 Rust 实现的兼容 xxl-job 协议的独立调度平台（作者也是 rnacos 作者），技术过硬。但定位**正面冲突**：

| 维度 | ratch-job | 本调度器 |
|---|---|---|
| 部署形态 | 独立服务 + 自带 raft 集群 | 单二进制插件 |
| Admin / 用户 | 自有 admin（默认 admin/admin） | 复用项目已有完整 RBAC + 多租户 + JWT |
| 通信协议 | xxl-job HTTP（心跳/注册/触发） | 进程内函数调用 + mpsc channel |
| 任务参数 | 协议限制为 String `triggerParam` | `serde_json::Value` 任意结构 |
| 任务上下文注入 | HTTP body 自解析 | 复用 summer 的 `Component<T>` `Config<T>` |
| 多租户 | `ns://{namespace}/{app}` 字符串前缀 | 已有真正的 `tenant_id` 链路 |
| 操作审计 | 自有 raft 日志 | 已有 `#[operation_log]` 体系 |
| 多实例水平扩展 | ✅（raft 共识） | ❌ 不支持 |

`ratch-job` 解决的是"统一调度一堆异构 Java 执行器 + 多实例水平扩展"。本调度器解决的是"单 Rust 进程想要动态任务管理"。

**取舍**：单实例需求 → 用本调度器；多实例需求 → 直接上 `ratch-job`，**不在本调度器内做分布式协调**。

---

## 三、百家之长：抄哪家的哪一点

| 来源 | 抄什么 |
|---|---|
| **Quartz** | JobDetail / Trigger 分离、Misfire 策略 |
| **xxl-job** | 阻塞策略（SERIAL / DISCARD / OVERRIDE） |
| **Hangfire** | 任务状态机 `Enqueued → Running → Succeeded / Failed / Timeout / Canceled / Discarded`，所有状态迁移落库可审计 |
| **Sidekiq** | 指数退避公式 `(retry_count^4) + 15 + (rand(30) * (retry_count + 1))` |
| **Faktory** | Dead Letter 区分 + 手动 reenqueue（仅设计参考） |
| **River**（Go） | Unique jobs（按 args hash 去重）、insert-time deduplication |
| **PowerJob / DolphinScheduler** | 任务依赖触发（线性链） |

不抄的东西：
- xxl-job 路由策略（FIRST/ROUND_ROBIN/...）和分片广播 → 单实例无意义
- Solid Queue / Temporal 选主 → 单实例无需选主
- DolphinScheduler 完整 DAG → 当前只做线性依赖

---

## 四、整体架构

```
                   ┌────────────────────────┐
   Admin UI ───────│  CRUD API (axum)       │── 写 sys.job + bump version
                   └──────────┬─────────────┘
                              │ direct call
                              ▼
   ┌─────────────────────────────────────────────────────┐
   │          单进程内 (无 broker, 无选主)                │
   │                                                     │
   │  ┌──────────────────┐                               │
   │  │ DynamicScheduler │ ─── upsert / remove ──────┐   │
   │  │ (cron tick 触发) │                            │   │
   │  └────────┬─────────┘                            │   │
   │           │ trigger_now                          │   │
   │           ▼                                      ▼   │
   │  ┌──────────────────┐    finalize    ┌──────────────┐│
   │  │  Worker          │ ───────────────│ DB           ││
   │  │ - blocking       │                │ - sys.job    ││
   │  │ - retry          │                │ - sys.job_run││
   │  │ - timeout        │   依赖触发     │ - sys.job_   ││
   │  │ - unique         │ ──────────────▶│   dependency ││
   │  │ - rhai script    │ LocalTrigger   └──────────────┘│
   │  └──────────────────┘  (mpsc)                        │
   └─────────────────────────────────────────────────────┘
```

### 核心约束

- **单实例**：cron tick 在本进程内 spawn，无需选主；不存在两个实例同时触发同一 cron 的问题
- **DB 是单一事实源**：所有调度状态、配置变更直接写库；service 层修改后通过 `SchedulerHandle` 直接调本进程 scheduler 同步
- **依赖触发走进程内 mpsc**：`engine/local_trigger.rs` 的 `tokio::sync::mpsc::UnboundedChannel<LocalTrigger>` —— worker 完成上游后 send，plugin 启动的 trigger loop 接收并调 `scheduler.trigger_now`，零序列化、零网络
- **任务函数体仍在代码里**：通过 `#[job_handler("name")]` 宏 + inventory 编译期注册到全局 registry（Rust AOT 硬约束；脚本任务用 Rhai 弥补）

---

## 五、数据模型

### sys.job — 任务定义

```sql
CREATE TABLE sys.job (
    id              BIGSERIAL     PRIMARY KEY,
    tenant_id       BIGINT,                            -- 多租户隔离
    name            VARCHAR(128)  NOT NULL,
    group_name      VARCHAR(64)   NOT NULL DEFAULT 'default',
    description     TEXT          NOT NULL DEFAULT '',
    handler         VARCHAR(128)  NOT NULL,            -- registry key 或 'script::rhai'
    schedule_type   VARCHAR(16)   NOT NULL,            -- CRON / FIXED_RATE / FIXED_DELAY / ONESHOT
    cron_expr       VARCHAR(64),
    interval_ms     BIGINT,
    fire_time       TIMESTAMP,                         -- ONESHOT 触发时间
    params_json     JSONB         NOT NULL DEFAULT '{}',
    script          TEXT,                              -- 脚本任务源码
    script_engine   VARCHAR(16),                       -- rhai
    enabled         BOOLEAN       NOT NULL DEFAULT TRUE,
    blocking        VARCHAR(16)   NOT NULL DEFAULT 'SERIAL',     -- SERIAL / DISCARD / OVERRIDE
    misfire         VARCHAR(16)   NOT NULL DEFAULT 'FIRE_NOW',   -- FIRE_NOW / IGNORE / RESCHEDULE
    timeout_ms      BIGINT        NOT NULL DEFAULT 0,            -- 0 = 不限
    retry_max       INT           NOT NULL DEFAULT 0,
    retry_backoff   VARCHAR(16)   NOT NULL DEFAULT 'EXPONENTIAL',-- EXPONENTIAL / LINEAR / FIXED
    unique_key      VARCHAR(128),                      -- River 风格幂等键
    version         BIGINT        NOT NULL DEFAULT 0,  -- 乐观锁
    created_by      BIGINT,
    create_time     TIMESTAMP     NOT NULL DEFAULT CURRENT_TIMESTAMP,
    update_time     TIMESTAMP     NOT NULL DEFAULT CURRENT_TIMESTAMP
);
```

**对比早期设计**：移除了 `route_strategy / shard_total` —— 单实例不需要。

### sys.job_run — 单次触发执行记录

```sql
CREATE TABLE sys.job_run (
    id            BIGSERIAL     PRIMARY KEY,
    job_id        BIGINT        NOT NULL,
    trace_id      VARCHAR(64)   NOT NULL,
    trigger_type  VARCHAR(16)   NOT NULL,           -- CRON / MANUAL / RETRY / WORKFLOW / API / MISFIRE
    trigger_by    BIGINT,                           -- 上游 run_id（依赖触发）或 user_id（手动触发）
    state         VARCHAR(16)   NOT NULL,           -- ENQUEUED / RUNNING / SUCCEEDED / FAILED / TIMEOUT / CANCELED / DISCARDED
    instance      VARCHAR(64),                      -- 执行实例 hostname:pid（重启后 PID 变，可区分启动批次）
    scheduled_at  TIMESTAMP     NOT NULL,
    started_at    TIMESTAMP,
    finished_at   TIMESTAMP,
    retry_count   INT           NOT NULL DEFAULT 0,
    result_json   JSONB,
    error_message TEXT,
    log_excerpt   TEXT,
    unique_key    VARCHAR(128),                     -- worker 计算的去重 lock value
    create_time   TIMESTAMP     NOT NULL DEFAULT CURRENT_TIMESTAMP
);
```

**对比早期设计**：移除了 `shard_index / shard_total`。`instance` 字段保留（hostname:pid 审计用）。

### sys.job_dependency — 任务依赖（P3.1）

```sql
CREATE TABLE sys.job_dependency (
    id              BIGSERIAL     PRIMARY KEY,
    upstream_id     BIGINT        NOT NULL,
    downstream_id   BIGINT        NOT NULL,
    on_state        VARCHAR(16)   NOT NULL DEFAULT 'SUCCEEDED', -- SUCCEEDED / FAILED / ALWAYS
    enabled         BOOLEAN       NOT NULL DEFAULT TRUE,
    create_time     TIMESTAMP     NOT NULL DEFAULT CURRENT_TIMESTAMP
);
```

`(upstream_id, downstream_id)` 唯一；service 层禁自循环 + BFS 防环（最多 100 跳）。

### 已废弃的表（旧库迁移用）

如果你是从早期多实例设计的 schema 升级过来，需要执行：

```sql
ALTER TABLE sys.job     DROP COLUMN IF EXISTS route_strategy;
ALTER TABLE sys.job     DROP COLUMN IF EXISTS shard_total;
ALTER TABLE sys.job_run DROP COLUMN IF EXISTS shard_index;
ALTER TABLE sys.job_run DROP COLUMN IF EXISTS shard_total;
DROP TABLE  IF EXISTS sys.job_instance;
```

DDL 文件 `sql/sys/job.sql` 末尾已包含这段迁移。

---

## 六、模块划分

```
crates/summer-job-dynamic/
├── Cargo.toml
├── src/
│   ├── lib.rs                       # 入口 + 重导出
│   ├── plugin.rs                    # SummerSchedulerPlugin（启动钩子 + LocalTrigger loop）
│   ├── context.rs                   # JobContext / JobError / JobResult + ctx.component()/config()/params_as()
│   ├── registry.rs                  # JobHandlerEntry + BuiltinJob inventory + HandlerRegistry
│   ├── enums.rs                     # 调度状态/策略 enum（String 后端）
│   ├── dto.rs                       # CreateJobDto / UpdateJobDto / JobVo / JobRunVo / 批量操作 DTO
│   ├── entity/
│   │   ├── sys_job.rs
│   │   ├── sys_job_run.rs
│   │   └── sys_job_dependency.rs
│   ├── service/
│   │   ├── job_service.rs           # CRUD + import_builtin + trigger（直接走 SchedulerHandle）
│   │   ├── dependency_service.rs    # add / remove / list / BFS 防环
│   │   └── stats_service.rs         # 仪表盘聚合（overview + 单任务 stats）
│   ├── router/job_router.rs         # /api/scheduler/* admin 路由
│   ├── script/rhai_handler.rs       # script::rhai handler + dryrun
│   └── engine/
│       ├── scheduler.rs             # DynamicScheduler 包装 JobScheduler，register/remove/trigger_now/fire_misfire
│       ├── worker.rs                # Worker 单次执行（blocking / retry / timeout / unique / 触发下游）
│       ├── handle.rs                # SchedulerHandle（service 层透过它直接调本进程 scheduler）
│       ├── local_trigger.rs         # LocalTrigger 进程内 mpsc channel
│       ├── blocking.rs              # BlockingTracker / SERIAL / DISCARD / OVERRIDE
│       ├── retry.rs                 # next_retry_delay（Sidekiq 公式）
│       ├── misfire.rs               # evaluate（FIRE_NOW / IGNORE / RESCHEDULE）
│       ├── next_fire.rs             # 下次触发时间计算（前端列表用，含批量 last_run）
│       ├── unique.rs                # should_apply / compute_lock_value / has_conflict
│       └── metrics.rs               # SchedulerMetrics（进程内累计计数）
└── tests/macro_smoke.rs             # 验证 #[job_handler] 宏注册 inventory
```

**对比早期设计**：删除 `engine/election.rs / events.rs / heartbeat.rs / strategy/route.rs`，新增 `engine/local_trigger.rs / next_fire.rs / unique.rs / misfire.rs`。

### handler 签名

```rust
pub struct JobContext {
    pub run_id: i64,
    pub job_id: i64,
    pub trace_id: String,
    pub params: serde_json::Value,
    pub retry_count: i32,
    pub cancel: tokio_util::sync::CancellationToken,
    pub app: Arc<App>,
    pub script: Option<String>,
}

pub type JobResult = Result<serde_json::Value, JobError>;

#[job_handler("s3_multipart_cleanup")]
async fn s3_cleanup(ctx: JobContext) -> JobResult {
    let s3 = ctx.component::<aws_sdk_s3::Client>();
    Ok(serde_json::json!({"aborted": 0}))
}
```

宏行为：
1. 编译期把 `("s3_multipart_cleanup", fn_pointer)` 注册到 `inventory`
2. 启动时 `HandlerRegistry::collect()` 收集所有 handler
3. 调度器按 `sys.job.handler` 字段查 registry 拿到函数指针，拼参数调用

### Plugin 注册顺序

```rust
App::new()
    .add_plugin(SeaOrmPlugin)
    .add_plugin(RedisPlugin)        // 仅业务用，调度器自己不依赖 Redis
    .add_plugin(JobPlugin)          // 现有 summer_job (兼容旧 #[cron] 编译期任务)
    .add_plugin(SummerSchedulerPlugin)
    ...
```

启动逻辑（参见 `plugin.rs::start`）：
1. `HandlerRegistry::collect()` → `add_component(Arc<HandlerRegistry>)`
2. `SchedulerHandle::default()` → `add_component`（占位，schedule 钩子里 install）
3. 在 `add_scheduler` 钩子里：
   - 拿 JobScheduler / DbConn / SchedulerMetrics / 各 Service 组件
   - 创建 `local_trigger::channel()` 拿到 `(trigger_tx, trigger_rx)`
   - 构建 `Worker { trigger_tx, ... }` 和 `DynamicScheduler`
   - `handle.install(scheduler)` 让 service 层能拿到
   - `spawn_local_trigger_loop(scheduler, db, trigger_rx)` 后台消费下游触发
   - 收集 inventory 里的 `BuiltinJob` 调 `import_builtin_if_absent` 落库
   - 加载所有 `enabled=true` 的 jobs 注册到 scheduler
   - 跑一遍 misfire sweep（FIRE_NOW 策略 + 错过 ≥1 次 → 补跑）

---

## 七、关键策略详解

### 阻塞策略（xxl-job 风格，单进程实现）

任务上次还没跑完，又到下一次触发点：

- `SERIAL`：串行执行，新触发排队等待（`BlockingTracker` 用 mutex + queue）
- `DISCARD`：丢弃新触发（写一条 DISCARDED 记录留痕）
- `OVERRIDE`：取消正在跑的旧任务（`CancellationToken.cancel()`），立即跑新触发

### Misfire 策略（Quartz）

调度器停机/进程重启错过了 cron 触发点：

- `FIRE_NOW`：立即补跑一次（多次错过也只补一次，防风暴）
- `IGNORE`：忽略错过的，等下一次
- `RESCHEDULE`：补跑全部错过的（谨慎，可能压垮系统）

### 重试退避（Sidekiq）

```rust
fn next_retry_delay(retry_count: u32, strategy: RetryBackoff) -> Duration {
    match strategy {
        Exponential => {
            let base = (retry_count as u64).pow(4);
            Duration::from_secs(base + 15 + rand::random::<u64>() % (30 * (retry_count as u64 + 1)))
        }
        Linear => Duration::from_secs((retry_count + 1) as u64 * 30),
        Fixed  => Duration::from_secs(60),
    }
}
```

### 超时杀任务

```rust
let cancel = CancellationToken::new();
let ctx = JobContext { cancel: cancel.clone(), ... };
match tokio::time::timeout(timeout, handler(ctx)).await {
    Err(_) => { cancel.cancel(); mark_timeout(...); }
    Ok(res) => { ... }
}
```

handler 内部需要 cooperative cancel：长循环里 `ctx.check_cancel()?`。

### 任务依赖触发（P3.1）

worker 完成 run（终态 SUCCEEDED / FAILED / TIMEOUT / CANCELED）后：

```rust
worker.run_once 终态 ─▶ try_fire_downstream(terminal)
                       │
                       ├─ DependencyService.list_to_fire(upstream_id, terminal)
                       │     筛 enabled=true 且 on_state ∈ {terminal, ALWAYS}
                       │
                       └─ for downstream in candidates:
                            trigger_tx.send(LocalTrigger {
                                job_id: downstream,
                                trigger_by: Some(upstream_run_id),
                                trigger_type: TriggerType::Workflow,
                                ...
                            })
```

进程内 mpsc，零延迟、不丢消息（除非进程崩）。`Discarded` 不触发下游（被阻塞策略丢弃，没真正执行过）。

### Unique jobs（P3.2）

`sys.job.unique_key` 当**开关 + 维度名**：

| 值 | 行为 |
|---|---|
| NULL | 不去重（默认） |
| `"params"` | 按 `params_json` sha256 hash 去重 |
| 任意其他字符串 | 字面用作 lock string（如填 `"global"` = 全局只能一个 run） |

仅 `Cron / Manual / Misfire / Api` 触发参与去重；`Retry / Workflow` 跳过（避免 retry 撞死自己 + 依赖触发被锁）。冲突时写一条 `state=DISCARDED` 记录留痕。

DB 兜底：
```sql
CREATE UNIQUE INDEX idx_sys_job_run_unique_active
    ON sys.job_run (job_id, unique_key)
    WHERE unique_key IS NOT NULL AND state IN ('ENQUEUED', 'RUNNING');
```

### Rhai 脚本任务（P3.4）

handler 字段填 `script::rhai`，源码存 `sys.job.script`。脚本能用：
- `params`：参数对象（自动从 JSON 转 rhai Map / Array / 标量）
- `log_info(msg)` / `log_warn(msg)`：写到 tracing
- 标准 rhai 算术 / 字符串 / Map / Array / 控制流

沙盒：默认无文件 / 无网络；`max_operations = 1_000_000`、`max_string_size = 1MB`、`max_array_size = 10_000`。超时强杀复用 worker 的 `tokio::time::timeout`（rhai 1.x 没 cooperative cancel）。

`POST /api/scheduler/script/dryrun` 提供同步试运行（不写 DB），前端编辑器用。

---

## 八、阶段分解（实施粒度）

### P1：基础动态化 ✅ 全部上线

- [x] 表 `sys.job` + `sys.job_run` + `sys.job_dependency`（DDL 在 `sql/sys/job.sql`）
- [x] `#[job_handler("name")]` 宏 + inventory registry
- [x] `JobContext` + handler 调用约定（`ctx.component::<T>()` / `ctx.config::<T>()` / `ctx.params_as::<T>()`）
- [x] CRUD API + 启停 + 手动触发 + handler 列表 + 执行记录 + 仪表盘统计 + 批量操作
- [x] 内嵌 `JobScheduler`（复用 `JobPlugin` 已注册的 component）
- [x] DB ↔ Scheduler 同步（service 直接调 `SchedulerHandle`，无事件总线）
- [x] 运行日志状态机（ENQUEUED → RUNNING → SUCCEEDED / FAILED / TIMEOUT / CANCELED / DISCARDED）
- [x] 把现有两个 `#[cron]` 迁过去（`s3_multipart_cleanup` + `socket_session_gc`），通过 `BuiltinJob` inventory 启动期 import

### P2：单实例生产可用 ✅ 全部上线

- [x] 阻塞策略：`SERIAL`（队列）/ `DISCARD`（丢弃 + 留痕）/ `OVERRIDE`（cancel 旧执行）
- [x] 失败重试 + 指数退避（Sidekiq 公式 EXPONENTIAL / LINEAR / FIXED）
- [x] 超时杀任务（`tokio::time::timeout` + `CancellationToken`，handler `ctx.check_cancel()`）
- [x] handler panic 不挂进程（自动 `catch_unwind`）
- [x] Misfire 策略（FIRE_NOW / IGNORE / RESCHEDULE）
- [x] 进程内 metrics（`SchedulerMetrics` 原子计数，按 trigger_type / state 分桶）
- [x] **不做**：选主 / 心跳 / pub/sub 事件总线 / 路由策略 / 分片广播（多实例需求由 ratch-job 接管）

### P3：高级能力（已落 3 项，其余按需）

- [x] **P3.1** 任务依赖触发（A 跑完按 `on_state` 触发 B）— `sys.job_dependency` 表 + `DependencyService`（含 BFS 防环）+ worker 钩子在 `try_fire_downstream` 走 LocalTrigger mpsc
- [x] **P3.2** Unique jobs（按 unique_key 维度防重）— `sys.job.unique_key` 当开关，worker 用 sha256 计算 lock value 写入 `sys.job_run.unique_key`
- [x] **P3.4** Rhai 脚本任务（handler=`script::rhai`，沙盒 + dryrun）
- [ ] **P3.3** 任务版本与回滚（`version` 字段已预留，需建 history 表 + rollback API）— 暂搁
- [ ] **P3.5** 工作流 DAG（DolphinScheduler 风格 join 等待）— 暂搁
- [ ] 分片广播 — **不做**（多实例特性，迁 ratch-job）
- [ ] Web UI — 前端仓库独立开发中
- [ ] 接 Prometheus exporter — 已有 metrics，加 text/plain 端点 30 分钟可补

---

## 九、关键决策

| 决策点 | 选择 | 理由 |
|---|---|---|
| 新建 crate vs 塞进 summer-system | **新建 `summer-job-dynamic`** | 解耦，未来可独立开源 |
| 保留 `#[cron]` 宏 | **保留** | 作为"内置任务"快捷方式，启动时 import 到 DB，可在网页改 cron 但不能删 |
| 脚本引擎 | **Rhai** | Rust 原生，syntax 类 Rust，安全沙箱；不选 mlua（Lua 5.x 风味不一致） |
| 调度内核 | **`tokio-cron-scheduler` 复用 `JobPlugin` 已注册的 scheduler component** | 不自起新实例，跟 `#[cron]` 静态任务统一管理 |
| 多实例支持 | **不支持，需多实例直接迁 ratch-job** | 避免重新发明分布式协调；本调度器专注单实例可用性 |
| 跨组件通信 | **进程内 mpsc + 直接函数调用** | 不引入 broker，零序列化、零网络 |
| 依赖触发 | **走 mpsc 而非直接调 scheduler** | 让"触发入口"统一在 scheduler，blocking / metrics / 重试链路一致 |
| 主键序列 | **BIGSERIAL** | Postgres 原生，不用雪花 ID（单实例不需要分布式 ID） |

---

## 十、与现有系统的集成点

- **多租户**：`sys.job.tenant_id`，调度时按 tenant 过滤（`SummerShardingPlugin` 自动改写 SQL）
- **RBAC**：CRUD API 用 `#[has_perm("system:job:list")]` 等装饰（开发期可能临时摘除，上线前必须补回）
- **审计**：所有 admin 操作走 `#[operation_log]`
- **i18n**：错误信息走 `rust-i18n`
- **Component 注入**：handler 复用 summer 的 extractor 体系（`ctx.component::<T>()` / `ctx.config::<T>()`）

---

## 十一、风险与开放问题

1. **任务函数体编译期约束**：Rust 不能动态写函数。脚本任务 P3.4 通过 Rhai 弥补，但宿主 API（能调哪些组件）需要规划。当前默认沙盒禁用所有外部调用。

2. **任务参数 schema 校验**：`params_json` 是 JSONB，handler 内部 `serde_json::from_value` 容易出错。后续可给 handler 关联类型 + 注册期生成 JSON schema 校验。

3. **任务取消的 cooperative 性**：handler 必须主动检查 `cancel`，否则 timeout 杀不掉。已写规范，未做静态强制。

4. **进程崩溃残留**：worker 进程 SIGKILL 时正在 RUNNING 的 run 会留在 DB 永远不结束。**reaper** 待补：启动时把 `started_at < now - threshold` 且 state=RUNNING 的标 FAILED("instance crashed")。

5. **优雅停机**：当前没接 SIGTERM 等待 in-flight handler 跑完。K8s rolling update 会硬杀。后续可补一个"shutdown 信号 → cancel 所有 cron tick + drain in_flight"。

6. **`sys.job_run` 表膨胀**：每个 cron 任务一天最多 1440 行。半年累积可观。需补一个内置 `summer_system::job_run_cleanup` 任务，每天清 30 天前的 SUCCEEDED / DISCARDED 记录，FAILED / TIMEOUT / CANCELED 全保留。

---

## 十二、当前实现速查

### 启动流程

```
1. SummerSchedulerPlugin::build (build 阶段)
   - HandlerRegistry::collect() → add_component(Arc<HandlerRegistry>)
   - SchedulerHandle::default() → add_component（占位）
   - SchedulerMetrics::new() → add_component
   - app.add_scheduler(start)（注册延迟启动钩子）

2. SummerSchedulerPlugin::start (schedule 钩子)
   - 拿 JobScheduler / DbConn / SchedulerHandle / JobService / DependencyService component
   - 创建 local_trigger::channel() → (trigger_tx, trigger_rx)
   - 构建 Worker / DynamicScheduler
   - handle.install(scheduler)（service 层从此可直接调 scheduler）
   - spawn_local_trigger_loop（后台消费 LocalTrigger，调 scheduler.trigger_now）
   - 收集 inventory::iter::<BuiltinJob> → JobService.import_builtin_if_absent()
   - load enabled jobs → scheduler.load_and_register_all
   - run_misfire_sweep（FIRE_NOW + 错过 ≥1 次 → 补跑）
```

### admin API 速查

```
GET    /api/scheduler/handlers                        列出可用 handler 名（前端下拉）
GET    /api/scheduler/jobs                            分页查询任务
GET    /api/scheduler/jobs/{id}                       任务详情
POST   /api/scheduler/jobs                            创建任务
PUT    /api/scheduler/jobs/{id}                       更新任务
DELETE /api/scheduler/jobs/{id}                       删除任务
POST   /api/scheduler/jobs/{id}/toggle                启停 (body: {"enabled": true})
POST   /api/scheduler/jobs/{id}/trigger               手动触发（同步 spawn 调 worker.execute）
GET    /api/scheduler/runs                            分页查执行记录
GET    /api/scheduler/runs/{id}                       执行记录详情
GET    /api/scheduler/jobs/{id}/dependencies          列双向依赖
POST   /api/scheduler/jobs/{id}/dependencies          加依赖
DELETE /api/scheduler/jobs/{id}/dependencies/{depId}  删依赖
GET    /api/scheduler/metrics                         进程内累计指标
GET    /api/scheduler/stats/overview?period=24h       仪表盘聚合（DB GROUP BY）
GET    /api/scheduler/jobs/{id}/stats?period=7d       单任务统计（avg / P50 / P99 / 趋势点）
POST   /api/scheduler/jobs/batch/toggle               批量启停
POST   /api/scheduler/jobs/batch/trigger              批量触发
DELETE /api/scheduler/jobs/batch                      批量删除
POST   /api/scheduler/script/dryrun                   Rhai 脚本试运行（不写 DB，捕获 logs）
```

权限：`system:job:list` / `system:job:create` / `system:job:update` / `system:job:delete` / `system:job:trigger` 等（前端 v-auth 用 `add` / `edit` / `delete` / `toggle` / `trigger` / `batch` / `dependency` / `dryrun` 8 个 authMark）

### Sidekiq 重试公式

```
EXPONENTIAL: retry_count^4 + 15 + rand(30) * (retry_count + 1)
LINEAR:      (retry_count + 1) * 30 + rand(30)
FIXED:       60 (秒)
```

---

## 十三、未来演进方向（待评估）

### 13.1 多实例水平扩展

**结论：不在本调度器内实现，直接迁 ratch-job。**

如果某天单实例顶不住（QPS / 单点故障容忍 / 业务跨可用区），切换路径：
1. 数据迁移：本调度器的 `sys.job` 表导出为 ratch-job 格式（schedule/handler/params）
2. handler 改造：现有 Rust handler 函数变成 ratch-job 的"执行器"（HTTP 入口）
3. 拆服务：本进程不再跑调度器，只暴露执行端点；ratch-job 集群独立部署

**为什么不在本调度器加分布式**：
- 选主 / 心跳 / 路由 / 分片广播是另一套复杂度量级，做不好就半残（参考早期版本就是因此回退）
- ratch-job 已经做了，且兼容 xxl-job 协议，生态成熟
- 本调度器专注"单进程内动态调度"这个完整闭环，不贪多

### 13.2 任务版本与回滚（P3.3，建表即可）

`sys.job.version` 字段已预留。补一张 `sys.job_history` 表（每次 update 时落一份完整 JSONB 快照 + 操作人 + 时间），加 `GET /jobs/{id}/history` + `POST /jobs/{id}/rollback/{version}` 即可。约 2-3 小时工作量。仅在多人协作改任务、需要审计/回滚时有价值。

### 13.3 工作流 DAG（P3.5，复杂）

当前 P3.1 依赖触发是线性链或简单分叉。完整 DAG 需要：
- `sys.job_workflow` / `sys.job_workflow_run` / `sys.job_workflow_node_run` 三张表
- 拓扑排序 + join 等待引擎（"全部 upstream 成功才跑 D"）
- 失败策略 / 取消传播 / 节点超时
- 防环（DAG 检测）
- 前端 DAG 拖拽编辑器

工作量约 2-3 天后端 + 2-3 天前端。仅在有真实 ETL / 多步业务流水线需求时做。

### 13.4 不需要切换的场景

只要满足下面任一项，都不必动当前架构：
- 永远单实例部署
- QPS 要求低（cron tick 间隔 ≥ 1 分钟）
- 没有"多 upstream join 等待"需求

---

## 附录：变更日志

| 日期 | 变更 |
|---|---|
| 2026-05-03 | 单实例化重构：删除选主 / 心跳 / pub/sub 事件总线 / 路由策略 / 分片广播；service 层走 `SchedulerHandle` 直调，依赖触发走 `LocalTrigger` mpsc。多实例需求改为推荐迁 ratch-job |
| 2026-05-02 | P3.4 Rhai 脚本任务上线 |
| 2026-05-02 | P3.2 Unique jobs 上线 |
| 2026-05-02 | P3.1 任务依赖触发上线 |
| 2026-05-02 | P0/P1 前端配套接口（nextFireAt / lastRun / 批量操作 / dryrun / stats）上线 |
| 2026-05-01 | P2 阻塞策略 + 重试 + 超时 + Misfire 上线 |
| 2026-05-01 | P1 基础动态化（DDL + 宏 + Plugin + CRUD + 内置任务迁移）上线 |
