use std::future::Future;
use std::time::Duration;

use crate::auth::middleware::AiAuthLayer;
use summer::app::AppBuilder;
use summer::async_trait;
use summer::error::Result;
use summer::plugin::{MutableComponentRegistry, Plugin};
use summer_web::LayerConfigurator;
use tokio_util::task::TaskTracker;

const RELAY_STREAM_TASK_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Default)]
pub struct RelayStreamTaskTracker {
    inner: TaskTracker,
}

impl RelayStreamTaskTracker {
    pub fn new() -> Self {
        Self {
            inner: TaskTracker::new(),
        }
    }

    pub fn spawn<F>(&self, future: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.inner.spawn(future);
    }

    pub fn close(&self) {
        self.inner.close();
    }

    pub async fn wait(&self) {
        self.inner.wait().await;
    }
}

/// summer-ai-relay Relay 域插件入口
pub struct SummerAiRelayPlugin;

#[async_trait]
impl Plugin for SummerAiRelayPlugin {
    async fn build(&self, app: &mut AppBuilder) {
        let tracker = RelayStreamTaskTracker::new();
        app.add_component(reqwest::Client::new());
        app.add_component(tracker.clone());
        app.add_router_layer(|router| router.route_layer(AiAuthLayer::new()));
        app.add_shutdown_hook(move |_app| {
            Box::new(async move {
                tracker.close();
                match tokio::time::timeout(RELAY_STREAM_TASK_SHUTDOWN_TIMEOUT, tracker.wait()).await
                {
                    Ok(()) => Ok::<_, summer::error::AppError>(
                        "summer-ai relay stream tasks drained".to_string(),
                    ),
                    Err(_) => {
                        tracing::warn!(
                            "summer-ai relay stream task drain timed out after {:?}",
                            RELAY_STREAM_TASK_SHUTDOWN_TIMEOUT
                        );
                        Ok::<_, summer::error::AppError>(
                            "summer-ai relay stream task drain timed out".to_string(),
                        )
                    }
                }
            }) as Box<dyn Future<Output = Result<String>> + Send>
        });
    }

    fn name(&self) -> &str {
        "summer_ai_relay::SummerAiRelayPlugin"
    }
}
