//! Socket 会话索引 GC 清理

use summer::extractor::Component;
use summer_job::cron;
use tracing::{error, info};

use crate::socketio::service::SocketGatewayService;

/// 每 10 分钟执行：扫描全局 socket 索引，清理 session 已过期的幽灵条目
#[cron("0 */10 * * * *")]
async fn socket_session_gc(Component(service): Component<SocketGatewayService>) {
    match service.gc_stale_index_entries().await {
        Ok(count) if count > 0 => {
            info!("Socket 会话 GC 清理了 {} 个过期索引条目", count);
        }
        Err(e) => {
            error!(%e, "Socket 会话 GC 任务失败");
        }
        _ => {}
    }
}
