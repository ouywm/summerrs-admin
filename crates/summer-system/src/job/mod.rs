//! 系统域内置任务。
//!
//! 固定时间的内部任务使用 `summer-job` cron 表达式随应用启动注册；需要外部
//! 控制台编排或手动触发的任务再接入 summer-xxl-job/ratch-job。

pub mod ratch_client;
pub mod s3_cleanup;
pub mod socket_session_gc;

#[cfg(debug_assertions)]
pub mod test_panic;

use summer::app::AppBuilder;

#[cfg(debug_assertions)]
use summer_xxl_job::XxlJobConfigurator;

pub fn register_xxl_handlers(app: &mut AppBuilder) -> &mut AppBuilder {
    #[cfg(debug_assertions)]
    app.add_xxl_async_handler(test_panic::HANDLER_NAME, test_panic::TestPanicHandler);

    app
}
