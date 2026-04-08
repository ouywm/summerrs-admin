use crate::auth::middleware::AiAuthLayer;
use summer::app::AppBuilder;
use summer::async_trait;
use summer::plugin::{MutableComponentRegistry, Plugin};
use summer_web::LayerConfigurator;

/// summer-ai-relay Relay 域插件入口
pub struct SummerAiRelayPlugin;

#[async_trait]
impl Plugin for SummerAiRelayPlugin {
    async fn build(&self, app: &mut AppBuilder) {
        app.add_component(reqwest::Client::new());
        app.add_router_layer(|router| router.route_layer(AiAuthLayer::new()));
    }

    fn name(&self) -> &str {
        "summer_ai_relay::SummerAiRelayPlugin"
    }
}
