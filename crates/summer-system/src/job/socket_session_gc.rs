//! Socket 会话索引 GC 清理 —— xxl-job 执行器 handler。
//!
//! 由 ratch-job / xxl-job-admin 远程下发调度。原 cron `0 */10 * * * *`（每 10 分钟），
//! 现在在 admin 控制台维护，绑定 handler 名 [`HANDLER_NAME`]。

use summer::async_trait;
use summer::plugin::service::Service;
use summer_xxl_job::{AsyncJobHandler, JobContext};
use tracing::{error, info};

use crate::socketio::service::SocketGatewayService;

/// admin 侧任务绑定的 handler 名。
pub const HANDLER_NAME: &str = "summer_system::socket_session_gc";

/// 清理 socket 会话索引中的幽灵条目。Redis 里偶尔会残留已断开连接但索引未
/// 清理的会话记录，本任务周期扫描并剔除这些失效条目。
#[derive(Clone, Service)]
pub struct SocketSessionGcHandler {
    #[inject(component)]
    service: SocketGatewayService,
}

#[async_trait]
impl AsyncJobHandler for SocketSessionGcHandler {
    async fn process(&self, ctx: JobContext) -> anyhow::Result<JobContext> {
        match self.service.gc_stale_index_entries().await {
            Ok(count) => {
                if count > 0 {
                    info!("Socket 会话 GC 清理了 {} 个过期索引条目", count);
                }
                Ok(ctx)
            }
            Err(e) => {
                error!(%e, "Socket 会话 GC 任务失败");
                Err(anyhow::anyhow!(e.to_string()))
            }
        }
    }
}
