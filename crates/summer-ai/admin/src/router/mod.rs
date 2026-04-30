use summer_auth::{GroupAuthLayer, JwtStrategy};
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

/// 收集本 crate 的 inventory 注册路由(不带 layer)。
///
/// 大多数情况下 app crate 应直接调 [`router_with_layers`];这个函数留给单测 / 自定义
/// 中间件栈场景使用。
pub fn router() -> Router {
    summer_web::handler::grouped_router(crate::admin_group())
}

/// 组装 admin 域 Router,挂上 JWT 鉴权 layer。
///
/// app crate 直接调这个函数即可,不需要 import [`JwtStrategy`]。
pub fn router_with_layers() -> Router {
    router().layer(GroupAuthLayer::new(JwtStrategy::for_group(
        crate::admin_group(),
    )))
}
