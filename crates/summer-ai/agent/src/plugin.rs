use summer::app::AppBuilder;
use summer::async_trait;
use summer::plugin::Plugin;

pub struct SummerAiAgentPlugin;

#[async_trait]
impl Plugin for SummerAiAgentPlugin {
    async fn build(&self, _app: &mut AppBuilder) {}
}
