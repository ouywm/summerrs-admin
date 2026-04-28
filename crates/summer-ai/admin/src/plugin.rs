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
use summer_auth::session::SessionManager;
use summer_web::LayerConfigurator;

/// admin 域 Plugin 入口。
pub struct SummerAiAdminPlugin;

#[async_trait]
impl Plugin for SummerAiAdminPlugin {
    async fn build(&self, app: &mut AppBuilder) {
        // 获取 SessionManager（已在 summer-auth plugin 中注册）
        let manager = app
            .get_component::<SessionManager>()
            .expect("summer-ai-admin requires SessionManager (SummerAuthPlugin)");

        // 创建 JWT 策略
        let strategy = JwtStrategy::new(
            manager,
            None,                // 使用默认路径配置（全量鉴权）
            Self::admin_group(), // 使用 "summer-ai-admin"
        );

        // 注册 GroupAuthLayer
        app.add_group_layer(Self::admin_group(), move |router| {
            router.layer(GroupAuthLayer::new(strategy.clone()))
        });

        tracing::info!(
            group = Self::admin_group(),
            "summer-ai-admin plugin initialized (JwtStrategy bound)"
        );
    }

    fn name(&self) -> &str {
        "summer_ai_admin::SummerAiAdminPlugin"
    }

    fn dependencies(&self) -> Vec<&str> {
        vec!["summer_auth::SummerAuthPlugin"] // 确保在 auth plugin 之后执行
    }
}

impl SummerAiAdminPlugin {
    pub fn admin_group() -> &'static str {
        env!("CARGO_PKG_NAME")
    }
}
