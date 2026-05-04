//! 仅 debug 构建注册的测试 handler，用于集成验证 Serial / Override 阻塞策略。
//! release 构建不会有这些 handler。

#![cfg(debug_assertions)]

use std::time::Duration;

use summer_admin_macros::job_handler;
use summer_job_dynamic::{JobContext, JobError, JobResult};

/// 5 秒不可取消 sleep；跑完返回 `{"slept": 5000}`。用于 Serial 排队验证。
#[job_handler("summer_system::test_slow_5s")]
async fn test_slow_5s(_ctx: JobContext) -> JobResult {
    tokio::time::sleep(Duration::from_secs(5)).await;
    Ok(serde_json::json!({"slept": 5000}))
}

/// 5 秒 cooperative sleep —— 每 200ms 检查一次 cancel，被 cancel 时立刻返回 Canceled。
/// 用于 Override 抢占验证。
#[job_handler("summer_system::test_cooperative_5s")]
async fn test_cooperative_5s(ctx: JobContext) -> JobResult {
    let total = Duration::from_secs(5);
    let tick = Duration::from_millis(200);
    let mut elapsed = Duration::ZERO;
    while elapsed < total {
        ctx.check_cancel()?; // 被 cancel 时返回 Err(JobError::Canceled)
        tokio::time::sleep(tick).await;
        elapsed += tick;
    }
    let _ = JobError::Canceled; // 防 unused
    Ok(serde_json::json!({"slept": elapsed.as_millis()}))
}
