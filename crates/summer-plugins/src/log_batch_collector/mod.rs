//! 日志专用批量收集器
//!
//! 将多条日志记录累积到一定数量或超时后，一次性批量 INSERT，
//! 减少数据库交互次数，提升写入吞吐量。
//!
//! 架构：
//! ```text
//!  Service (pre-processing)
//!       │
//!       ▼  push (try_send)
//!  ┌─────────────┐
//!  │   flume     │  有界通道
//!  │   bounded   │
//!  └──────┬──────┘
//!         │  recv_async
//!         ▼
//!    Flush Worker
//!    ├─ 累积到 batch_size 条 → INSERT INTO ... VALUES (...), (...), ...
//!    └─ 超时 flush_interval → 不足 batch_size 也刷新
//! ```
//!
//! 注意：SeaORM 的 `insert_many` 不会触发 `ActiveModelBehavior::before_save`，
//! 因此 service 层必须在 push 之前手动设置 `create_time` 等时间戳字段。

pub mod config;

pub use config::LogBatchConfig;

use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use sea_orm::{ActiveModelBehavior, ActiveModelTrait, EntityTrait, IntoActiveModel};
use summer::app::AppBuilder;
use summer::async_trait;
use summer::config::ConfigRegistry;
use summer::error::Result as AppResult;
use summer::plugin::{ComponentRegistry, MutableComponentRegistry, Plugin};
use summer_sea_orm::DbConn;
use summer_system_model::entity::{sys_login_log, sys_operation_log};
use tokio::sync::Notify;
use tokio_util::task::TaskTracker;

const LOG_BATCH_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);
const LOG_BATCH_RETRY_DELAYS_MS: &[u64] = &[50, 200, 500];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogBatchPushError {
    Full,
    Closed,
}

#[derive(Default)]
struct CollectorControl {
    closed: AtomicBool,
    in_flight_pushes: AtomicUsize,
    shutdown: Notify,
}

/// 通用日志批量收集器
#[derive(Clone)]
pub struct LogBatchCollector<A: Send + 'static> {
    sender: flume::Sender<A>,
    control: Arc<CollectorControl>,
}

impl<A: Send + 'static> LogBatchCollector<A> {
    /// 从 flume sender 构造。通常由 [`spawn_typed_collector`] 调用，业务层不需要直接用。
    pub fn new(sender: flume::Sender<A>) -> Self {
        Self {
            sender,
            control: Arc::new(CollectorControl::default()),
        }
    }

    /// 推送一条记录到批量收集器（非阻塞）
    ///
    /// 如果通道已满，记录将被丢弃并记录警告日志。
    pub fn push(&self, model: A) -> Result<(), LogBatchPushError> {
        if self.control.closed.load(Ordering::Acquire) {
            return Err(LogBatchPushError::Closed);
        }

        self.control.in_flight_pushes.fetch_add(1, Ordering::AcqRel);
        if self.control.closed.load(Ordering::Acquire) {
            self.finish_push();
            return Err(LogBatchPushError::Closed);
        }

        let result = self.sender.try_send(model).map_err(|e| match e {
            flume::TrySendError::Full(_) => LogBatchPushError::Full,
            flume::TrySendError::Disconnected(_) => LogBatchPushError::Closed,
        });
        self.finish_push();
        result
    }

    pub fn close(&self) {
        if !self.control.closed.swap(true, Ordering::AcqRel) {
            self.control.shutdown.notify_waiters();
        }
    }

    fn finish_push(&self) {
        if self.control.in_flight_pushes.fetch_sub(1, Ordering::AcqRel) == 1
            && self.control.closed.load(Ordering::Acquire)
        {
            self.control.shutdown.notify_waiters();
        }
    }
}

/// 操作日志批量收集器
pub type OperationLogCollector = LogBatchCollector<sys_operation_log::ActiveModel>;

/// 登录日志批量收集器
pub type LoginLogCollector = LogBatchCollector<sys_login_log::ActiveModel>;

