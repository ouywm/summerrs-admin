use axum_client_ip::ClientIpSource;
use std::sync::Arc;
use summer::app::AppBuilder;
use summer::async_trait;
use summer::config::ConfigRegistry;
use summer::plugin::{ComponentRegistry, MutableComponentRegistry, Plugin};
use summer_auth::path_auth::PathAuthConfig;
use summer_auth::storage::AuthStorage;
use summer_auth::storage::redis::RedisStorage;
use summer_auth::{AuthConfig, AuthLayer, SessionManager};
use summer_redis::Redis;
use summer_web::LayerConfigurator;

use crate::router::SystemAdminRouteGroup;

pub struct SystemAdminAuthRouterPlugin;

#[async_trait]
impl Plugin for SystemAdminAuthRouterPlugin {
    async fn build(&self, app: &mut AppBuilder) {
        tracing::info!("Initializing system admin auth router plugin...");

        let config = app
            .get_config::<AuthConfig>()
            .expect("system admin auth config load failed");

        let admin_route_group = app
            .get_component::<SystemAdminRouteGroup>()
            .expect("system admin route group component load failed");

        let redis = app
            .get_component::<Redis>()
            .expect("redis component load failed for system admin auth");

        // 创建认证存储后端并初始化会话管理器
        let storage: Arc<dyn AuthStorage> = Arc::new(RedisStorage::new(redis));
        let manager = SessionManager::new(storage, config);
        app.add_component(manager.clone());

        let path_config = admin_auth_path_config();
        app.add_component(path_config.clone());

        tracing::info!("Registering system admin router merge layer with AuthLayer");
        app.add_router_layer(move |router| {
            router
                .merge(
                    admin_route_group
                        .0
                        .clone()
                        .layer(AuthLayer::new(manager.clone(), Some(path_config.clone()))),
                )
                .layer(ClientIpSource::ConnectInfo.into_extension())
        });

        tracing::info!("system admin auth router plugin initialized successfully");
    }

    fn name(&self) -> &str {
        "summer_system::SystemAdminAuthRouterPlugin"
    }

    fn dependencies(&self) -> Vec<&str> {
        vec!["summer_redis::RedisPlugin"]
    }
}

fn admin_auth_path_config() -> PathAuthConfig {
    PathAuthConfig {
        include: vec!["/**".to_string()],
        exclude: vec!["/auth/login".to_string(), "/auth/refresh".to_string()],
    }
}

#[cfg(test)]
mod tests {
    use super::admin_auth_path_config;

    #[test]
    fn admin_auth_paths_only_exclude_login_and_refresh() {
        let config = admin_auth_path_config();

        assert!(!config.requires_auth("/auth/login"));
        assert!(!config.requires_auth("/auth/refresh"));
        assert!(config.requires_auth("/auth/logout"));
        assert!(config.requires_auth("/sys/user/list"));
    }
}
