use summer::app::AppBuilder;
use summer::async_trait;
use summer::plugin::Plugin;

pub struct SummerAiGatewayPlugin;

#[async_trait]
impl Plugin for SummerAiGatewayPlugin {
    async fn build(&self, _app: &mut AppBuilder) {
        // TODO: 加载配置、注册组件、挂载路由
    }

    fn name(&self) -> &str {
        "summer_ai_gateway::SummerAiGatewayPlugin"
    }
}
