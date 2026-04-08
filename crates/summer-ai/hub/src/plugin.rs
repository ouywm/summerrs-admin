use summer::app::AppBuilder;
use summer::async_trait;
use summer::plugin::Plugin;
use summer_ai_admin::SummerAiAdminPlugin;
use summer_ai_billing::SummerAiBillingPlugin;
use summer_ai_relay::SummerAiRelayPlugin;

/// summer-ai-hub 装配层插件入口
pub struct SummerAiHubPlugin;

#[async_trait]
impl Plugin for SummerAiHubPlugin {
    async fn build(&self, app: &mut AppBuilder) {
        SummerAiAdminPlugin.build(app).await;
        SummerAiBillingPlugin.build(app).await;
        SummerAiRelayPlugin.build(app).await;
    }

    fn name(&self) -> &str {
        "summer_ai_hub::SummerAiHubPlugin"
    }
}
