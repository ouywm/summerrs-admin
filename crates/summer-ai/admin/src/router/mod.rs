//! 路由模块

pub mod alert_event;
pub mod alert_rule;
pub mod alert_silence;
pub mod channel;
pub mod channel_account;
pub mod channel_model_price;
pub mod daily_stats;
pub mod guardrail;
pub mod request;
pub mod request_execution;
pub mod retry_attempt;
pub mod vendor;

use summer_web::Router;

pub fn routes() -> Router {
    Router::new()
        .merge(alert_rule::routes())
        .merge(alert_event::routes())
        .merge(alert_silence::routes())
        .merge(channel::routes())
        .merge(channel_account::routes())
        .merge(channel_model_price::routes())
        .merge(daily_stats::routes())
        .merge(guardrail::routes())
        .merge(request::routes())
        .merge(request_execution::routes())
        .merge(retry_attempt::routes())
        .merge(vendor::routes())
}
