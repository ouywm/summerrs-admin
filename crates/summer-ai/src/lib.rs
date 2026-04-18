use summer::app::AppBuilder;
use summer::async_trait;
use summer::plugin::Plugin;

mod config;
mod llm;

pub struct SummerAiPlugin;

#[async_trait]
impl Plugin for SummerAiPlugin {
    async fn build(&self, _app: &mut AppBuilder) {}

    fn name(&self) -> &str {
        "summer_ai::SummerAiPlugin"
    }
}
