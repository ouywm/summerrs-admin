//! summer-job-dynamic
//!
//! 动态任务调度系统 —— 取代 `#[cron]` 硬编码模式，把任务定义放到 DB，支持网页 CRUD、
//! 启停、手动触发、执行日志、失败重试、超时杀任务、阻塞 / 路由 / 分片策略。
//!
//! 调度内核复用 `summer_job::JobPlugin` 已注册的 `tokio_cron_scheduler::JobScheduler`，
//! 静态 `#[cron]` 任务和 DB 动态任务跑在同一个调度器实例里。
//!
//! handler 注册通过独立的 `JobHandlerEntry` inventory，宏 `#[job_handler("name")]` 把
//! async fn 包成 `fn(JobContext) -> Future<JobResult>` 并注册到全局 registry，调度器
//! 按 `sys_job.handler` 字段查表执行。
//!
//! 当前为 A1 阶段（核心骨架），DB / scheduler 同步 / CRUD / worker 留给 A2。

pub mod context;
pub mod dto;
pub mod engine;
pub mod entity;
pub mod enums;
pub mod plugin;
pub mod registry;
pub mod router;
pub mod script;
pub mod service;

pub use context::{JobContext, JobError, JobResult};
pub use plugin::SummerSchedulerPlugin;
pub use registry::{BuiltinJob, HandlerFn, HandlerRegistry, JobHandlerEntry};
pub use router::router_with_layers;

pub use summer_admin_macros::job_handler;

#[doc(hidden)]
pub use inventory as __inventory;

/// 本 crate 路由分组名（auto group），与 `env!("CARGO_PKG_NAME")` 一致。
pub fn job_dynamic_group() -> &'static str {
    env!("CARGO_PKG_NAME")
}
