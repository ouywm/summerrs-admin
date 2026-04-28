use std::sync::Arc;

use summer::app::AppBuilder;
use summer::async_trait;
use summer::config::ConfigRegistry;
use summer::plugin::{ComponentRegistry, MutableComponentRegistry, Plugin};
use summer_redis::Redis;
use summer_web::LayerConfigurator;

use crate::config::AuthConfig;
use crate::group_layer::GroupAuthLayer;
use crate::jwt_strategy::JwtStrategy;
use crate::path_auth::{PathAuthConfigs, RouteRule};
use crate::public_routes::public_routes_in_group;
use crate::session::SessionManager;
use crate::storage::AuthStorage;
use crate::storage::redis::RedisStorage;

pub struct SummerAuthPlugin {
    group: &'static str,
}

impl SummerAuthPlugin {
    pub fn new(group: &'static str) -> Self {
        Self { group }
    }
}

#[async_trait]
impl Plugin for SummerAuthPlugin {
    async fn build(&self, app: &mut AppBuilder) {
        let config = app
            .get_config::<AuthConfig>()
            .expect("auth plugin config load failed");

        tracing::info!("Initializing summer-auth plugin...");

        let redis: Redis = app
            .get_component::<Redis>()
            .expect("redis component load failed");

        let storage: Arc<dyn AuthStorage> = Arc::new(RedisStorage::new(redis));

        let manager = SessionManager::new(storage, config);
        app.add_component(manager.clone());

        // 获取路径认证配置
        let mut configs = app
            .get_component::<PathAuthConfigs>()
            .expect("PathAuthConfigs not found, please call .auth_configure() first");

        let mut cfg = configs
            .get_mut(self.group)
            .unwrap_or_else(|| panic!("PathAuthConfig for group '{}' not found", self.group))
            .clone();

        // 合并公开路由
        let public_iter = public_routes_in_group(self.group);
        for r in public_iter {
            let rule = RouteRule::new(r.method, r.pattern.to_string());
            if !cfg.exclude.contains(&rule) {
                cfg.exclude.push(rule);
            }
        }
        cfg.rebuild_param_route_cache();

        tracing::info!(
            group = self.group,
            "Registering GroupAuthLayer for JWT strategy"
        );
        let strategy = JwtStrategy::new(manager, Some(cfg), self.group);
        app.add_group_layer(self.group, move |router| {
            router.layer(GroupAuthLayer::new(strategy.clone()))
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
