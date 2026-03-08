//! TypedBatchQueue — 支持类型化批量处理的通用任务队列
//!
//! 合并了 `BackgroundTaskQueue`（通用异步任务）和 `LogBatchCollector`（专用批量收集）
//! 的功能，通过类型系统自动路由到对应的批量处理通道。
//!
//! # 架构
//!
//! ```text
//!  ┌─────────────────────────────────────────────────────────┐
//!  │                    TypedBatchQueue                      │
//!  ├──────────────────┬──────────────────────────────────────┤
//!  │  spawn(future)   │  push::<T>(item)                     │
//!  │  通用异步任务      │  类型化批量收集（按 TypeId 路由）        │
//!  │                  │                                      │
//!  │  ┌─────────┐     │  ┌──────────┐   ┌──────────┐         │
//!  │  │  flume  │     │  │ flume<A> │   │ flume<B> │  ...    │
//!  │  │  MPMC   │     │  │ bounded  │   │ bounded  │         │
//!  │  └─┬──┬──┬─┘     │  └─────┬────┘   └─────┬────┘         │
//!  │    │  │  │       │        │              │              │
//!  │    ▼  ▼  ▼       │        ▼              ▼              │
//!  │   W0  W1 W2      │   FlushWorker     FlushWorker        │
//!  │                  │   (count/timeout)  (count/timeout)   │
//!  └──────────────────┴──────────────────────────────────────┘
//! ```
//!
//! # 使用方式（示例）
//!
//! ```rust,ignore
//! use crate::plugin::typed_batch::TypedBatchQueueBuilder;
//! use sea_orm::EntityTrait;
//!
//! // 在 Plugin::build 中构建
//! let queue = TypedBatchQueueBuilder::new()
//!     .task_capacity(4096)
//!     .task_workers(4)
//!     .register_batch::<sys_operation_log::ActiveModel>(
//!         50,                                    // batch_size
//!         Duration::from_millis(500),             // flush_interval
//!         4096,                                   // channel capacity
//!         {
//!             let db = db.clone();
//!             move |batch| {
//!                 let db = db.clone();
//!                 async move {
//!                     if let Err(e) = sys_operation_log::Entity::insert_many(batch)
//!                         .exec(&db).await {
//!                         tracing::error!("批量写入失败: {}", e);
//!                     }
//!                 }
//!             }
//!         },
//!     )
//!     .build();
//!
//! // 在 service 中使用
//! queue.spawn(async { /* 通用异步任务 */ });
//! queue.push::<sys_operation_log::ActiveModel>(model); // 类型化批量收集
//! ```
//!
//! # 当前状态
//!
//! 此模块为备用实现，当前项目使用 `BackgroundTaskQueue` + `LogBatchCollector` 两层分离方案。
//! 当需要动态注册大量不同类型的批量处理时（5 种以上），可切换为此方案。

use std::any::{Any, TypeId};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use rustc_hash::FxHashMap;

/// 通用异步任务类型
type BoxTask = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

/// 类型化 sender 注册表
///
/// 使用 `FxHashMap`（rustc-hash）替代 `std::collections::HashMap`，
/// 对 `TypeId`（u128）这类整数 key 的哈希速度比 SipHash 快 2-5 倍。
struct BatchRegistry {
    senders: FxHashMap<TypeId, Box<dyn Any + Send + Sync>>,
}

impl BatchRegistry {
    fn new() -> Self {
        Self {
            senders: FxHashMap::default(),
        }
    }

    /// 注册一个类型化的 sender
    fn register<T: Send + 'static>(&mut self, sender: flume::Sender<T>) {
        self.senders.insert(TypeId::of::<T>(), Box::new(sender));
    }

    /// 获取指定类型的 sender
    fn get<T: Send + 'static>(&self) -> Option<&flume::Sender<T>> {
        self.senders
            .get(&TypeId::of::<T>())
            .and_then(|boxed| boxed.downcast_ref::<flume::Sender<T>>())
    }
}

/// 支持类型化批量处理的通用任务队列
///
/// 一个组件同时提供两种能力：
/// - `spawn(future)` — 通用异步任务（同 `BackgroundTaskQueue`）
/// - `push::<T>(item)` — 类型化批量收集，自动攒批 + flush
#[derive(Clone)]
pub struct TypedBatchQueue {
    task_sender: flume::Sender<BoxTask>,
    registry: Arc<BatchRegistry>,
}

impl TypedBatchQueue {
    /// 提交一个通用异步任务（非阻塞）
    pub fn spawn<F>(&self, task: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        if let Err(e) = self.task_sender.try_send(Box::pin(task)) {
            match e {
                flume::TrySendError::Full(_) => {
                    tracing::warn!("TypedBatchQueue 任务队列已满，任务被丢弃");
                }
                flume::TrySendError::Disconnected(_) => {
                    tracing::error!("TypedBatchQueue 任务队列已关闭，任务被丢弃");
                }
            }
        }
    }

