use std::sync::Arc;

use summer::app::AppBuilder;
use summer::async_trait;
use summer::config::ConfigRegistry;
use summer::plugin::{ComponentRegistry, MutableComponentRegistry, Plugin};
use summer_web::LayerConfigurator;

use crate::config::AuthConfig;
use crate::middleware::AuthLayer;
use crate::path_auth::PathAuthConfig;
use crate::session::SessionManager;
use crate::storage::AuthStorage;

pub struct SummerAuthPlugin;

#[async_trait]
impl Plugin for SummerAuthPlugin {
    async fn build(&self, app: &mut AppBuilder) {
        let config = app
            .get_config::<AuthConfig>()
            .expect("auth plugin config load failed");

        tracing::info!("Initializing summer-auth plugin...");

        // 创建存储后端
        let storage: Arc<dyn AuthStorage> = Self::create_storage(app);

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
        #[cfg(feature = "redis")]
        {
            vec!["summer_redis::RedisPlugin"]
        }
        #[cfg(not(feature = "redis"))]
        {
            vec![]
        }
    }
}

impl SummerAuthPlugin {
    #[allow(unused_variables)]
    fn create_storage(app: &AppBuilder) -> Arc<dyn AuthStorage> {
        // 优先使用 Redis
        #[cfg(feature = "redis")]
        {
            if let Some(redis) = app.get_component::<summer_redis::Redis>() {
                tracing::info!("Using Redis storage for summer-auth");
                Arc::new(crate::storage::redis::RedisStorage::new(redis))
            } else {
                panic!(
                    "Feature 'redis' is enabled but RedisPlugin is not added. \
                     Please add RedisPlugin before SummerAuthPlugin."
                );
            }
        }

        // 回退到内存存储
        #[cfg(all(feature = "memory", not(feature = "redis")))]
        {
            tracing::info!("Using Memory storage for summer-auth");
            Arc::new(crate::storage::memory::MemoryStorage::new())
        }

        #[cfg(not(any(feature = "memory", feature = "redis")))]
        {
            panic!("No storage backend available. Enable 'memory' or 'redis' feature.");
        }
    }
}
