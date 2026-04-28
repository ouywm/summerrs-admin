//! SummerAiAdminPlugin —— admin 域 Plugin 入口。
//!
//! 职责：
//! - 给 "summer-ai-admin" 路由组挂 `GroupAuthLayer::new(JwtStrategy)`
//!   （通过 `add_group_layer` —— 只套在本 crate 的 handler 上，不影响其他 crate）
//!
//! handler 通过 `#[post("...", group = "summer-ai-admin")]` 宏自动注册到 inventory。
//!
//! 依赖 `summer_auth::SummerAuthPlugin`（提供 SessionManager）。

use summer::app::AppBuilder;
use summer::async_trait;
use summer::plugin::{ComponentRegistry, Plugin};
use summer_auth::GroupAuthLayer;
use summer_auth::jwt_strategy::JwtStrategy;
use summer_auth::path_auth::{PathAuthConfigs, RouteRule};
use summer_auth::public_routes::public_routes_in_group;
use summer_auth::session::SessionManager;
use summer_web::LayerConfigurator;

/// admin 域 Plugin 入口。
pub struct SummerAiAdminPlugin;

#[async_trait]
impl Plugin for SummerAiAdminPlugin {
    async fn build(&self, app: &mut AppBuilder) {
        let _manager = app
            .get_component::<SessionManager>()
            .expect("summer-ai-admin requires SessionManager (SummerAuthPlugin)");

        let mut configs = app
            .get_component::<PathAuthConfigs>()
            .expect("PathAuthConfigs not found, please call .auth_configure() first");

        let group = Self::admin_group();
        let mut cfg = configs
            .get_mut(group)
            .unwrap_or_else(|| panic!("PathAuthConfig for group '{}' not found", group))
            .clone();

        // 合并公开路由
        let public_iter = public_routes_in_group(group);
        for r in public_iter {
            let rule = RouteRule::new(r.method, r.pattern.to_string());
            if !cfg.exclude.contains(&rule) {
                cfg.exclude.push(rule);
            }
        }
        cfg.rebuild_param_route_cache();

        tracing::info!(group = group, "Registering GroupAuthLayer for JWT strategy");
        let strategy = JwtStrategy::new(Some(cfg), group);
        app.add_group_layer(group, move |router| {
            router.layer(GroupAuthLayer::new(strategy.clone()))
        });

        tracing::info!(
            group = group,
            "summer-ai-admin plugin initialized (JwtStrategy bound)"
        );
    }

    fn name(&self) -> &str {
        "summer_ai_admin::SummerAiAdminPlugin"
    }

    fn dependencies(&self) -> Vec<&str> {
        vec!["summer_auth::SummerAuthPlugin"]
    }
}

impl SummerAiAdminPlugin {
    pub fn admin_group() -> &'static str {
        env!("CARGO_PKG_NAME")
    }
}
