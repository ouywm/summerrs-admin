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
use crate::middleware::AuthLayer;
use crate::path_auth::{PathAuthConfig, RouteRule};
use crate::public_routes::{iter_public_routes, public_routes_in_group};
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
        // - 若未提供 `PathAuthConfig` 组件：默认 `include("/**")`（全部路径鉴权，group=None 走旧全局挂载）
        // - 合并通过 inventory 收集的 `#[public]` / `#[no_auth]` 路由到 `exclude`
        let mut cfg = app.get_component::<PathAuthConfig>().unwrap_or_else(|| {
            tracing::info!("PathAuthConfig not configured, defaulting to include('/**')");
            PathAuthConfig::new(vec![RouteRule::any("/**")], vec![])
        });

        // 合并公开路由（来自 `#[public]` / `#[no_auth]`）
        //   - 有 group：仅合并属于本 group 的条目 + 空 group 的老条目（兼容旧宏展开）
        //   - 无 group：沿用旧全量合并
        let public_iter: Vec<_> = match cfg.group {
            Some(name) => {
                let mut v = public_routes_in_group(name);
                v.extend(public_routes_in_group(""));
                v
            }
            None => iter_public_routes().into_iter().collect(),
        };

        for r in public_iter {
            let rule = RouteRule::new(r.method, r.pattern.to_string());
            if !cfg.exclude.contains(&rule) {
                cfg.exclude.push(rule);
            }
        }
        // 公开路由可能包含 `{param}`，合并完后重建一次 matchit 缓存（启动期一次性完成）
        cfg.rebuild_param_route_cache();

        match cfg.group {
            Some(group_name) => {
                tracing::info!(
                    group = group_name,
                    "Registering GroupAuthLayer for JWT strategy"
                );
                let strategy = JwtStrategy::new(manager, Some(cfg), group_name);
                app.add_group_layer(group_name, move |router| {
                    router.layer(GroupAuthLayer::new(strategy.clone()))
                });
            }
            None => {
                tracing::info!("Registering AuthLayer middleware (global, legacy mode)");
                let path_config = Some(cfg);
                app.add_router_layer(move |router| {
                    router.layer(AuthLayer::new(manager.clone(), path_config.clone()))
                });
            }
        }

        tracing::info!("summer-auth plugin initialized successfully");
    }

    fn name(&self) -> &str {
        "summer_auth::SummerAuthPlugin"
    }

    fn dependencies(&self) -> Vec<&str> {
        vec!["summer_redis::RedisPlugin"]
    }
}
