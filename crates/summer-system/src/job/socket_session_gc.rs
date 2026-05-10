//! Socket 会话索引 GC 清理 —— 动态调度任务。

use summer_admin_macros::job_handler;
use summer_job_dynamic::dto::CreateJobDto;
use summer_job_dynamic::enums::ScheduleType;
use summer_job_dynamic::{JobContext, JobError, JobResult};
use tracing::{error, info};

use crate::socketio::service::SocketGatewayService;

pub const HANDLER_NAME: &str = "summer_system::socket_session_gc";

fn default_dto() -> CreateJobDto {
    CreateJobDto {
        name: "socket-session-gc".to_string(),
        group_name: Some("system".to_string()),
        description: Some("清理 socket 会话索引中的幽灵条目".to_string()),
        handler: HANDLER_NAME.to_string(),
        schedule_type: ScheduleType::Cron,
        cron_expr: Some("0 */10 * * * *".to_string()),
        interval_ms: None,
        fire_time: None,
        params_json: None,
        enabled: Some(true),
        timeout_ms: Some(0),
        retry_max: Some(0),
        tenant_id: None,
    }
}

inventory::submit!(summer_job_dynamic::BuiltinJob {
    dto_factory: default_dto,
});

/// 清理 socket 会话索引中的幽灵条目。Redis 里偶尔会残留已断开连接但索引未
/// 清理的会话记录，本任务周期扫描并剔除这些失效条目。
#[job_handler("summer_system::socket_session_gc")]
async fn socket_session_gc(ctx: JobContext) -> JobResult {
    let service: SocketGatewayService = ctx.component();
    match service.gc_stale_index_entries().await {
        Ok(count) => {
            if count > 0 {
                info!("Socket 会话 GC 清理了 {} 个过期索引条目", count);
            }
            Ok(serde_json::json!({"cleaned": count}))
        }
        Err(e) => {
            error!(%e, "Socket 会话 GC 任务失败");
            Err(JobError::Handler(anyhow::anyhow!(e.to_string())))
        }
    }
}
