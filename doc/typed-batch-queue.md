# TypedBatchQueue 使用指南

## 概述

`TypedBatchQueue` 是一个合并了**通用异步任务队列**和**类型化批量收集器**的组件，
一个组件同时提供两种能力：

- `spawn(future)` — 提交通用异步任务（如 JWT 查询、通知推送等）
- `push::<T>(item)` — 按类型自动路由到对应的批量通道，攒批后 flush

## 架构

```text
┌──────────────────────────────────────────────────────────┐
│                    TypedBatchQueue                       │
├──────────────────┬──────────────────────────────────────┤
│  spawn(future)   │  push::<T>(item)                     │
│  通用异步任务      │  类型化批量收集（按 TypeId 路由）       │
│                  │                                      │
│  ┌─────────┐    │  ┌──────────┐   ┌──────────┐         │
│  │  flume   │    │  │ flume<A> │   │ flume<B> │  ...    │
│  │  MPMC    │    │  │ bounded  │   │ bounded  │         │
│  └─┬──┬──┬─┘    │  └─────┬────┘   └─────┬────┘         │
│    │  │  │      │        │               │              │
│    ▼  ▼  ▼      │        ▼               ▼              │
│   W0  W1 W2     │   FlushWorker     FlushWorker         │
│                  │   (count/timeout)  (count/timeout)    │
└──────────────────┴──────────────────────────────────────┘
```

## 核心组件

### FxHashMap（rustc-hash）

TypedBatchQueue 使用 `rustc-hash` 提供的 `FxHashMap` 作为类型注册表的底层存储。

#### 为什么不用其他 HashMap？

| 库 | 核心能力 | 开销 | 适用场景 |
|---|---|---|---|
| **`FxHashMap`** | 极致哈希速度 | 最低 | 整数 key、读多写少 |
| `DashMap` | 并发读写安全 | 分片锁开销 | 多线程同时读写 |
| `IndexMap` | 保持插入顺序 | 额外的索引数组 | 需要按序遍历 |
| `std HashMap` | 通用安全 | SipHash 防 DoS | 不信任 key 来源时 |

选择 `FxHashMap` 的原因：

1. **访问模式**：注册表在初始化时写入（单线程），运行时只读（`Arc` 包裹不可变）。
   不需要 `DashMap` 的并发写能力，它的分片锁在只读场景是纯开销。

2. **Key 类型**：`TypeId` 本质是 `u128` 整数。`FxHash` 对整数只需一次乘法运算，
   比 SipHash（std HashMap 默认）和 DashMap（默认也是 SipHash 变体）快 2-5 倍。

3. **零依赖**：`rustc-hash` 无传递依赖，编译快、二进制小。

4. **不需要有序**：我们按 `TypeId` 精确查找 sender，不遍历注册表，
   所以 `IndexMap` 的保序能力无用。

#### 什么时候换其他 HashMap？

- **需要运行时动态注册类型**（热插拔 batch） → 换 `DashMap`
- **需要按注册顺序遍历所有 batch 做统计** → 换 `IndexMap`
- **Key 来源不可信（如用户输入的字符串）** → 用 `std HashMap`（SipHash 防 HashDoS）

## 使用方式

### 1. 在 Plugin 中构建

```rust
use crate::plugin::typed_batch::TypedBatchQueueBuilder;
use std::time::Duration;

pub struct MyPlugin;

#[async_trait]
impl Plugin for MyPlugin {
    async fn build(&self, app: &mut AppBuilder) {
        let db: DbConn = app.get_component()
            .expect("DbConn not found");

        let queue = TypedBatchQueueBuilder::new()
            .task_capacity(4096)    // 通用任务队列容量
            .task_workers(4)        // 通用任务 worker 数
            // 注册操作日志批量收集
            .register_batch::<sys_operation_log::ActiveModel>(
                50,                                     // 累积 50 条触发 flush
                Duration::from_millis(500),              // 或 500ms 超时触发
                4096,                                    // 通道容量
                {
                    let db = db.clone();
                    move |batch| {
                        let db = db.clone();
                        async move {
                            if let Err(e) = sys_operation_log::Entity::insert_many(batch)
                                .exec(&db).await
                            {
                                tracing::error!("操作日志批量写入失败: {}", e);
                            }
                        }
                    }
                },
            )
            // 注册登录日志批量收集
            .register_batch::<sys_login_log::ActiveModel>(
                50,
                Duration::from_millis(500),
                4096,
                {
                    let db = db.clone();
                    move |batch| {
                        let db = db.clone();
                        async move {
                            if let Err(e) = sys_login_log::Entity::insert_many(batch)
                                .exec(&db).await
                            {
                                tracing::error!("登录日志批量写入失败: {}", e);
                            }
                        }
                    }
                },
            )
            .build();

        app.add_component(queue);
    }
}
```

### 2. 在 Service 中使用

