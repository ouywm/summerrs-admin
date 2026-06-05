//! 仅 debug 构建注册的 panic handler，用于验证执行器的 panic 处理链路。
//! release 构建不会注册这个 handler。

#![cfg(debug_assertions)]

use summer::async_trait;
use summer_xxl_job::{AsyncJobHandler, JobContext};

/// admin 侧任务绑定的 handler 名。
pub const HANDLER_NAME: &str = "summer_system::test_panic";

/// [debug 专用] 故意 panic，用于验证执行器是否把 panic 正确转成失败回调而不是
/// 搞崩进程。release 构建不注册。
#[derive(Clone)]
pub struct TestPanicHandler;

#[async_trait]
impl AsyncJobHandler for TestPanicHandler {
    async fn process(&self, _ctx: JobContext) -> anyhow::Result<JobContext> {
        panic!("intentional panic for panic-handling test");
    }
}
