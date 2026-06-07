//! 系统域内置任务 handler —— 接入 summer-xxl-job 执行器。
//!
//! 每个 handler 由 ratch-job / xxl-job-admin 远程下发调度，调度配置在 admin
//! 控制台维护。需要依赖注入的 handler 用 `add_xxl_async_service` 注册，
//! 无依赖的用 `add_xxl_async_handler`。调用 [`register_xxl_handlers`] 一次性
//! 完成全部注册。

pub mod ratch_client;
pub mod s3_cleanup;
pub mod socket_session_gc;

#[cfg(debug_assertions)]
pub mod test_panic;

use summer::app::AppBuilder;
use summer_xxl_job::XxlJobConfigurator;

/// 把系统域全部内置任务 handler 注册到 summer-xxl-job 执行器。
///
/// 在 `App` 构建链里调用，需在 `XxlJobPlugin` 之后。需要 DI 的 handler 通过
/// `add_xxl_async_service::<H>` 在依赖注入完成后惰性构造。
pub fn register_xxl_handlers(app: &mut AppBuilder) -> &mut AppBuilder {
    app.add_xxl_async_service::<s3_cleanup::S3MultipartCleanupHandler>(s3_cleanup::HANDLER_NAME)
        .add_xxl_async_service::<socket_session_gc::SocketSessionGcHandler>(
            socket_session_gc::HANDLER_NAME,
        );

    #[cfg(debug_assertions)]
    app.add_xxl_async_handler(test_panic::HANDLER_NAME, test_panic::TestPanicHandler);

    app
}