```rust
#[derive(Clone, Service)]
pub struct OperationLogService {
    #[inject(component)]
    queue: TypedBatchQueue,       // 只需注入 1 个组件
    #[inject(component)]
    ip_searcher: Ip2RegionSearcher,
}

impl OperationLogService {
    pub fn record_async(&self, dto: CreateOperationLogDto) {
        let ip_location = self.ip_searcher.search_location(&dto.client_ip);
        let queue = self.queue.clone();

        // 通用异步任务：预处理（JWT 查询等）
        queue.spawn(async move {
            let user_name = Self::get_user_name(&dto.user_id.to_string()).await;

            let model = sys_operation_log::ActiveModel {
                user_name: Set(user_name),
                // ... 其他字段
                create_time: Set(chrono::Local::now().naive_local()),
                ..Default::default()
            };

            // 类型化批量收集：自动路由到 sys_operation_log 的 batch channel
            queue.push::<sys_operation_log::ActiveModel>(model);
        });
    }
}
```

### 3. 扩展新类型

只需在 Builder 中增加一行 `register_batch`，无需修改任何其他代码：

```rust
// 新增：审计日志批量收集
.register_batch::<sys_audit_log::ActiveModel>(
    100,                                    // 审计日志量大，攒 100 条
    Duration::from_millis(1000),             // 1 秒超时
    8192,
    {
        let db = db.clone();
        move |batch| {
            let db = db.clone();
            async move {
                sys_audit_log::Entity::insert_many(batch).exec(&db).await.ok();
            }
        }
    },
)
```

## 与当前方案对比

### 当前方案：两层分离（3 个组件）

```text
Service
  ├── BackgroundTaskQueue      ← spawn(future)
  ├── OperationLogCollector    ← push(model)
  └── LoginLogCollector        ← push(model)
```

- 优点：简单直观，各组件职责单一
- 缺点：每新增一种类型，需改 Plugin 代码 + 新增组件 + Service 新增注入

### TypedBatchQueue：一层合并（1 个组件）

```text
Service
  └── TypedBatchQueue
        ├── spawn(future)                          ← 通用任务
        ├── push::<OperationLogActiveModel>(m)      ← 操作日志
        └── push::<LoginLogActiveModel>(m)          ← 登录日志
```

- 优点：Service 只注入 1 个组件，新增类型只需 `register_batch` 一行
- 缺点：类型擦除 + `downcast_ref` 引入微量运行时开销（约 1ns/次）

### 选择建议

| 场景 | 推荐方案 |
|---|---|
| 2-3 种批量类型 | 两层分离（当前方案） |
| 5 种以上或经常新增 | TypedBatchQueue |
| 需要运行时动态注册 | TypedBatchQueue + 将 `FxHashMap` 换为 `DashMap` |

## 内部实现细节

### 类型路由原理

```text
push::<OperationLogActiveModel>(model)
  │
  ▼
TypeId::of::<OperationLogActiveModel>()  →  u128 哈希值
  │
  ▼
FxHashMap.get(&type_id)  →  Box<dyn Any + Send + Sync>
  │
  ▼
downcast_ref::<flume::Sender<OperationLogActiveModel>>()  →  &Sender<T>
  │
  ▼
sender.try_send(model)  →  进入对应的 flume bounded channel
```

`downcast_ref` 内部只做一次 `TypeId` 比较（整数相等判断），零堆分配，开销可忽略。

### 批量刷新策略

每个注册的类型都有独立的 flush worker，使用 `tokio::select! { biased }` 双触发：

```text
tokio::select! {
    biased;  // 优先处理数据接收，避免定时器抢占

    item = rx.recv_async() => {
        buffer.push(item);
        if buffer.len() >= batch_size {
            handler(drain(buffer)).await;     // count 触发
        }
    }

    _ = interval.tick() => {
        handler(drain(buffer)).await;         // timeout 触发
    }
}
```

- **count 触发**：累积到 `batch_size` 条立即 flush（高负载时发挥作用）
- **timeout 触发**：超过 `flush_interval` 不足 `batch_size` 也 flush（低负载时保证时效性）
- **biased**：给予数据接收更高优先级，避免空闲时定时器频繁空转

### 重要注意事项

1. **insert_many 不触发 before_save**：SeaORM 的 `insert_many` 跳过 `ActiveModelBehavior::before_save` 钩子。
   必须在 `push` 之前手动设置 `create_time` 等自动字段。

2. **空批次保护**：flush 前检查 `buffer.is_empty()`，SeaORM 的 `insert_many` 传入空迭代器会 panic。

3. **handler 闭包的 Clone**：handler 通常需要 `clone` 外部资源（如 `DbConn`）。
   使用 `move |batch| { let db = db.clone(); async move { ... } }` 模式。

4. **类型必须预注册**：运行时 `push` 未注册的类型会记录 error 日志并丢弃。
   所有类型必须在 `build()` 之前通过 `register_batch` 注册。