    /// 推送一个类型化的项到对应的批量收集器（非阻塞）
    ///
    /// 类型 T 必须先通过 `TypedBatchQueueBuilder::register_batch` 注册，
    /// 否则该项会被丢弃并记录错误日志。
    pub fn push<T: Send + 'static>(&self, item: T) {
        match self.registry.get::<T>() {
            Some(sender) => {
                if let Err(e) = sender.try_send(item) {
                    match e {
                        flume::TrySendError::Full(_) => {
                            tracing::warn!(
                                "TypedBatchQueue batch<{}> 已满，记录被丢弃",
                                std::any::type_name::<T>()
                            );
                        }
                        flume::TrySendError::Disconnected(_) => {
                            tracing::error!(
                                "TypedBatchQueue batch<{}> 已关闭，记录被丢弃",
                                std::any::type_name::<T>()
                            );
                        }
                    }
                }
            }
            None => {
                tracing::error!(
                    "TypedBatchQueue 未注册类型 {}，请在 Builder 中调用 register_batch",
                    std::any::type_name::<T>()
                );
            }
        }
    }
}

/// TypedBatchQueue 构建器
///
/// 通过 Builder 模式配置通用任务队列参数，并注册任意数量的类型化批量处理器。
pub struct TypedBatchQueueBuilder {
    task_capacity: usize,
    task_workers: usize,
    registry: BatchRegistry,
}

impl Default for TypedBatchQueueBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl TypedBatchQueueBuilder {
    pub fn new() -> Self {
        Self {
            task_capacity: 4096,
            task_workers: 4,
            registry: BatchRegistry::new(),
        }
    }

    /// 设置通用任务队列容量（默认 4096）
    pub fn task_capacity(mut self, capacity: usize) -> Self {
        self.task_capacity = capacity;
        self
    }

    /// 设置通用任务队列 worker 数量（默认 4）
    pub fn task_workers(mut self, workers: usize) -> Self {
        self.task_workers = workers;
        self
    }

    /// 注册一个类型化的批量收集器
    ///
    /// # 参数
    /// - `batch_size`: 累积多少条后触发一次批量处理
    /// - `flush_interval`: 超时强制刷新间隔
    /// - `capacity`: 通道容量
    /// - `handler`: 批量处理回调，接收 `Vec<T>` 执行实际操作（如 `insert_many`）
    ///
    /// # 类型约束
    /// - `T: Send + 'static` — 传输的数据类型
    /// - `F: Fn(Vec<T>) -> Fut` — 处理回调，每次 flush 时调用
    pub fn register_batch<T, F, Fut>(
        mut self,
        batch_size: usize,
        flush_interval: Duration,
        capacity: usize,
        handler: F,
    ) -> Self
    where
        T: Send + 'static,
        F: Fn(Vec<T>) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let (tx, rx) = flume::bounded::<T>(capacity);
        self.registry.register(tx);

        let type_name = std::any::type_name::<T>();
        tracing::info!(
            "TypedBatchQueue 注册 batch<{}>: batch_size={}, flush_interval={}ms, capacity={}",
            type_name,
            batch_size,
            flush_interval.as_millis(),
            capacity
        );

        tokio::spawn(batch_flush_loop(
            rx,
            batch_size,
            flush_interval,
            type_name,
            handler,
        ));

        self
    }

    /// 构建并返回 TypedBatchQueue
    pub fn build(self) -> TypedBatchQueue {
        let (tx, rx) = flume::bounded::<BoxTask>(self.task_capacity);

        for i in 0..self.task_workers {
            let rx = rx.clone();
            tokio::spawn(async move {
                while let Ok(task) = rx.recv_async().await {
                    task.await;
                }
                tracing::info!("TypedBatchQueue worker-{} 已退出", i);
            });
        }

        tracing::info!(
            "TypedBatchQueue 已构建: {} workers, 容量 {}",
            self.task_workers,
            self.task_capacity
        );

        TypedBatchQueue {
            task_sender: tx,
            registry: Arc::new(self.registry),
        }
    }
}

/// 批量刷新循环
async fn batch_flush_loop<T, F, Fut>(
    rx: flume::Receiver<T>,
    batch_size: usize,
    flush_interval: Duration,
    type_name: &str,
    handler: F,
) where
    T: Send + 'static,
    F: Fn(Vec<T>) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send,
{
    let mut buffer: Vec<T> = Vec::with_capacity(batch_size);
    let mut interval = tokio::time::interval(flush_interval);
    interval.tick().await; // 跳过第一次立即触发

    loop {
        tokio::select! {
            biased;

            item = rx.recv_async() => {
                match item {
                    Ok(item) => {
                        buffer.push(item);
                        if buffer.len() >= batch_size {
                            let batch: Vec<T> = buffer.drain(..).collect();
                            let count = batch.len();
                            handler(batch).await;
                            tracing::debug!(
                                "TypedBatchQueue batch<{}> flush {} 条",
                                type_name, count
                            );
                        }
                    }
                    Err(_) => {
                        if !buffer.is_empty() {
                            let batch: Vec<T> = buffer.drain(..).collect();
                            let count = batch.len();
                            handler(batch).await;
                            tracing::debug!(
                                "TypedBatchQueue batch<{}> 关闭前 flush {} 条",
                                type_name, count
                            );
                        }
                        tracing::info!("TypedBatchQueue batch<{}> 已退出", type_name);
                        break;
                    }
                }
            }

            _ = interval.tick() => {
                if !buffer.is_empty() {
                    let batch: Vec<T> = buffer.drain(..).collect();
                    let count = batch.len();
                    handler(batch).await;
                    tracing::debug!(
                        "TypedBatchQueue batch<{}> timeout flush {} 条",
                        type_name, count
                    );
                }
            }
        }
    }
}
