use std::sync::Arc;

use summer::app::AppBuilder;
use summer::async_trait;
use summer::config::ConfigRegistry;
use summer::plugin::{ComponentRegistry, MutableComponentRegistry, Plugin};
use summer_redis::Redis;
use summer_web::LayerConfigurator;

use crate::config::AuthConfig;
use crate::middleware::AuthLayer;
use crate::path_auth::{PathAuthConfig, RouteRule};
use crate::public_routes::iter_public_routes;
use crate::session::SessionManager;
use crate::storage::AuthStorage;
use crate::storage::redis::RedisStorage;

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
            .expect("redis component load failed");

        let storage: Arc<dyn AuthStorage> = Arc::new(RedisStorage::new(redis));

        let manager = SessionManager::new(storage, config);
        app.add_component(manager.clone());

        // 获取鉴权路径配置
        // - 若没有提供 `PathAuthConfig` 组件：默认 `include("/**")`（即默认全部需要鉴权）
        // - 合并通过 inventory 收集的 `#[public]` / `#[no_auth]` 路由到 `exclude`
        let mut cfg = app.get_component::<PathAuthConfig>().unwrap_or_else(|| {
            tracing::info!("PathAuthConfig not configured, defaulting to include('/**')");
            PathAuthConfig::new(vec![RouteRule::any("/**")], vec![])
        });

        // 合并公开路由（来自 `#[public]` / `#[no_auth]`）
        for r in iter_public_routes() {
            let rule = RouteRule::new(r.method, r.pattern.to_string());
            if !cfg.exclude.contains(&rule) {
                cfg.exclude.push(rule);
            }
        }
        // 公开路由可能包含 `{param}`，合并完后重建一次 matchit 缓存（启动期一次性完成）
        cfg.rebuild_param_route_cache();

        let path_config = Some(cfg);

        // 注册中间件
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
