use summer::component;
use summer::extractor::Component;
use summer_redis::Redis;

// pub struct RateLimitPlugin;
//
// #[async_trait]
// impl Plugin for RateLimitPlugin {
//     async fn build(&self, app: &mut AppBuilder) {
//         let redis = app.get_component::<summer_redis::Redis>();
//         let engine = summer_common::rate_limit::RateLimitEngine::new(redis);
//         app.add_component(engine);
//     }
//
//     fn name(&self) -> &str {
//         "summer_system::RateLimitPlugin"
//     }
//
//     fn dependencies(&self) -> Vec<&str> {
//         vec!["summer_redis::RedisPlugin"]
//     }
// }

#[component]
pub fn rate_limit(
    #[inject("summer_redis::RedisPlugin")] Component(redis): Component<Redis>,
) -> summer_common::rate_limit::RateLimitEngine {
    summer_common::rate_limit::RateLimitEngine::new(Some(redis))
}
