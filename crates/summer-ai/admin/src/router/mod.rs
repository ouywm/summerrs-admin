pub mod ability;
pub mod channel;
pub mod channel_account;
pub mod channel_model_price;
pub mod config_entry;
pub mod daily_stats;
pub mod group_ratio;
pub mod model_config;
pub mod openai_oauth;
pub mod request_log;
pub mod routing_rule;
pub mod routing_target;
pub mod token;
pub mod user_quota;
pub mod vendor;

use summer_web::Router;

pub fn admin_router() -> Router {
    let router = Router::new();
    let router = ability::routes(router);
    let router = channel::routes(router);
    let router = channel_account::routes(router);
    let router = channel_model_price::routes(router);
    let router = config_entry::routes(router);
    let router = token::routes(router);
    let router = group_ratio::routes(router);
    let router = request_log::routes(router);
    let router = daily_stats::routes(router);
    let router = user_quota::routes(router);
    let router = model_config::routes(router);
    let router = openai_oauth::routes(router);
    let router = routing_rule::routes(router);
    let router = routing_target::routes(router);
    vendor::routes(router)
}
