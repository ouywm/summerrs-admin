use summer::app::AppBuilder;
use summer::async_trait;
use summer::plugin::Plugin;

/// summer-ai-admin 管理域插件入口
pub struct SummerAiAdminPlugin;

#[async_trait]
impl Plugin for SummerAiAdminPlugin {
    async fn build(&self, _app: &mut AppBuilder) {}

    fn name(&self) -> &str {
        "summer_ai_admin::SummerAiAdminPlugin"
    }
}
