//! 路由模块

pub mod channel;
pub mod channel_account;
pub mod channel_model_price;
pub mod request;
pub mod request_execution;
pub mod retry_attempt;
pub mod vendor;

use summer_web::Router;

pub fn routes() -> Router {
    Router::new()
        .merge(channel::routes())
        .merge(channel_account::routes())
        .merge(channel_model_price::routes())
        .merge(request::routes())
        .merge(request_execution::routes())
        .merge(retry_attempt::routes())
        .merge(vendor::routes())
}
