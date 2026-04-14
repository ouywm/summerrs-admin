use std::sync::Arc;

use summer::app::AppBuilder;
use summer::async_trait;
use summer::config::ConfigRegistry;
use summer::plugin::{ComponentRegistry, MutableComponentRegistry, Plugin};
use summer_redis::Redis;
use summer_web::LayerConfigurator;

use crate::config::AuthConfig;
use crate::middleware::AuthLayer;
use crate::path_auth::PathAuthConfig;
use crate::session::SessionManager;
use crate::storage::redis::RedisStorage;
use crate::storage::AuthStorage;

pub struct SummerAuthPlugin;

#[async_trait]
impl Plugin for SummerAuthPlugin {
    async fn build(&self, app: &mut AppBuilder) {
        let config = app
            .get_config::<AuthConfig>()
            .expect("auth plugin config load failed");

        tracing::info!("Initializing summer-auth plugin...");

        let redis: Redis = app
            .get_component::<Redis>()
            .expect("redis component load failed")
            .into();

        // 创建存储后端
        let storage: Arc<dyn AuthStorage> = Arc::new(RedisStorage::new(redis));

        // 创建 SessionManager
        let manager = SessionManager::new(storage, config);
        app.add_component(manager.clone());

        // 注册中间件
        let path_config = app.get_component::<PathAuthConfig>();
        tracing::info!("Registering AuthLayer middleware");
        app.add_router_layer(move |router| {
            router.layer(AuthLayer::new(manager.clone(), path_config.clone()))
        });

        tracing::info!("summer-auth plugin initialized successfully");
    }

    fn name(&self) -> &str {
        "summer_auth::SummerAuthPlugin"
    }

    fn dependencies(&self) -> Vec<&str> {
        vec!["summer_redis::RedisPlugin"]
    }
}
