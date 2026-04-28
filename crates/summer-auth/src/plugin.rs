//! SummerAuthPlugin - 注册 SessionManager 组件

use std::sync::Arc;

use summer::app::AppBuilder;
use summer::async_trait;
use summer::config::ConfigRegistry;
use summer::plugin::{ComponentRegistry, MutableComponentRegistry, Plugin};
use summer_redis::Redis;

use crate::config::AuthConfig;
use crate::session::SessionManager;
use crate::storage::AuthStorage;
use crate::storage::redis::RedisStorage;

pub struct SummerAuthPlugin;

#[async_trait]
impl Plugin for SummerAuthPlugin {
    async fn build(&self, app: &mut AppBuilder) {
        let config = app
            .get_config::<AuthConfig>()
            .expect("auth config load failed");

        let redis = app
            .get_component::<Redis>()
            .expect("redis component load failed");

        let storage: Arc<dyn AuthStorage> = Arc::new(RedisStorage::new(redis));
        let manager = SessionManager::new(storage, config);
        app.add_component(manager);

        tracing::info!("SessionManager registered");
    }

    fn name(&self) -> &str {
        "summer_auth::SummerAuthPlugin"
    }

    fn dependencies(&self) -> Vec<&str> {
        vec!["summer_redis::RedisPlugin"]
    }
}
