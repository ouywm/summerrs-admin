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

use std::time::Duration;

use config::{default_batch_size, default_capacity, default_flush_interval_ms};
use summer_model::entity::{sys_login_log, sys_operation_log};
use sea_orm::{ActiveModelTrait, EntityTrait};
use summer::app::AppBuilder;
use summer::async_trait;
use summer::config::ConfigRegistry;
use summer::plugin::{ComponentRegistry, MutableComponentRegistry, Plugin};

use summer_sea_orm::DbConn;

/// 通用日志批量收集器
#[derive(Clone)]
pub struct LogBatchCollector<A: Send + 'static> {
    sender: flume::Sender<A>,
}

impl<A: Send + 'static> LogBatchCollector<A> {
    /// 推送一条记录到批量收集器（非阻塞）
    ///
    /// 如果通道已满，记录将被丢弃并记录警告日志。
    pub fn push(&self, model: A) {
        if let Err(e) = self.sender.try_send(model) {
            match e {
                flume::TrySendError::Full(_) => {
                    tracing::warn!("日志批量收集器已满，记录被丢弃");
                }
                flume::TrySendError::Disconnected(_) => {
                    tracing::error!("日志批量收集器已关闭，记录被丢弃");
                }
            }
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
) where
    A: ActiveModelTrait + Send + 'static,
{
    let mut buffer: Vec<A> = Vec::with_capacity(batch_size);
    let mut interval = tokio::time::interval(flush_interval);
    interval.tick().await; // 跳过第一次立即触发

    loop {
        tokio::select! {
            biased; // 优先处理数据接收

            item = rx.recv_async() => {
                match item {
                    Ok(model) => {
                        buffer.push(model);
                        if buffer.len() >= batch_size {
                            flush_batch(&db, &mut buffer, entity_name).await;
                        }
                    }
                    Err(_) => {
                        // 通道关闭，刷新剩余数据后退出
                        if !buffer.is_empty() {
                            flush_batch(&db, &mut buffer, entity_name).await;
                        }
                        tracing::info!("{} 批量收集器已退出", entity_name);
                        break;
                    }
                }
            }

            _ = interval.tick() => {
                if !buffer.is_empty() {
                    flush_batch(&db, &mut buffer, entity_name).await;
                }
            }
        }
    }
}

/// 执行批量 INSERT
async fn flush_batch<A>(db: &DbConn, buffer: &mut Vec<A>, entity_name: &str)
where
    A: ActiveModelTrait + Send,
{
    let batch: Vec<A> = buffer.drain(..).collect();
    let count = batch.len();

    match <A::Entity as EntityTrait>::insert_many(batch)
        .exec(db)
        .await
    {
        Ok(_) => {
            tracing::debug!("{} 批量写入 {} 条", entity_name, count);
        }
        Err(e) => {
            tracing::error!("{} 批量写入失败 ({} 条): {}", entity_name, count, e);
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
        {
            let db = db.clone();
            let batch_size = config.batch_size;
            tokio::spawn(flush_loop(
                db,
                op_rx,
                batch_size,
                flush_interval,
                "操作日志",
            ));
        }

        // 登录日志收集器
        let (login_tx, login_rx) = flume::bounded::<sys_login_log::ActiveModel>(config.capacity);
        {
            let db = db.clone();
            let batch_size = config.batch_size;
            tokio::spawn(flush_loop(
                db,
                login_rx,
                batch_size,
                flush_interval,
                "登录日志",
            ));
        }

        app.add_component(OperationLogCollector { sender: op_tx });
        app.add_component(LoginLogCollector { sender: login_tx });

        tracing::info!(
            "日志批量收集器已启动: batch_size={}, flush_interval={}ms, capacity={}",
            config.batch_size,
            config.flush_interval_ms,
            config.capacity
        );
    }

    fn name(&self) -> &str {
        "log-batch-collector"
    }

    fn dependencies(&self) -> Vec<&str> {
        vec!["summer_sea_orm::SeaOrmPlugin"]
    }
}
