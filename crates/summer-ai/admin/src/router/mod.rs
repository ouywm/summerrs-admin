use summer_web::Router;

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

pub fn router() -> Router {
    summer_web::handler::grouped_router(crate::admin_group())
}
