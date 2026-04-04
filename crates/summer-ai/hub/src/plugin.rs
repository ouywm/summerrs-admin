use summer::app::AppBuilder;
use summer::async_trait;
use summer::plugin::Plugin;

pub struct SummerAiHubPlugin;

#[async_trait]
impl Plugin for SummerAiHubPlugin {
    async fn build(&self, _app: &mut AppBuilder) {}

    fn name(&self) -> &str {
        "summer_ai_hub::SummerAiHubPlugin"
    }
}
