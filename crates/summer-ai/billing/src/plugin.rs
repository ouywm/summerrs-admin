use summer::app::AppBuilder;
use summer::async_trait;
use summer::plugin::Plugin;

/// summer-ai-billing 计费域插件入口
pub struct SummerAiBillingPlugin;

#[async_trait]
impl Plugin for SummerAiBillingPlugin {
    async fn build(&self, _app: &mut AppBuilder) {}

    fn name(&self) -> &str {
        "summer_ai_billing::SummerAiBillingPlugin"
    }
}
