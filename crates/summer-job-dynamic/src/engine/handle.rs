//! `SchedulerHandle` —— build 阶段先注册 placeholder，schedule 钩子里 install
//! 真正的 [`DynamicScheduler`]。
//!
//! 当前 crate 是单实例调度器，service 层通过这个 handle 直接同步本进程内的
//! scheduler 运行态，不再经过跨进程消息总线或选举。

use std::sync::Arc;

use tokio::sync::RwLock;

use crate::engine::scheduler::DynamicScheduler;

#[derive(Clone, Default)]
pub struct SchedulerHandle {
    inner: Arc<RwLock<Inner>>,
}

#[derive(Default)]
struct Inner {
    scheduler: Option<DynamicScheduler>,
}

impl SchedulerHandle {
    pub async fn install(&self, scheduler: DynamicScheduler) {
        let mut inner = self.inner.write().await;
        inner.scheduler = Some(scheduler);
    }

    pub async fn current(&self) -> Option<DynamicScheduler> {
        self.inner.read().await.scheduler.clone()
    }

    pub async fn is_installed(&self) -> bool {
        self.inner.read().await.scheduler.is_some()
    }
}
