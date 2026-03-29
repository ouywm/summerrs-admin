use crate::auth::middleware::AiAuthLayer;
use crate::relay::http_client::UpstreamHttpClient;
use crate::service::log_batch::AiLogBatchQueue;
use summer::app::AppBuilder;
use summer::async_trait;
use summer::config::ConfigRegistry;
use summer::plugin::{ComponentRegistry, MutableComponentRegistry, Plugin};
use summer_plugins::background_task::BackgroundTaskConfig;
use summer_plugins::log_batch_collector::LogBatchConfig;
use summer_sea_orm::DbConn;
use summer_web::LayerConfigurator;

pub struct SummerAiHubPlugin;

#[async_trait]
impl Plugin for SummerAiHubPlugin {
    async fn build(&self, app: &mut AppBuilder) {
        tracing::info!("Initializing summer-ai-hub plugin...");

        let db: DbConn = app
            .get_component::<DbConn>()
            .expect("DbConn not found; ensure SeaOrmPlugin is registered before SummerAiHubPlugin");
        let task_config =
            app.get_config::<BackgroundTaskConfig>()
                .unwrap_or(BackgroundTaskConfig {
                    capacity: 4096,
                    workers: 4,
                });
        let batch_config = app
            .get_config::<LogBatchConfig>()
            .unwrap_or(LogBatchConfig {
                batch_size: 50,
                flush_interval_ms: 500,
                capacity: 4096,
            });

        let http_client =
            UpstreamHttpClient::build().expect("failed to build shared upstream http client");
        let ai_log_queue = AiLogBatchQueue::build(db.clone(), &task_config, &batch_config);
        app.add_component(http_client);
        app.add_component(ai_log_queue);

        app.add_router_layer(|r| r.route_layer(AiAuthLayer::new()));
    }

    fn name(&self) -> &str {
        "summer_ai_hub::SummerAiHubPlugin"
    }

    fn dependencies(&self) -> Vec<&str> {
        vec!["summer_sea_orm::SeaOrmPlugin", "summer_redis::RedisPlugin"]
    }
}
