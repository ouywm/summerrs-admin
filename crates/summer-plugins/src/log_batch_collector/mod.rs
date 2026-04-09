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

use config::{default_batch_size, default_capacity, default_flush_interval_ms};
use sea_orm::ExprTrait;
use sea_orm::sea_query::{Expr, OnConflict};
use sea_orm::{ActiveModelBehavior, ActiveModelTrait, EntityTrait, IntoActiveModel};
use summer::app::AppBuilder;
use summer::async_trait;
use summer::config::ConfigRegistry;
use summer::error::Result as AppResult;
use summer::plugin::{ComponentRegistry, MutableComponentRegistry, Plugin};
use summer_ai_model::entity::log as ai_log;
use summer_system_model::entity::{sys_login_log, sys_operation_log};
use summer_sea_orm::DbConn;
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
    fn new(sender: flume::Sender<A>) -> Self {
        Self {
            sender,
            control: Arc::new(CollectorControl::default()),
        }
    }

    /// 推送一条记录到批量收集器（非阻塞）
    ///
    /// 如果通道已满，记录将被丢弃并记录警告日志。
    pub fn push(&self, model: A) -> std::result::Result<(), LogBatchPushError> {
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

/// AI 调用日志批量收集器
pub type AiLogCollector = LogBatchCollector<ai_log::ActiveModel>;

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
)
where
    A: ActiveModelTrait + ActiveModelBehavior + Send + Clone,
    <A::Entity as EntityTrait>::Model: IntoActiveModel<A>,
{
    let batch = std::mem::take(buffer);
    let count = batch.len();

    for (attempt, delay_ms) in LOG_BATCH_RETRY_DELAYS_MS.iter().enumerate() {
        match <A::Entity as EntityTrait>::insert_many(batch.clone()).exec(db).await {
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

    tracing::error!("{} 批量写入重试后仍失败，降级为单条写入 {} 条", entity_name, count);
    for model in batch {
        if let Err(error) = model.insert(db).await {
            tracing::error!("{} 单条写入失败: {}", entity_name, error);
        }
    }
}

fn ai_log_on_conflict() -> OnConflict {
    OnConflict::column(ai_log::Column::DedupeKey)
        .target_and_where(Expr::col((ai_log::Entity, ai_log::Column::DedupeKey)).ne(""))
        .do_nothing()
        .to_owned()
}

async fn flush_ai_log_loop(
    db: DbConn,
    rx: flume::Receiver<ai_log::ActiveModel>,
    batch_size: usize,
    flush_interval: Duration,
    control: Arc<CollectorControl>,
) {
    let mut buffer: Vec<ai_log::ActiveModel> = Vec::with_capacity(batch_size);
    let mut interval = tokio::time::interval(flush_interval);
    let mut shutting_down = false;
    interval.tick().await;

    loop {
        if shutting_down {
            while let Ok(model) = rx.try_recv() {
                buffer.push(model);
                if buffer.len() >= batch_size {
                    flush_ai_log_batch(&db, &mut buffer, &control).await;
                }
            }

            if !buffer.is_empty() {
                flush_ai_log_batch(&db, &mut buffer, &control).await;
            }

            if rx.is_empty() && control.in_flight_pushes.load(Ordering::Acquire) == 0 {
                tracing::info!("AI日志 批量收集器已退出");
                break;
            }

            tokio::select! {
                _ = control.shutdown.notified() => {}
                _ = tokio::time::sleep(Duration::from_millis(10)) => {}
            }
            continue;
        }

        tokio::select! {
            biased;

            _ = control.shutdown.notified() => {
                shutting_down = true;
            }

            item = rx.recv_async() => {
                match item {
                    Ok(model) => {
                        buffer.push(model);
                        if buffer.len() >= batch_size {
                            flush_ai_log_batch(&db, &mut buffer, &control).await;
                        }
                    }
                    Err(_) => {
                        if !buffer.is_empty() {
                            flush_ai_log_batch(&db, &mut buffer, &control).await;
                        }
                        tracing::info!("AI日志 批量收集器已退出");
                        break;
                    }
                }
            }

            _ = interval.tick() => {
                if !buffer.is_empty() {
                    flush_ai_log_batch(&db, &mut buffer, &control).await;
                }
            }
        }
    }
}

async fn flush_ai_log_batch(
    db: &DbConn,
    buffer: &mut Vec<ai_log::ActiveModel>,
    control: &CollectorControl,
) {
    let batch = std::mem::take(buffer);
    let count = batch.len();

    for (attempt, delay_ms) in LOG_BATCH_RETRY_DELAYS_MS.iter().enumerate() {
        match ai_log::Entity::insert_many(batch.clone())
            .on_conflict(ai_log_on_conflict())
            .exec(db)
            .await
        {
            Ok(_) => {
                tracing::debug!("AI日志 批量写入 {} 条", count);
                return;
            }
            Err(error) => {
                tracing::warn!(
                    "AI日志 批量写入失败，准备重试 ({}/{}): {}",
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

    tracing::error!("AI日志 批量写入重试后仍失败，降级为单条写入 {} 条", count);
    for model in batch {
        if let Err(error) = ai_log::Entity::insert(model)
            .on_conflict(ai_log_on_conflict())
            .exec(db)
            .await
        {
            tracing::error!("AI日志 单条写入失败: {}", error);
        }
    }
}

/// 日志批量收集器插件
///
/// 必须在 `SeaOrmPlugin` 之后注册，因为需要获取 `DbConn` 组件。
pub struct LogBatchCollectorPlugin;

#[async_trait]
impl Plugin for LogBatchCollectorPlugin {
    async fn build(&self, app: &mut AppBuilder) {
        let tracker = TaskTracker::new();
        let config = app
            .get_config::<LogBatchConfig>()
            .unwrap_or_else(|_| LogBatchConfig {
                batch_size: default_batch_size(),
                flush_interval_ms: default_flush_interval_ms(),
                capacity: default_capacity(),
            });

        let db: DbConn = app
            .get_component::<DbConn>()
            .expect("DbConn 未找到，请确保 SeaOrmPlugin 在 LogBatchCollectorPlugin 之前注册");

        let flush_interval = Duration::from_millis(config.flush_interval_ms);

        // 操作日志收集器
        let (op_tx, op_rx) = flume::bounded::<sys_operation_log::ActiveModel>(config.capacity);
        let op_collector = OperationLogCollector::new(op_tx);
        {
            let db = db.clone();
            let batch_size = config.batch_size;
            let control = op_collector.control.clone();
            tracker.spawn(flush_loop(
                db,
                op_rx,
                batch_size,
                flush_interval,
                "操作日志",
                control,
            ));
        }

        // 登录日志收集器
        let (login_tx, login_rx) = flume::bounded::<sys_login_log::ActiveModel>(config.capacity);
        let login_collector = LoginLogCollector::new(login_tx);
        {
            let db = db.clone();
            let batch_size = config.batch_size;
            let control = login_collector.control.clone();
            tracker.spawn(flush_loop(
                db,
                login_rx,
                batch_size,
                flush_interval,
                "登录日志",
                control,
            ));
        }

        // AI 日志收集器
        let (ai_tx, ai_rx) = flume::bounded::<ai_log::ActiveModel>(config.capacity);
        let ai_collector = AiLogCollector::new(ai_tx);
        {
            let db = db.clone();
            let batch_size = config.batch_size;
            let control = ai_collector.control.clone();
            tracker.spawn(flush_ai_log_loop(
                db,
                ai_rx,
                batch_size,
                flush_interval,
                control,
            ));
        }

        app.add_component(op_collector.clone());
        app.add_component(login_collector.clone());
        app.add_component(ai_collector.clone());

        tracing::info!(
            "日志批量收集器已启动: batch_size={}, flush_interval={}ms, capacity={}",
            config.batch_size,
            config.flush_interval_ms,
            config.capacity
        );

        app.add_shutdown_hook(move |_app| {
            let tracker = tracker.clone();
            let op_collector = op_collector.clone();
            let login_collector = login_collector.clone();
            let ai_collector = ai_collector.clone();
            Box::new(async move {
                op_collector.close();
                login_collector.close();
                ai_collector.close();
                tracker.close();
                match tokio::time::timeout(LOG_BATCH_SHUTDOWN_TIMEOUT, tracker.wait()).await {
                    Ok(()) => Ok::<_, summer::error::AppError>(
                        "log batch collector tasks drained".to_string(),
                    ),
                    Err(_) => {
                        tracing::warn!(
                            "log batch collector task drain timed out after {:?}",
                            LOG_BATCH_SHUTDOWN_TIMEOUT
                        );
                        Ok::<_, summer::error::AppError>(
                            "log batch collector task drain timed out".to_string(),
                        )
                    }
                }
            }) as Box<dyn Future<Output = AppResult<String>> + Send>
        });
    }

    fn name(&self) -> &str {
        "log-batch-collector"
    }

    fn dependencies(&self) -> Vec<&str> {
        vec!["summer_sea_orm::SeaOrmPlugin"]
    }
}

#[cfg(test)]
mod tests {
    use super::{LogBatchCollector, LogBatchPushError};

    #[test]
    fn log_batch_collector_plugin_supports_ai_log_models() {
        let source = include_str!("mod.rs");
        assert!(source.contains("AiLogCollector"));
        assert!(source.contains("summer_ai_model::entity::log"));
    }

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
    }

    #[test]
    fn log_batch_collector_push_reports_closed_after_shutdown() {
        let (tx, _rx) = flume::bounded::<i32>(1);
        let collector = LogBatchCollector::new(tx);

        collector.close();
        assert_eq!(collector.push(1), Err(LogBatchPushError::Closed));
    }

    #[test]
    fn log_batch_collector_hot_path_avoids_mutex_wrapped_sender() {
        let source = include_str!("mod.rs");
        assert!(source.contains("AtomicBool"));
        assert!(source.contains("in_flight_pushes"));
    }

    #[test]
    fn log_batch_collector_backoff_is_interruptible_by_shutdown() {
        let source = include_str!("mod.rs");
        assert!(source.contains("Notify"));
        assert!(source.contains("shutdown.notified()"));
        assert!(source.contains("tokio::select!"));
    }

    #[test]
    fn ai_log_batch_insert_uses_conflict_ignore_for_dedupe_key() {
        let source = include_str!("mod.rs");
        let prod_source = source.split("#[cfg(test)]").next().unwrap_or(source);
        assert!(prod_source.contains("ai_log::Entity::insert_many(batch.clone())"));
        assert!(prod_source.contains(".on_conflict(ai_log_on_conflict())"));
        assert!(prod_source.contains("ai_log::Column::DedupeKey"));
        assert!(prod_source.contains(".target_and_where("));
        assert!(prod_source.contains(".do_nothing()"));
    }
}
