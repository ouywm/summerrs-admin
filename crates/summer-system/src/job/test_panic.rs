//! 仅 debug 构建注册的 panic handler，用于验证 worker 的 catch_unwind 链路。
//! release 构建不会有这个 handler，前端下拉也看不到。

#![cfg(debug_assertions)]

use summer_admin_macros::job_handler;
use summer_job_dynamic::{JobContext, JobResult};

/// [debug 专用] 故意 panic，用于验证 worker 的 catch_unwind 链路是否把 panic
/// 正确转成 FAILED 而不是搞崩进程。release 构建不注册，生产网页看不到。
#[job_handler("summer_system::test_panic")]
async fn test_panic(_ctx: JobContext) -> JobResult {
    panic!("intentional panic for catch_unwind test");
}