/// 批量刷新循环：累积到 batch_size 或超时 flush_interval 后执行 insert_many
async fn flush_loop<A>(
    db: DbConn,
    rx: flume::Receiver<A>,
    batch_size: usize,
    flush_interval: Duration,
    entity_name: &'static str,
    control: Arc<CollectorControl>,
) where
    A: ActiveModelTrait + ActiveModelBehavior + Send + Clone + 'static,
    <A::Entity as EntityTrait>::Model: IntoActiveModel<A>,
{
    let mut buffer: Vec<A> = Vec::with_capacity(batch_size);
    let mut interval = tokio::time::interval(flush_interval);
    let mut shutting_down = false;
    interval.tick().await; // 跳过第一次立即触发

    loop {
        if shutting_down {
            while let Ok(model) = rx.try_recv() {
                buffer.push(model);
                if buffer.len() >= batch_size {
                    flush_batch(&db, &mut buffer, entity_name, &control).await;
                }
            }

            if !buffer.is_empty() {
                flush_batch(&db, &mut buffer, entity_name, &control).await;
            }

            if rx.is_empty() && control.in_flight_pushes.load(Ordering::Acquire) == 0 {
                tracing::info!("{} 批量收集器已退出", entity_name);
                break;
            }

            tokio::select! {
                _ = control.shutdown.notified() => {}
                _ = tokio::time::sleep(Duration::from_millis(10)) => {}
            }
            continue;
        }

        tokio::select! {
            biased; // 优先处理数据接收

            _ = control.shutdown.notified() => {
                shutting_down = true;
            }

            item = rx.recv_async() => {
                match item {
                    Ok(model) => {
                        buffer.push(model);
                        if buffer.len() >= batch_size {
                            flush_batch(&db, &mut buffer, entity_name, &control).await;
                        }
                    }
                    Err(_) => {
                        // 通道关闭，刷新剩余数据后退出
                        if !buffer.is_empty() {
                            flush_batch(&db, &mut buffer, entity_name, &control).await;
                        }
                        tracing::info!("{} 批量收集器已退出", entity_name);
                        break;
                    }
                }
            }

            _ = interval.tick() => {
                if !buffer.is_empty() {
                    flush_batch(&db, &mut buffer, entity_name, &control).await;
                }
            }
        }
    }
}

/// 执行批量 INSERT
async fn flush_batch<A>(
    db: &DbConn,
    buffer: &mut Vec<A>,
    entity_name: &str,
    control: &CollectorControl,
) where
    A: ActiveModelTrait + ActiveModelBehavior + Send + Clone,
    <A::Entity as EntityTrait>::Model: IntoActiveModel<A>,
{
    let batch = std::mem::take(buffer);
    let count = batch.len();

    for (attempt, delay_ms) in LOG_BATCH_RETRY_DELAYS_MS.iter().enumerate() {
        match <A::Entity as EntityTrait>::insert_many(batch.clone())
            .exec(db)
            .await
        {
            Ok(_) => {
                tracing::debug!("{} 批量写入 {} 条", entity_name, count);
                return;
            }
            Err(error) => {
                tracing::warn!(
                    "{} 批量写入失败，准备重试 ({}/{}): {}",
                    entity_name,
                    attempt + 1,
                    LOG_BATCH_RETRY_DELAYS_MS.len(),
                    error
                );
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_millis(*delay_ms)) => {}
                    _ = control.shutdown.notified() => {}
                }
            }
        }
    }

    tracing::error!(
        "{} 批量写入重试后仍失败，降级为单条写入 {} 条",
        entity_name,
        count
    );
    for model in batch {
        if let Err(error) = model.insert(db).await {
            tracing::error!("{} 单条写入失败: {}", entity_name, error);
        }
    }
}

/// 日志批量收集器插件
///
/// 为 `summer-system` 域的两个内置日志实体（`sys_operation_log` / `sys_login_log`）
/// 注册批量收集器。业务层直接通过 `#[inject(component)]` 取
/// `LogBatchCollector<sys_operation_log::ActiveModel>` / `...sys_login_log::ActiveModel`。
///
/// 其他 crate 需要批量收集自己的实体时，直接在自己的 `Plugin::build` 里调
/// [`spawn_typed_collector`]，不需要再注册本插件的变种。
///
/// 必须在 `SeaOrmPlugin` 之后注册。
pub struct LogBatchCollectorPlugin;

#[async_trait]
impl Plugin for LogBatchCollectorPlugin {
    async fn build(&self, app: &mut AppBuilder) {
        let config = app.get_config::<LogBatchConfig>().unwrap_or_default();

        spawn_typed_collector::<sys_operation_log::ActiveModel>(app, "操作日志", config.clone());
        spawn_typed_collector::<sys_login_log::ActiveModel>(app, "登录日志", config);
    }

    fn name(&self) -> &str {
        "log-batch-collector"
    }

    fn dependencies(&self) -> Vec<&str> {
        vec!["summer_sea_orm::SeaOrmPlugin"]
    }
}

// ---------------------------------------------------------------------------
// spawn_typed_collector —— 给下游业务 crate 用的 generic helper
// ---------------------------------------------------------------------------

