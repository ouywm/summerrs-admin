//! Socket 会话索引 GC 清理 —— summer-job 固定周期任务。
//!
//! 固定时间任务由应用本地 cron 表达式维护；需要外部控制台调度的任务再接入
//! xxl-job/ratch-job。

use summer::extractor::Component;
use summer_job::cron;
use tracing::{error, info};

use crate::socketio::service::SocketGatewayService;

/// 每 10 分钟清理 socket 会话索引中的幽灵条目。
#[cron("0 */10 * * * *")]
pub async fn socket_session_gc_job(Component(service): Component<SocketGatewayService>) {
    if let Err(e) = run_socket_session_gc(&service).await {
        error!(%e, "Socket 会话 GC 任务失败");
    }
}

/// Redis 里偶尔会残留已断开连接但索引未清理的会话记录，本任务周期扫描并剔除
/// 这些失效条目。
async fn run_socket_session_gc(service: &SocketGatewayService) -> anyhow::Result<()> {
    match service.gc_stale_index_entries().await {
        Ok(count) => {
            if count > 0 {
                info!("Socket 会话 GC 清理了 {} 个过期索引条目", count);
            }
            Ok(())
        }
        Err(e) => {
            error!(%e, "Socket 会话 GC 任务失败");
            Err(anyhow::anyhow!(e.to_string()))
        }
    }
}
