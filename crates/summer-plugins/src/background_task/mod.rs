//! 通用后台任务队列插件
//!
//! 基于 `flume` MPMC 有界通道 + 多 worker 实现，
//! 类似 Java 的 `ThreadPoolTaskExecutor` 或 Go 的 buffered channel + N 个 consumer goroutine。
//!
//! 架构：
//! ```text
//!  Producer (Handler)
//!       │
//!       ▼  try_send
//!  ┌──────────┐
//!  │  flume   │  单个共享有界通道（MPMC）
//!  │  bounded │
//!  └─┬──┬──┬──┘
//!    │  │  │   竞争获取（自然负载均衡）
//!    ▼  ▼  ▼
//!   W0  W1 W2 ...  N 个 worker 并发消费
//! ```
//!
//! 特点：
//! - MPMC 通道，多 worker 共享同一个队列，天然负载均衡
//! - 有界队列，防止内存溢出（队列满时丢弃任务并警告）
//! - 多 worker 并发消费，吞吐量随 worker 数线性提升
//! - 非阻塞提交（`try_send`），不影响主请求响应速度

pub mod config;
#[allow(dead_code)]
pub mod typed_batch;

pub use config::BackgroundTaskConfig;

use std::future::Future;
use std::pin::Pin;

use config::{default_capacity, default_workers};
use summer::app::AppBuilder;
use summer::async_trait;
use summer::config::ConfigRegistry;
use summer::plugin::{MutableComponentRegistry, Plugin};

/// 后台任务类型：一个 Send + 'static 的异步闭包
type BoxTask = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

/// 通用后台任务队列
///
/// 通过 spring 依赖注入获取，调用 `spawn` 提交后台任务。
/// 内部基于 flume MPMC 通道，多个 worker 竞争获取任务，天然负载均衡。
#[derive(Clone)]
pub struct BackgroundTaskQueue {
    sender: flume::Sender<BoxTask>,
}

impl BackgroundTaskQueue {
    /// 提交一个后台任务（非阻塞）
    ///
    /// 如果队列已满，任务将被丢弃并记录警告日志。
    pub fn spawn<F>(&self, task: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        if let Err(e) = self.sender.try_send(Box::pin(task)) {
            match e {
                flume::TrySendError::Full(_) => {
                    tracing::warn!("后台任务队列已满，任务被丢弃");
                }
                flume::TrySendError::Disconnected(_) => {
                    tracing::error!("后台任务队列已关闭，任务被丢弃");
                }
            }
        }
    }
}

/// 后台任务队列插件
pub struct BackgroundTaskPlugin;

#[async_trait]
impl Plugin for BackgroundTaskPlugin {
    async fn build(&self, app: &mut AppBuilder) {
        let (capacity, workers) = app
            .get_config::<BackgroundTaskConfig>()
            .map(|c| (c.capacity, c.workers))
            .unwrap_or_else(|_| (default_capacity(), default_workers()));

        let (tx, rx) = flume::bounded::<BoxTask>(capacity);

        // 启动 N 个 worker，共享同一个 Receiver（MPMC 天然负载均衡）
        for i in 0..workers {
            let rx = rx.clone();
            tokio::spawn(async move {
                while let Ok(task) = rx.recv_async().await {
                    task.await;
                }
                tracing::info!("后台任务队列 worker-{} 已退出", i);
            });
        }

        app.add_component(BackgroundTaskQueue { sender: tx });
        tracing::info!("后台任务队列已启动: {} workers, 容量 {}", workers, capacity);
    }

    fn name(&self) -> &str {
        "background-task"
    }
}
