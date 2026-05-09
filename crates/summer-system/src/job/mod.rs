//! 系统域内置任务定义。
//!
//! 每个 handler 通过 `#[job_handler]` 注册到 `summer-job-dynamic` registry，
//! 同时通过 `inventory::submit!(BuiltinJob { ... })` 注册默认 DTO，启动期由
//! `SummerSchedulerPlugin::start` 一次性 import 到 DB（已存在则保留 DB 配置）。

pub mod s3_cleanup;
pub mod socket_session_gc;

#[cfg(debug_assertions)]
pub mod test_panic;
