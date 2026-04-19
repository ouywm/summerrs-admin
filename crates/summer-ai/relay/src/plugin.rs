//! SummerAiRelayPlugin —— relay 域 Plugin 入口。
//!
//! 负责：
//! - 注册 `reqwest::Client` 作 Component
//! - 挂 `/v1/*` 路由（chat / models / 后续 embeddings/responses）
//! - AiAuthLayer 鉴权（P5 加）
//! - 流式任务 TaskTracker（shutdown 时 drain，P3 加）

use summer::app::AppBuilder;
use summer::async_trait;
use summer::plugin::{MutableComponentRegistry, Plugin};
use summer_web::LayerConfigurator;

use crate::router::relay_router;

/// relay 域 Plugin 入口。
pub struct SummerAiRelayPlugin;

#[async_trait]
impl Plugin for SummerAiRelayPlugin {
    async fn build(&self, app: &mut AppBuilder) {
        let http = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
        {
            Ok(client) => client,
            Err(error) => {
                tracing::error!(?error, "summer-ai-relay reqwest client build failed");
                return;
            }
        };
        app.add_component(http);

        app.add_router_layer(|router| router.merge(relay_router()));

        tracing::info!("summer-ai-relay plugin initialized (P0 skeleton + routes)");
    }

    fn name(&self) -> &str {
        "summer_ai_relay::SummerAiRelayPlugin"
    }
}
