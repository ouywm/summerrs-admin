//! SummerAiRelayPlugin —— relay 域 Plugin 入口。
//!
//! 职责：
//! - 注册 `reqwest::Client`（出站 HTTP 客户端）作 component
//! - 构造 `AiTokenStore` 作 component
//! - 给 `"summer-ai-relay"` 路由组挂 `GroupAuthLayer::new(ApiKeyStrategy)`
//!   （通过 `add_group_layer` —— 只套在本 crate 的 handler 上，不影响其他 crate）
//!
//! `ChannelStore` / `TrackingService` 都用 `#[derive(Service)]`，summer 框架的
//! `auto_inject_service` 会在 app 启动时自动从 inventory 里把它们装进 registry,
//! 并按 `#[inject(component)]` 字段拉取依赖（`DbConn` / `Redis` / `BackgroundTaskQueue`）。
//!
//! handler 通过 `#[post("/...", group = "summer-ai-relay")]` 宏自动注册到 inventory。
//!
//! 依赖 `summer_sea_orm::SeaOrmPlugin` + `summer_redis::RedisPlugin` +
//! `summer_plugins::BackgroundTaskPlugin`（注入源）。

use std::time::Duration;

use summer::app::AppBuilder;
use summer::async_trait;
use summer::plugin::{ComponentRegistry, MutableComponentRegistry, Plugin};
use summer_auth::GroupAuthLayer;
use summer_redis::Redis;
use summer_sea_orm::DbConn;
use summer_web::LayerConfigurator;

use crate::auth::{AiTokenStore, ApiKeyStrategy};

/// relay 域 Plugin 入口。
pub struct SummerAiRelayPlugin;

#[async_trait]
impl Plugin for SummerAiRelayPlugin {
    async fn build(&self, app: &mut AppBuilder) {
        let http = match reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
        {
            Ok(client) => client,
            Err(error) => {
                tracing::error!(?error, "summer-ai-relay reqwest client build failed");
                return;
            }
        };
        app.add_component(http);

        let db = app
            .get_component::<DbConn>()
            .expect("summer-ai-relay requires DbConn (SeaOrmPlugin)");
        let redis = app
            .get_component::<Redis>()
            .expect("summer-ai-relay requires Redis (RedisPlugin)");

        let token_store = AiTokenStore::new(db, redis);
        app.add_component(token_store.clone());

        let strategy = ApiKeyStrategy::new(token_store, Self::relay_group());
        app.add_group_layer(Self::relay_group(), move |r| {
            r.layer(GroupAuthLayer::new(strategy.clone()))
        });

        tracing::info!(
            group = Self::relay_group(),
            "summer-ai-relay plugin initialized (ApiKeyStrategy bound)"
        );
    }

    fn name(&self) -> &str {
        "summer_ai_relay::SummerAiRelayPlugin"
    }

    fn dependencies(&self) -> Vec<&str> {
        vec![
            "summer_sea_orm::SeaOrmPlugin",
            "summer_redis::RedisPlugin",
            "background-task",
        ]
    }
}

impl SummerAiRelayPlugin {
    /// group 名——与 handler 的 `#[post("...", group = "summer-ai-relay")]` 对齐。
    pub fn relay_group() -> &'static str {
        env!("CARGO_PKG_NAME")
    }
}
