use summer::app::AppBuilder;
use summer::async_trait;
use summer::plugin::{ComponentRegistry, MutableComponentRegistry, Plugin};

pub struct RateLimitPlugin;

#[async_trait]
impl Plugin for RateLimitPlugin {
    async fn build(&self, app: &mut AppBuilder) {
        let redis = app.get_component::<summer_redis::Redis>();
        let engine = summer_common::rate_limit::RateLimitEngine::new(redis);
        app.add_component(engine);
    }

    fn name(&self) -> &str {
        "summer_system::RateLimitPlugin"
    }
}
