use summer::app::AppBuilder;
use summer::async_trait;
use summer::plugin::Plugin;

use crate::application::ApplicationModule;
use crate::domain::DomainModule;
use crate::infrastructure::InfrastructureModule;
use crate::interfaces::InterfaceModule;
use crate::interfaces::http::HttpInterfaceModule;

/// summer-ai-hub DDD 插件入口
pub struct SummerAiHubPlugin;

#[async_trait]
impl Plugin for SummerAiHubPlugin {
    async fn build(&self, _app: &mut AppBuilder) {
        let _ = ApplicationModule;
        let _ = DomainModule;
        let _ = InfrastructureModule;
        let _ = InterfaceModule;
        let _ = HttpInterfaceModule;
    }

    fn name(&self) -> &str {
        "summer_ai_hub::SummerAiHubPlugin"
    }
}