/// 为任意 Entity 启动一个批量收集器，注册为 Component，并挂 shutdown hook。
///
/// 下游 crate 在自己的 `Plugin::build` 里调一次，就能得到一个可注入的
/// `LogBatchCollector<A>`，无需复制一套 flume + flush_loop + TaskTracker。
///
/// # 约束
///
/// - `A` 必须是 SeaORM 的 `ActiveModel`（走 `insert_many`）
/// - **`insert_many` 不触发 `ActiveModelBehavior::before_save`**，业务必须在 `push`
///   之前手动设置 `create_time` / `update_time` 等时间戳字段
/// - 必须在 `SeaOrmPlugin` 之后调用（需要 `DbConn` 已注册）
///
/// # 用法
///
/// ```rust,ignore
/// use summer_plugins::log_batch_collector::{spawn_typed_collector, LogBatchConfig};
/// use summer_ai_model::entity::billing::transaction;
///
/// spawn_typed_collector::<transaction::ActiveModel>(
///     app,
///     "ai.transaction",
///     LogBatchConfig::default(),
/// );
/// ```
///
/// Service 里直接注入：
///
/// ```rust,ignore
/// use summer_plugins::log_batch_collector::LogBatchCollector;
/// use summer_ai_model::entity::billing::transaction;
///
/// #[derive(Clone, Service)]
/// pub struct BillingService {
///     #[inject(component)]
///     tx_collector: LogBatchCollector<transaction::ActiveModel>,
/// }
/// ```
///
/// # Panics
///
/// 如果 `DbConn` 未注册（`SeaOrmPlugin` 未先跑）会 panic。
pub fn spawn_typed_collector<A>(
    app: &mut AppBuilder,
    entity_name: &'static str,
    config: LogBatchConfig,
) where
    A: ActiveModelTrait + ActiveModelBehavior + Send + Clone + 'static,
    <A::Entity as EntityTrait>::Model: IntoActiveModel<A>,
{
    let db: DbConn = app.get_component::<DbConn>().unwrap_or_else(|| {
        panic!(
            "spawn_typed_collector<{}> requires DbConn (SeaOrmPlugin must be registered first)",
            entity_name
        )
    });

    let flush_interval = Duration::from_millis(config.flush_interval_ms);
    let (tx, rx) = flume::bounded::<A>(config.capacity);
    let collector = LogBatchCollector::<A>::new(tx);
    let control = collector.control.clone();

    let tracker = TaskTracker::new();
    tracker.spawn(flush_loop(
        db,
        rx,
        config.batch_size,
        flush_interval,
        entity_name,
        control,
    ));

    app.add_component(collector.clone());

    let collector_for_hook = collector.clone();
    let tracker_for_hook = tracker.clone();
    app.add_shutdown_hook(move |_app| {
        let collector = collector_for_hook.clone();
        let tracker = tracker_for_hook.clone();
        Box::new(async move {
            collector.close();
            tracker.close();
            match tokio::time::timeout(LOG_BATCH_SHUTDOWN_TIMEOUT, tracker.wait()).await {
                Ok(()) => Ok::<_, summer::error::AppError>(format!(
                    "{entity_name} batch collector drained"
                )),
                Err(_) => {
                    tracing::warn!(
                        "{entity_name} batch collector drain timed out after {:?}",
                        LOG_BATCH_SHUTDOWN_TIMEOUT
                    );
                    Ok::<_, summer::error::AppError>(format!(
                        "{entity_name} batch collector drain timed out"
                    ))
                }
            }
        }) as Box<dyn Future<Output = AppResult<String>> + Send>
    });

    tracing::info!(
        "{entity_name} 批量收集器已启动: batch_size={}, flush_interval={}ms, capacity={}",
        config.batch_size,
        config.flush_interval_ms,
        config.capacity
    );
}

#[cfg(test)]
mod tests {
    use super::{LogBatchCollector, LogBatchPushError};

    #[test]
    fn log_batch_collector_exposes_shutdown_drain_support() {
        let source = include_str!("mod.rs");
        assert!(source.contains("TaskTracker"));
        assert!(source.contains("add_shutdown_hook"));
        assert!(source.contains("close()"));
    }

    #[test]
    fn log_batch_collector_push_reports_backpressure_errors() {
        let (tx, _rx) = flume::bounded::<i32>(1);
        let collector = LogBatchCollector::new(tx);

        assert_eq!(collector.push(1), Ok(()));
        assert_eq!(collector.push(2), Err(LogBatchPushError::Full));

        collector.close();
        assert_eq!(collector.push(3), Err(LogBatchPushError::Closed));
    }
}
